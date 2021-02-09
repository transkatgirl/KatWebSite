#![warn(clippy::all)]

use actix_web::{get, web, guard, body::{Body, ResponseBody}, dev::ServiceResponse, http::{header, header::{ContentType, IntoHeaderValue}, Method, StatusCode}, middleware::{Compress, Logger, DefaultHeaders, NormalizePath, TrailingSlash, ErrorHandlers, ErrorHandlerResponse}, App, HttpRequest, HttpResponse, HttpServer, Responder};
use std::{process, fs, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}};
use serde_derive::Deserialize;
use log::{trace, warn, debug, error, log_enabled, info, Level, LevelFilter};

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
struct Server {
	http_bind: Vec<SocketAddr>,
	log_format: String,
}

// TODO:
// - finish implementing web server
//   - implement form handling
//   - implement http auth
//   - implement https + hsts
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
		process::exit(exitcode::IOERR);
	});
	trace!("parsing config.toml as toml");
	let config: Config = toml::from_str(&config_data).unwrap_or_else(|err| {
		error!("Unable to parse config file! {}", err);
		process::exit(exitcode::CONFIG);
	});

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

	info!("Loaded configuration.");

	trace!("starting HttpServer");
	server.run().await.unwrap_or_else(|err| {
			error!("Unable to start server! {}", err);
			process::exit(exitcode::OSERR);
	});
	trace!("HttpServer execution has stopped");
}
