use actix_web::{get, web, guard, body::{Body, ResponseBody}, dev::ServiceResponse, http::{header, header::{ContentType, IntoHeaderValue}, HeaderValue, Method, StatusCode}, middleware::{Compress, Logger, DefaultHeaders, NormalizePath, TrailingSlash, ErrorHandlers, ErrorHandlerResponse}, App, HttpRequest, HttpResponse, HttpServer, Responder};
use std::{process, fs, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}};
use serde_derive::Deserialize;
use log::{trace, warn, debug, error, log_enabled, info, Level, LevelFilter};

#[derive(Deserialize,Clone,Debug)]
struct Config {
	files: Files,
	server: Server,
}

#[derive(Deserialize,Clone,Debug)]
struct Files {
	root_dir: PathBuf,
}

#[derive(Deserialize,Clone,Debug)]
struct Server {
	http_bind: Option<Vec<SocketAddr>>,
}

// TODO:
// - finish implementing web server
//   - consider implementing file listing
//   - implement configurable redirects
//   - implement form handling
//   - implement http auth
//   - implement https + hsts
//   - implement http reverse proxy
// - implement page generation
//   - katsite code may be useful as a reference

fn render_error<B>(mut res: ServiceResponse<B>) -> actix_web::Result<ErrorHandlerResponse<B>> {
	let status = res.status();

	res.response_mut()
		.headers_mut()
		.insert(header::CONTENT_TYPE, ContentType::html().try_into().unwrap());

	res = res.map_body::<_, B>(|_, _| {
		ResponseBody::Other(format!(
			"<h2>Error HTTP {}</h2><p>{}</p>",
			status.as_u16(),
			status.canonical_reason().unwrap_or("")
		).into())
	});

	Ok(ErrorHandlerResponse::Response(res))
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

	trace!("opening root directory");
	let htmldir = config.files.root_dir.read_dir().unwrap_or_else(|err| {
		error!("Unable to open root directory! {}", err);
		process::exit(exitcode::IOERR);
	});

	trace!("iterating over root directory's contents");
	let mut vhosts = Vec::new();
	for subfolder in htmldir {
		if let Ok(vhost) = subfolder {
			vhosts.push(vhost.path())
		}
	}
	trace!("detected vhosts: {:?}", vhosts);


	info!("Loaded configuration.");


	trace!("configuring HttpServer");
	let conf = config.to_owned();
	let mut server = HttpServer::new(move || {
		trace!("generating application builder");

		let mut app = App::new() // TODO: Add actix_web::middleware::errhandlers::ErrorHandlers middleware
			.wrap(Logger::new("%{Host}i %a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %D"))
			.wrap(NormalizePath::new(TrailingSlash::MergeOnly))
			.wrap(
				ErrorHandlers::new()
					.handler(StatusCode::NOT_FOUND, render_error)
			)
			.wrap(Compress::default())
			.data(conf.to_owned())
			.default_service(
				actix_files::Files::new("", conf.files.root_dir.join("default"))
					.index_file("index.html")
					.prefer_utf8(true)
					.disable_content_disposition()
			);
	
		for vhost in &vhosts {
			app = app.service(
				web::scope("/")
					.guard(guard::Host(
						String::from(vhost.file_name().unwrap().to_string_lossy())
					))
					.service(
						actix_files::Files::new("", vhost)
							.index_file("index.html")
							.prefer_utf8(true)
							.disable_content_disposition()
					)
			)
		}

		app/*.service(
			web::scope("/")
				.guard(guard::Post())
				.service(handle_post)

		)*/
	});

	trace!("adding port bindings");
	for addr in config.server.http_bind.unwrap_or_else(|| vec![]) {
		server = server.bind(addr).unwrap_or_else(|err| {
			error!("Unable to bind to port! {}", err);
			process::exit(exitcode::OSERR);
		})
	}

	trace!("starting HttpServer");
	server.run().await.unwrap_or_else(|err| {
			error!("Unable to start server! {}", err);
			process::exit(exitcode::OSERR);
	});
	trace!("HttpServer execution has stopped");
}
