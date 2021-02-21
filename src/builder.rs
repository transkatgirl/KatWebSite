#![warn(clippy::all)]

use comrak::ComrakOptions;
use glob::MatchOptions;
use log::{trace, warn, debug, error, info};
use rayon::prelude::*;
use serde_derive::Deserialize;
use std::{process, iter, fs, fs::File, collections::BTreeMap, net::SocketAddr, ffi::OsStr, path::{Path, PathBuf}, io::BufReader, sync::Arc, error::Error, boxed::Box};

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

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Builder {
	pub input_glob: String,
	pub output: PathBuf,
}

fn build_page(input_path: &Path, paths: Vec<PathBuf>, builder: &Builder) {
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

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	debug!("starting builder for {:?}", builder.input_glob);
	let paths = glob::glob_with(&builder.input_glob, MatchOptions{
		case_sensitive: false,
		require_literal_separator: false,
		require_literal_leading_dot: true
	})?.filter_map(Result::ok).collect::<Vec<_>>();

	fs::create_dir_all(&builder.output)?;

	trace!("{:?} matches {:?}", builder.input_glob, paths);
	paths.iter().par_bridge().for_each(|fpath| {
		build_page(fpath, paths.to_owned(), builder);
	});

	Ok(())
}
