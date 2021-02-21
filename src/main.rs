#![warn(clippy::all)]

use clap::Clap;
use comrak::ComrakOptions;
use futures::try_join;
use glob::MatchOptions;
use log::{trace, warn, debug, error, info};
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{process, iter, fs, fs::File, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}, io::BufReader, sync::Arc, error::Error, boxed::Box};

mod http;

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
	builder: Vec<Builder>,

	#[serde(default)]
	vhost: Vec<http::Vhost>,

	#[serde(default)]
	headers: BTreeMap<String, String>,

	server: Option<http::Server>,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
struct Builder {
	input_glob: String,
	output: PathBuf,
}



// TODO:
// - implement page generation
//   - implement custom html generator
//   - implement page layouts
//   - implement frontmatter parsing
//     - allow specifying layout in frontmatter
//     - allow setting default variable values in config
//   - implement liquid templating
//     - allow accessing frontmatter variables through liquid
//     - implement subset of jekyll liquid
//   - implement html sanitizer
//   - possible feature: implement file minifiers (html, css, js)
//   - possible feature: implement media optimization
//   * katsite code may be useful as a reference
// - separate code into multiple files

fn build_page(builder: &Builder, paths: Vec<PathBuf>, input_path: &Path) {
	trace!("parsing {:?}", &input_path);
	let input = fs::read_to_string(&input_path).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input_path, err);
		process::exit(exitcode::IOERR);
	});

	let mut options = ComrakOptions::default();
	options.extension.strikethrough = true;
	options.extension.table = true;
	options.extension.autolink = true;
	options.extension.tasklist = true;
	options.extension.superscript = true;
	options.extension.header_ids = Some("user-content-".to_string());
	options.extension.footnotes = true;
	options.extension.description_lists = true;
	options.extension.front_matter_delimiter = Some("---".to_owned());
	options.parse.smart = true;
	options.render.unsafe_ = true;

	let html = comrak::markdown_to_html(&input, &options);

	let mut output_path = builder.output.to_owned();
	output_path.push(input_path.file_stem().unwrap());
	output_path.set_extension("html");

	fs::write(&output_path, html).unwrap_or_else(|err| {
		error!("Unable to write to {:?}! {}", &output_path, err);
		process::exit(exitcode::IOERR);
	});
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
			trace!("no page builder specified, skipping")
		},
		false => (
			info!("Generating site pages")
		),
	}
	for builder in &config.builder {
		debug!("starting builder for {:?}", builder.input_glob);
		let paths = glob::glob_with(&builder.input_glob, MatchOptions{
			case_sensitive: false,
			require_literal_separator: false,
			require_literal_leading_dot: true
		}).unwrap_or_else(|err| {
			error!("Unable to parse file glob! {}", err);
			process::exit(exitcode::CONFIG);
		}).filter_map(Result::ok).collect::<Vec<_>>();

		fs::create_dir_all(&builder.output).unwrap_or_else(|err| {
			error!("Unable to create {:?}! {}", &builder.output, err);
			process::exit(exitcode::IOERR);
		});

		trace!("{:?} matches {:?}", builder.input_glob, paths);
		paths.iter().par_bridge().for_each(|fpath| {
			build_page(builder, paths.to_owned(), fpath);
		});
	}

	let serverconfig = match config.server {
		Some(ref server) => server,
		None => {
			trace!("no http server specified, exiting early");
			return
		},
	};

	let http_server = http::run_http_server(false, serverconfig, &config.headers, &config.vhost).unwrap_or_else(|err| {
		error!("Unable to configure HTTP server! {}", err);
		process::exit(exitcode::CONFIG);
	});
	let https_server = http::run_http_server(true, serverconfig, &config.headers, &config.vhost).unwrap_or_else(|err| {
		error!("Unable to configure HTTPS server! {}", err);
		process::exit(exitcode::CONFIG);
	});
	try_join!(http_server, https_server).unwrap_or_else(|err| {
		error!("Unable to start server! {}", err);
		process::exit(exitcode::OSERR);
	});

	debug!("server execution has stopped");
}
