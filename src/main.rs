#![warn(clippy::all)]

use actix_web::{get, web, guard, body::{Body, ResponseBody}, dev::ServiceResponse, http::{header, header::{ContentType, IntoHeaderValue}, Method, StatusCode}, middleware::{Compress, Logger, DefaultHeaders, NormalizePath, TrailingSlash, ErrorHandlers, ErrorHandlerResponse}, App, HttpRequest, HttpResponse, HttpServer, Responder};
use std::{process, fs, fs::File, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}, io::BufReader, sync::Arc};
use serde_derive::Deserialize;
use log::{trace, warn, debug, error, log_enabled, info, Level, LevelFilter};
use rustls::{NoClientAuth, ServerConfig, ResolvesServerCertUsingSNI, sign, sign::CertifiedKey, internal::pemfile};

#[derive(Deserialize,Clone,Debug)]
struct Config {
	vhost: Vec<Vhost>,
	headers: BTreeMap<String, String>,
	server: Server,
}

#[derive(Deserialize,Clone,Debug)]
struct Vhost {
	host: String,
	files: Vec<Files>,
	redir: Vec<Redir>,
	tls: Option<Tls>,
}

#[derive(Deserialize,Clone,Debug)]
struct Files {
	mount: String,
	file_dir: PathBuf,
}

#[derive(Deserialize,Clone,Debug)]
struct Redir {
	target: String,
	dest: String,
	permanent: bool,
}

#[derive(Deserialize,Clone,Debug)]
struct Tls {
	cert_file: PathBuf,
	key_file: PathBuf,
	//hsts: bool,
}

#[derive(Deserialize,Clone,Debug)]
struct Server {
	http_bind: Vec<SocketAddr>,
	tls_bind: Vec<SocketAddr>,
	log_format: String,
}

// TODO:
// - finish implementing web server
//   - implement form handling
//   - implement http auth
//   - implement hsts
//   - clean up code
//   - implement http reverse proxy
// - implement page generation
//   - katsite code may be useful as a reference

fn handle_not_found() -> HttpResponse {
	HttpResponse::NotFound()
		.content_type("text/html; charset=utf-8")
		.body("<!DOCTYPE html><h3 style=\"font: 20px sans-serif; margin: 12px\">The requested resource could not be found.</h3>")
}

fn handle_redirect(status: web::Data<StatusCode>, dest: web::Data<String>) -> HttpResponse {
	HttpResponse::build(*status.as_ref())
		.header(header::LOCATION, dest.as_str())
		.finish()
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

	trace!("configuring rustls");
	let mut tlsconf = ServerConfig::new(NoClientAuth::new());
	let mut resolver = ResolvesServerCertUsingSNI::new();
	for vhost in &config.vhost {
		if let Some(tls) = &vhost.tls {
			let cert_file = &mut BufReader::new(File::open(&tls.cert_file).unwrap_or_else(|err| {
				error!("Unable to open TLS cert for {}! {}", vhost.host, err);
				process::exit(exitcode::NOINPUT);
			}));
			let key_file = &mut BufReader::new(File::open(&tls.key_file).unwrap_or_else(|err| {
				error!("Unable to open TLS private key for {}! {}", vhost.host, err);
				process::exit(exitcode::NOINPUT);
			}));

			let certs = pemfile::certs(cert_file).unwrap_or_else(|_| {
				error!("Unable to load TLS cert for {}!", vhost.host);
				process::exit(exitcode::DATAERR);
			});
			let keys = pemfile::pkcs8_private_keys(key_file).unwrap_or_else(|_| {
				error!("Unable to load TLS private key for {}!", vhost.host);
				process::exit(exitcode::DATAERR);
			});
			if keys.len() > 1 {
				debug!("more than one TLS key provided for {}", vhost.host)
			}
			let key = keys.get(0).unwrap_or_else(|| {
				error!("No TLS private keys found for {}!", vhost.host);
				process::exit(exitcode::DATAERR);
			});
			let signingkey = sign::any_supported_type(key).unwrap_or_else(|_| {
				error!("Unable to parse TLS private key for {}!", vhost.host);
				process::exit(exitcode::DATAERR);
			});

			resolver.add(&vhost.host, CertifiedKey::new(certs, Arc::new(signingkey))).unwrap_or_else(|err| {
				error!("Unable to configure TLS cert for {}! {}", vhost.host, err);
				process::exit(exitcode::DATAERR);
			});
		}
	}
	tlsconf.cert_resolver = Arc::new(resolver);

	trace!("configuring HttpServer");
	let conf = config.to_owned();
	let mut server = HttpServer::new(move || {
		trace!("generating application builder");

		let mut headers = DefaultHeaders::new();
		for (key, val) in &conf.headers {
			headers = headers.header(key, val);
		}

		let mut app = App::new()
			.wrap(Logger::new(&conf.server.log_format))
			.wrap(headers)
			.wrap(Compress::default())
			.default_service(web::route().to(handle_not_found));
	
		for vhost in &conf.vhost {
			let mut scope = web::scope("/").guard(guard::Host(
				String::from(&vhost.host)
			));

			for redir in vhost.redir.to_owned() {
				let status = match redir.permanent {
					true => StatusCode::PERMANENT_REDIRECT,
					false => StatusCode::TEMPORARY_REDIRECT,
				};
				scope = scope.service(web::resource(redir.target)
					.data(status).data(redir.dest)
					.to(handle_redirect)
				)
			}

			for files in &vhost.files {
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

			app = app.service(scope)
		}

		app/*.service(
			web::scope("/")
				.guard(guard::Post())
				.service(handle_post)

		)*/
	});

	trace!("adding port bindings");
	for addr in config.server.http_bind {
		server = server.bind(addr).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}
	for addr in config.server.tls_bind {
		server = server.bind_rustls(addr, tlsconf.to_owned()).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}

	info!("Loaded configuration.");

	trace!("starting HttpServer");
	server.run().await.unwrap_or_else(|err| {
			error!("Unable to start server! {}", err);
			process::exit(exitcode::OSERR);
	});
	trace!("HttpServer execution has stopped");
}
