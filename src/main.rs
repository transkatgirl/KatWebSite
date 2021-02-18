#![warn(clippy::all)]

use actix_web::{web, guard, http::{header, StatusCode}, middleware::{Compress, Logger, DefaultHeaders, NormalizePath, TrailingSlash}, App, Scope, HttpRequest, HttpResponse, HttpServer};
use std::{process, iter, fs, fs::File, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}, io::BufReader, sync::Arc, error::Error, boxed::Box};
use serde_derive::Deserialize;
use log::{trace, warn, debug, error, info, LevelFilter};
use rustls::{NoClientAuth, ServerConfig, ResolvesServerCertUsingSNI, sign, sign::CertifiedKey, PrivateKey, Certificate};

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Config {
	vhost: Vec<Vhost>,
	headers: BTreeMap<String, String>,
	server: Server,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Vhost {
	host: String,
	protocols: Vec<String>,
	files: Option<Vec<Files>>,
	redir: Option<Vec<Redir>>,
	tls: Option<Tls>,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Files {
	mount: String,
	file_dir: PathBuf,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Redir {
	target: String,
	dest: String,
	permanent: bool,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Tls {
	pemfiles: Vec<PathBuf>,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Server {
	http_bind: Vec<SocketAddr>,
	tls_bind: Vec<SocketAddr>,
	log_format: String,
}

// TODO:
// - implement page generation
//   - katsite code may be useful as a reference

fn handle_not_found() -> HttpResponse {
	HttpResponse::NotFound()
		.content_type("text/html; charset=utf-8")
		.body("<!DOCTYPE html><h3 style=\"font: 20px sans-serif; margin: 12px\">The requested resource could not be found.</h3>")
}

fn handle_redirect(req: HttpRequest, status: web::Data<StatusCode>, dest: web::Data<String>) -> HttpResponse {
	let mut dest = dest.to_string();
	for (_, segment) in req.match_info().iter() {
		dest = [&dest,"/",segment].concat()
	}

	HttpResponse::build(*status.as_ref())
		.append_header((header::LOCATION, dest))
		.finish()
}

fn create_certified_key(pemfiles: &[PathBuf]) -> Result<CertifiedKey, Box<dyn Error>> {
	let mut certs = Vec::new();
	let mut keys = Vec::new();
	for pemfile in pemfiles {
		let mut reader = BufReader::new(File::open(pemfile)?);
		for item in iter::from_fn(|| rustls_pemfile::read_one(&mut reader).transpose()) {
			match item? {
				rustls_pemfile::Item::X509Certificate(cert) => certs.push(Certificate(cert)),
				rustls_pemfile::Item::PKCS8Key(key) => keys.push(PrivateKey(key)),
				rustls_pemfile::Item::RSAKey(key) => keys.push(PrivateKey(key)),
			}
		}
	}

	let key = keys.get(0).ok_or("no valid keys found")?;
	let signingkey = sign::any_supported_type(key).or(Err("unable to parse key"))?;

	Ok(CertifiedKey::new(certs, Arc::new(signingkey)))
}

fn configure_vhost_scope(vhost: &Vhost, is_tls: bool) -> Option<Scope> {
	let is_active = match is_tls {
		true => vhost.protocols.contains(&"https".to_string()),
		false => vhost.protocols.contains(&"http".to_string()),
	};

	if !is_active {
		return None
	}

	let mut scope = web::scope("/")
		.guard(guard::Host(String::from(&vhost.host)));

	for redir in vhost.redir.to_owned().unwrap_or_default() {
		let status = match redir.permanent {
			true => StatusCode::PERMANENT_REDIRECT,
			false => StatusCode::TEMPORARY_REDIRECT,
		};
		let target = match redir.target.as_ref() {
			"/" => "",
			_ => &redir.target,
		};
		scope = scope.service(
			web::resource(target)
				.data(status).data(redir.dest)
				.to(handle_redirect)
		)
	}

	// Potentially useful future features:
	// - https://github.com/actix/actix-web/issues/1718
	// - https://github.com/actix/actix-web/issues/2000
	for files in vhost.files.to_owned().unwrap_or_default() {
		let mount = match files.mount.as_ref() {
			"/" => "",
			_ => &files.mount,
		};
		scope = scope.service(
			actix_files::Files::new(mount, &files.file_dir)
				.index_file("index.html")
				.prefer_utf8(true)
				.disable_content_disposition()
		)
	}
	
	Some(scope)
}

#[actix_web::main]
async fn main() {
	env_logger::Builder::new()
		.filter_level(LevelFilter::Info)
		.format_timestamp(Some(env_logger::fmt::TimestampPrecision::Millis))
		.parse_default_env()
		.init();

	trace!("reading config.toml");
	let config_data = fs::read_to_string("config.toml").unwrap_or_else(|err| {
		error!("Unable to read config file! {}", err);
		process::exit(exitcode::NOINPUT);
	});
	trace!("parsing config.toml as toml");
	let config: Config = toml::from_str(&config_data).unwrap_or_else(|err| {
		error!("Unable to parse config file! {}", err);
		process::exit(exitcode::CONFIG);
	});

	trace!("loading tls certificates");
	let mut resolver = ResolvesServerCertUsingSNI::new();
	for vhost in &config.vhost {
		if let Some(tls) = &vhost.tls {
			let keypair = create_certified_key(&tls.pemfiles).unwrap_or_else(|err| {
				error!("Unable to load certificate pair for {}! {}", vhost.host, err);
				process::exit(exitcode::DATAERR);
			});
			resolver.add(&vhost.host, keypair).unwrap_or_else(|err| {
				error!("Unable to configure certificate pair for {}! {}", vhost.host, err);
				process::exit(exitcode::DATAERR);
			});
		}
	}
	let mut tlsconf = ServerConfig::new(NoClientAuth::new());
	tlsconf.cert_resolver = Arc::new(resolver);

	trace!("configuring HttpServer");
	let conf = config.to_owned();
	let appbuilder = move |is_tls| {
		let mut headers = DefaultHeaders::new();
		for (key, val) in &conf.headers {
			headers = headers.header(key, val);
		}

		let mut app = App::new()
			.wrap(Logger::new(&conf.server.log_format))
			.wrap(headers)
			.wrap(NormalizePath::new(TrailingSlash::Trim))
			.wrap(Compress::default())
			.default_service(web::route().to(handle_not_found));

		for vhost in &conf.vhost {
			app = match configure_vhost_scope(vhost, is_tls) {
				Some(scope) => app.service(scope),
				None => app,
			};
		}

		app
	};
	let appbuilderr = appbuilder.to_owned();

	let mut server = HttpServer::new(move || {
		trace!("generating http application builder");
		appbuilder(false)
	});

	let mut servertls = HttpServer::new(move || {
		trace!("generating https application builder");
		appbuilderr(true)
	});

	trace!("adding port bindings");
	for addr in config.server.http_bind {
		server = server.bind(addr).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}
	for addr in &config.server.tls_bind {
		servertls = servertls.bind_rustls(addr, tlsconf.to_owned()).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}

	info!("Loaded configuration.");

	trace!("starting HttpServer");
	if !config.server.tls_bind.is_empty() {
		servertls.run();
	} else {
		debug!("tls server has no port bindings, skipping");
	}
	server.run().await.unwrap_or_else(|err| {
		error!("Unable to start server! {}", err);
		process::exit(exitcode::OSERR);
	});
	trace!("HttpServer execution has stopped");
}
