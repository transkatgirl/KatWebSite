#![warn(clippy::all)]

use clap::Clap;
use futures::try_join;
use log::{debug, error, info, trace, warn};
use mimalloc::MiMalloc;
use serde_derive::Deserialize;
use std::{env, fs, path::PathBuf, process};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod builder;
mod copier;
mod http;
mod runner;

/// A minimal static site generator and web server.
#[derive(Clap, Debug)]
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

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Config {
	#[serde(default)]
	pre_copier: Vec<copier::Copier>,

	#[serde(default)]
	pre_runner: Vec<runner::Runner>,

	#[serde(default)]
	builder: Vec<builder::Builder>,

	#[serde(default)]
	copier: Vec<copier::Copier>,

	#[serde(default)]
	runner: Vec<runner::Runner>,

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
		1 => "debug, actix_server::accept = warn, html5ever = info",
		2 => "trace, actix_web::middleware::logger = debug, rustls = debug, actix_server::accept = warn, html5ever = info",
		3...i32::MAX => "trace",
	};

	flexi_logger::Logger::with_env_or_str(logstr)
		.start()
		.unwrap_or_else(|err| {
			eprintln!("Unable to start logger! {}", err);
			process::exit(exitcode::SOFTWARE);
		});

	trace!("started logger with default RUST_LOG set to {:?}", logstr);

	info!("Loading configuration");

	debug!("parsing {:?} as toml configuration", opts.config);
	let config_data = fs::read_to_string(&opts.config).unwrap_or_else(|err| {
		error!("Unable to read config file! {}", err);
		process::exit(exitcode::NOINPUT);
	});
	let config: Config = toml::from_str(&config_data).unwrap_or_else(|err| {
		error!("Unable to parse config file! {}", err);
		process::exit(exitcode::CONFIG);
	});

	if let Some(config_path) = PathBuf::from(&opts.config).parent() {
		trace!("setting working directory to {:?}", &config_path);
		env::set_current_dir(config_path).unwrap_or_else(|err| {
			trace!("Unable to change working directory! {}", err);
		})
	}

	if config.builder.is_empty()
		&& config.copier.is_empty()
		&& config.pre_copier.is_empty()
		&& config.runner.is_empty()
		&& config.pre_runner.is_empty()
	{
		debug!("no page builders specified, skipping builder init");
	}
	for copier in &config.pre_copier {
		copier::run_copier(copier).unwrap_or_else(|err| {
			error!("Unable to run copier for {:?}! {}", copier.input_dir, err);
			process::exit(exitcode::IOERR);
		});
	}
	for runner in &config.pre_runner {
		if !runner::run_runner(runner) {
			process::exit(exitcode::UNAVAILABLE);
		}
	}
	for builder in &config.builder {
		builder::run_builder(builder).unwrap_or_else(|err| {
			error!("Unable to run builder for {:?}! {}", builder.input_dir, err);
			process::exit(exitcode::DATAERR);
		})
	}
	for copier in &config.copier {
		copier::run_copier(copier).unwrap_or_else(|err| {
			error!("Unable to run copier for {:?}! {}", copier.input_dir, err);
			process::exit(exitcode::IOERR);
		});
	}
	for runner in &config.runner {
		if !runner::run_runner(runner) {
			process::exit(exitcode::UNAVAILABLE);
		}
	}

	let http_server = http::run_http_server(false, &config.server, &config.headers, &config.vhost)
		.unwrap_or_else(|err| {
			error!("Unable to configure HTTP server! {}", err);
			process::exit(exitcode::CONFIG);
		});
	let https_server = http::run_http_server(true, &config.server, &config.headers, &config.vhost)
		.unwrap_or_else(|err| {
			error!("Unable to configure HTTPS server! {}", err);
			process::exit(exitcode::CONFIG);
		});
	try_join!(http_server, https_server).unwrap_or_else(|err| {
		error!("Unable to start server! {}", err);
		process::exit(exitcode::OSERR);
	});

	debug!("HttpServer execution has stopped");
}
