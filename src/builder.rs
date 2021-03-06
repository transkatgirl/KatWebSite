#![warn(clippy::all)]

use comrak::ComrakOptions;
use extract_frontmatter::Extractor;
use grass::{Options, OutputStyle};
use liquid::{ParserBuilder, Object};
use log::{trace, warn, debug, error, info};
use rayon::prelude::*;
use serde_derive::{Serialize, Deserialize};
use std::{process, iter, fs, path, path::{Path, PathBuf}, error::Error, boxed::Box, ffi::OsStr};

// TODO:
// - finish implementing Site
//   - allow symlinking/copying from builder dir to output folder
// - implement more of jeykll liquid
//   - implement file includes
// - implement layouts
//   - allow specifying layouts in frontmatter
// - implement html sanitizer
// - possible feature: implement file minifiers (html, css, js)
// - possible feature: implement media optimization
//   * katsite code may be useful as a reference

/*
order of interpretation:
- frontmatter/data file parsing
- liquid parsing
- markdown parsing
- layout parsing
*/

#[derive(Serialize,Clone,Debug)]
struct Site {
	pages: Vec<Page>,
	//files: Vec<PathBuf>,
	data: Vec<Object>,
}

#[derive(Serialize,Clone,Debug)]
struct Page {
	path: PathBuf,
	data: Object,
	content: String,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Builder {
	pub mount: PathBuf,
	pub output: PathBuf,

	pub sanitize: bool,

	pub default_dirs: Option<Dirs>,

	#[serde(default)]
	pub default_vars: Object,

}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Dirs {
	#[serde(default)]
	pub layout_dir: PathBuf,

	#[serde(default)]
	pub include_dir: PathBuf,
}

fn read_data(input: PathBuf) -> Object {
	trace!("loading {:?}", &input);

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input, err);
		process::exit(exitcode::IOERR);
	});

	toml::from_str(&input_str).unwrap_or_else(|err| {
		error!("Unable to parse {:?}! {}", &input, err);
		process::exit(exitcode::DATAERR);
	})
}

fn create_page(input: PathBuf, output: PathBuf) -> Option<Page> {
	trace!("parsing frontmatter for {:?}", &input);

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input, err);
		process::exit(exitcode::IOERR);
	});

	let mut extractor = Extractor::new(&input_str);
	extractor.select_by_terminator("---");
	extractor.discard_first_line();

	let content = extractor.remove().to_owned();
	if content == "" {
		debug!("{:?} does not contain frontmatter", &input);
		return None
	}

	let data: Object = toml::from_str(&extractor.extract()).unwrap_or_else(|err| {
		error!("Unable to parse {:?}'s frontmatter! {}", &input, err);
		process::exit(exitcode::DATAERR);
	});

	Some ( Page {
		path: output.join(&input.file_name().unwrap_or_default()),
		data: data,
		content: content,
	})
}

fn render_markdown(input: &str) -> String {
	let mut options = ComrakOptions::default();
	options.extension.strikethrough = true;
	options.extension.table = true;
	options.extension.autolink = true;
	options.extension.tasklist = true;
	options.extension.superscript = true;
	options.extension.header_ids = Some("user-content-".to_string());
	options.extension.footnotes = true;
	options.extension.description_lists = true;
	options.parse.smart = true;
	options.render.unsafe_ = true;

	comrak::markdown_to_html(input, &options)
}

fn render_sass(input: String) -> Result<String, Box<grass::Error>> {
	let options = Options::default()
		.style(OutputStyle::Compressed);

	grass::from_string(input, &options)
}

fn build_site_page(mut page: Page, site: Site) -> Page {
	trace!("building {:?}", &page.path);

	let template = ParserBuilder::with_stdlib()
		.build().unwrap_or_else(|err| {
			error!("Unable to create liquid parser! {}", err);
			process::exit(exitcode::SOFTWARE);
		})
		.parse(&page.content).unwrap_or_else(|err| {
			error!("Unable to parse {:?}! {}", &page.path, err);
			process::exit(exitcode::DATAERR);
		});

	let liquified = template.render(&liquid::object!({
		"site": site,
		"page": page,
	})).unwrap_or_else(|err| {
		error!("Unable to render {:?}! {}", &page.path, err);
		process::exit(exitcode::DATAERR);
	});

	match page.path.as_path().extension() {
		Some(ext) => { match ext.to_str() { // There's no way to make a static OsStr (yet).
			Some("md") => {
				trace!("generating {:?}", &page.path);
				page.content = render_markdown(&liquified);
				page.path.set_extension("html");
			},
			Some("sass") => {
				trace!("generating {:?}", &page.path);
				page.content = render_sass(liquified).unwrap_or_else(|err| {
					error!("Unable to compile {:?}! {}", &page.path, err);
					process::exit(exitcode::DATAERR);
				});
				page.path.set_extension("css");

			},
			_ => page.content = liquified,
		}},
		_ => page.content = liquified,
	}

	page
}

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	debug!("starting builder for {:?}", &builder.mount);

	fs::create_dir_all(&builder.output)?;

	let input = fs::read_dir(&builder.mount).unwrap_or_else(|err| {
		error!("Unable to open {:?}! {}", &builder.mount, err);
		process::exit(exitcode::IOERR);
	})
		.filter_map(Result::ok)
		.filter(|e| {
			e.file_type().map(|t| t.is_file()).unwrap_or(false)
		})
		.map(|e| e.path())
		.collect::<Vec<_>>();

	let data = input.iter()
		.filter(|p| p.extension() == Some(OsStr::new("toml")))
		.par_bridge()
		.map(|path| read_data(path.to_owned()))
		.collect::<Vec<_>>();

	let pages = input.iter()
		.filter(|p| p.extension() != Some(OsStr::new("toml")))
		.par_bridge()
		.filter_map(|path| create_page(path.to_owned(), builder.output.to_owned()))
		.collect::<Vec<_>>();

	let mut site = Site {
		pages: pages.to_owned(),
		data: data.to_owned(),
	};

	let pages = pages.iter().par_bridge()
		.map(|page| build_site_page(page.to_owned(), site.to_owned()))
		.collect::<Vec<_>>();

	site.pages = pages.to_owned();

	pages.iter().par_bridge().for_each(|page| {
		fs::write(&page.path, &page.content).unwrap_or_else(|err| {
			error!("Unable to write to {:?}! {}", &page.path, err);
			process::exit(exitcode::IOERR);
		});
	});

	Ok(())
}
