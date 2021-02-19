#![warn(clippy::all)]

use actix_web::{web, guard, http::{header, StatusCode}, middleware::{Compress, Logger, DefaultHeaders, NormalizePath, TrailingSlash}, App, Scope, HttpRequest, HttpResponse, HttpServer};
use clap::Clap;
use log::{trace, warn, debug, error, info};
use rustls::{NoClientAuth, ServerConfig, ResolvesServerCertUsingSNI, sign, sign::CertifiedKey, PrivateKey, Certificate};
use serde_derive::Deserialize;
use std::{process, iter, fs, fs::File, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}, io::BufReader, sync::Arc, error::Error, boxed::Box};

/// A minimal static site generator and web server.
#[derive(Clap,Debug)]
#[clap(version = "0.1.0")]
struct Opts {
	/// Specifies the configuration file to load.
	#[clap(short, long, default_value = "config.toml")]
	config: String,

	/// Decreases log verbosity, ignored if RUST_LOG is set. Minimum possible log verbosity is -2.
	#[clap(short, long, parse(from_occurrences))]
	quiet: i32,

	/// Increases log verbosity, ignored if RUST_LOG is set. Maximum possible log verbosity is 3.
	#[clap(short, long, parse(from_occurrences))]
	verbose: i32,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Config {
	vhost: Vec<Vhost>,

	#[serde(default)]
	headers: BTreeMap<String, String>,

	server: Option<Server>,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Vhost {
	host: String,

	#[serde(default)]
	files: Vec<Files>,

	#[serde(default)]
	redir: Vec<Redir>,

	tls: Option<Tls>,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Files {
	#[serde(default)]
	mount: String,

	file_dir: PathBuf,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Redir {
	#[serde(default)]
	target: String,

	dest: String,

	#[serde(default)]
	permanent: bool,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Tls {
	pemfiles: Vec<PathBuf>,
	http_dest: Option<String>,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Server {
	#[serde(default)]
	http_bind: Vec<SocketAddr>,

	#[serde(default)]
	tls_bind: Vec<SocketAddr>,

	#[serde(default = "default_server_log_format")]
	log_format: String,
}

fn default_server_log_format() -> String {
	"%{Host}i %a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %D".to_string()
}

// TODO:
// - implement page generation
//   - katsite code may be useful as a reference
// - separate code into multiple files

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

fn handle_https_redirect(req: HttpRequest, dest: web::Data<String>) -> HttpResponse {
	HttpResponse::PermanentRedirect()
		.append_header((header::LOCATION, [dest.as_str(), req.path()].concat()))
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
	if is_tls && vhost.tls.is_none() {
		return None
	}

	let mut scope = web::scope("/")
		.guard(guard::Host(String::from(&vhost.host)));

	// https://github.com/rust-lang/rust/issues/53667
	if let Some(Tls{ http_dest: Some(dest), ..}) = &vhost.tls {
		if !is_tls {
			return Some(
				scope.data(dest.to_owned()).default_service(web::to(handle_https_redirect))
			)
		}
	}

	for redir in vhost.redir.to_owned() {
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
	for files in vhost.files.to_owned() {
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
	let opts: Opts = Opts::parse();

	let logstr = match opts.verbose - opts.quiet {
		i32::MIN...-2 => "error",
		-1 => "warn, actix_web::middleware::logger = info",
		0 => "info, actix_server::accept = warn",
		1 => "debug, actix_server::accept = warn",
		2 => "trace, actix_web::middleware::logger = debug, rustls = debug, actix_server::accept = warn",
		3...i32::MAX => "trace",
	};

	flexi_logger::Logger::with_env_or_str(logstr)
		.start().unwrap();

	trace!("started logger with default RUST_LOG set to {:?}", logstr);

	info!("Loading configuration");

	trace!("reading {}", opts.config);
	let config_data = fs::read_to_string(&opts.config).unwrap_or_else(|err| {
		error!("Unable to read config file! {}", err);
		process::exit(exitcode::NOINPUT);
	});
	debug!("parsing {} as toml configuration", opts.config);
	let config: Config = toml::from_str(&config_data).unwrap_or_else(|err| {
		error!("Unable to parse config file! {}", err);
		process::exit(exitcode::CONFIG);
	});

	let serverconfig = match config.server {
		Some(ref server) => server,
		None => {
			trace!("no http server specified, exiting early");
			return
		},
	};

	debug!("initializing HttpServer");
	let conf = config.to_owned();
	let serverconf = serverconfig.to_owned();
	let appbuilder = move |is_tls| {
		let mut headers = DefaultHeaders::new();
		for (key, val) in &conf.headers {
			headers = headers.header(key, val);
		}

		let mut app = App::new()
			.wrap(Logger::new(&serverconf.log_format))
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

	for addr in &serverconfig.http_bind {
		server = server.bind(addr).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}
	let futureserver = server.run();

	if !serverconfig.tls_bind.is_empty() {
		debug!("loading tls certificates");
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

		for addr in &serverconfig.tls_bind {
			servertls = servertls.bind_rustls(addr, tlsconf.to_owned()).unwrap_or_else(|err| {
				error!("Unable to bind to port! {}", err);
				process::exit(exitcode::OSERR);
			})
		}

		servertls.run();
	}

	futureserver.await.unwrap_or_else(|err| {
		error!("Unable to start server! {}", err);
		process::exit(exitcode::OSERR);
	});
	debug!("HttpServer execution has stopped");
}
