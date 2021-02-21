#![warn(clippy::all)]

use clap::Clap;
use futures::try_join;
use log::{trace, warn, debug, error, info};
use serde_derive::Deserialize;
use std::{process, fs};

mod http;
mod builder;

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
	#[serde(default)]
	builder: Vec<builder::Builder>,

	#[serde(default)]
	vhost: Vec<http::Vhost>,

	#[serde(default)]
	headers: http::Headers,

	#[serde(default)]
	server: http::Server,
}

#[actix_web::main]
async fn main() {
	let opts: Opts = Opts::parse();

	let logstr = match opts.verbose - opts.quiet {
		i32::MIN...-2 => "error",
		-1 => "warn, actix_web::middleware::logger = info",
		0 => "info, actix_server::accept = warn, actix_server::builder = warn",
		1 => "debug, actix_server::accept = warn",
		2 => "trace, actix_web::middleware::logger = debug, rustls = debug, actix_server::accept = warn",
		3...i32::MAX => "trace",
	};

	flexi_logger::Logger::with_env_or_str(logstr)
		.start().unwrap_or_else(|err| {
			eprintln!("Unable to start logger! {}", err);
			process::exit(exitcode::UNAVAILABLE);
	});

	trace!("started logger with default RUST_LOG set to {:?}", logstr);

	info!("Loading configuration");

	debug!("parsing {} as toml configuration", opts.config);
	let config_data = fs::read_to_string(&opts.config).unwrap_or_else(|err| {
		error!("Unable to read config file! {}", err);
		process::exit(exitcode::NOINPUT);
	});
	let config: Config = toml::from_str(&config_data).unwrap_or_else(|err| {
		error!("Unable to parse config file! {}", err);
		process::exit(exitcode::CONFIG);
	});

	match config.builder.is_empty() {
		true => {
			debug!("no page builder specified, skipping")
		},
		false => (
			info!("Generating site pages")
		),
	}
	for builder in &config.builder {
		builder::run_builder(builder).unwrap_or_else(|err| {
			error!("Unable to run builder for {:?}! {}", builder.input_glob, err);
			process::exit(exitcode::DATAERR);
		})
	}

	let http_server = http::run_http_server(false, &config.server, &config.headers, &config.vhost).unwrap_or_else(|err| {
		error!("Unable to configure HTTP server! {}", err);
		process::exit(exitcode::CONFIG);
	});
	let https_server = http::run_http_server(true, &config.server, &config.headers, &config.vhost).unwrap_or_else(|err| {
		error!("Unable to configure HTTPS server! {}", err);
		process::exit(exitcode::CONFIG);
	});
	try_join!(http_server, https_server).unwrap_or_else(|err| {
		error!("Unable to start server! {}", err);
		process::exit(exitcode::OSERR);
	});

	debug!("HttpServer execution has stopped");
}
