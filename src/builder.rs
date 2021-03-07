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
// - implement symlinking/copying from builder dir to output folder
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
	files: Vec<PathBuf>,
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
	pub input_dir: PathBuf,
	pub output: PathBuf,

	#[serde(default)]
	pub renderers: Renderers,

	#[serde(default)]
	pub default_dirs: Dirs,

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

impl Default for Dirs {
	fn default() -> Self {  Dirs {
		layout_dir: PathBuf::new(),
		include_dir: PathBuf::new(),
	}}
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Renderers {
	#[serde(default)]
	pub data: bool,

	#[serde(default)]
	pub liquid: bool,

	#[serde(default)]
	pub sass: bool,

	#[serde(default)]
	pub markdown: bool,
}

fn default_true() -> bool { true }

impl Default for Renderers {
	fn default() -> Self { Renderers {
		data: true,
		liquid: true,
		sass: true,
		markdown: true,
	}}
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

fn create_page(input: PathBuf, output: PathBuf, renderers: &Renderers) -> Option<Page> {
	trace!("parsing frontmatter for {:?}", &input);

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input, err);
		process::exit(exitcode::IOERR);
	});

	let mut page = Page {
		path: output.join(&input.file_name().unwrap_or_default()),
		data: Object::new(),
		content: input_str,
	};

	if !renderers.liquid {
		return Some(page)
	}

	let mut extractor = Extractor::new(&page.content);
	extractor.select_by_terminator("---");
	extractor.discard_first_line();

	let content = extractor.remove().to_owned();
	if content == "" {
		debug!("{:?} does not contain frontmatter", &input);
		return None // todo: symlink files that don't contain frontmatter using fs::soft_link
	}

	page.data = toml::from_str(&extractor.extract()).unwrap_or_else(|err| {
		warn!("Unable to parse {:?}'s frontmatter! {}", &input, err);
		Object::new()
	});

	page.content = content;

	return Some(page)
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
	options.extension.front_matter_delimiter = Some("---".to_string());
	options.parse.smart = true;
	options.render.unsafe_ = true;

	comrak::markdown_to_html(input, &options)
}

fn render_sass(input: String) -> Result<String, Box<grass::Error>> {
	let options = Options::default()
		.style(OutputStyle::Compressed);

	grass::from_string(input, &options)
}

fn build_site_page(mut page: Page, site: Site, renderers: &Renderers) -> Page {
	trace!("building {:?}", &page.path);

	let liquified = if renderers.liquid {
		let template = ParserBuilder::with_stdlib()
			.build().unwrap_or_else(|err| {
				error!("Unable to create liquid parser! {}", err);
				process::exit(exitcode::SOFTWARE);
			})
			.parse(&page.content).unwrap_or_else(|err| {
				error!("Unable to parse {:?}! {}", &page.path, err);
				process::exit(exitcode::DATAERR);
			});

		template.render(&liquid::object!({
			"site": site,
			"page": page,
		})).unwrap_or_else(|err| {
			error!("Unable to render {:?}! {}", &page.path, err);
			process::exit(exitcode::DATAERR);
		})
	} else {
		page.content.to_owned()
	};

	match page.path.as_path().extension().unwrap_or_default().to_str() {
		Some("md") if renderers.markdown => {
			trace!("generating {:?}", &page.path);
			page.content = render_markdown(&liquified);
			page.path.set_extension("html");
		},
		Some("sass") if renderers.sass => {
			trace!("generating {:?}", &page.path);
			page.content = render_sass(liquified).unwrap_or_else(|err| {
				error!("Unable to compile {:?}! {}", &page.path, err);
				process::exit(exitcode::DATAERR);
			});
			page.path.set_extension("css");
		},
		_ => page.content = liquified,
	}

	page
}

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	debug!("starting builder for {:?}", &builder.input_dir);

	fs::create_dir_all(&builder.output)?;

	let input = fs::read_dir(&builder.input_dir).unwrap_or_else(|err| {
		error!("Unable to open {:?}! {}", &builder.input_dir, err);
		process::exit(exitcode::IOERR);
	})
		.filter_map(Result::ok)
		.filter(|e| {
			e.file_type().map(|t| t.is_file()).unwrap_or(false)
		})
		.map(|e| e.path())
		.collect::<Vec<_>>();

	let data = if builder.renderers.data {
		input.iter()
			.filter(|p| p.extension() == Some(OsStr::new("toml")))
			.par_bridge()
			.map(|path| read_data(path.to_owned()))
			.collect::<Vec<_>>()
	} else {
		vec![]
	};

	let pages = input.iter()
		.par_bridge()
		.filter_map(|path| create_page(path.to_owned(), builder.output.to_owned(), &builder.renderers))
		.collect::<Vec<_>>();

	let mut site = Site {
		pages: pages.to_owned(),
		files: input.to_owned(), // TODO: make site.files relative to output folder
		data: data.to_owned(),
	};

	let pages = pages.iter().par_bridge()
		.map(|page| build_site_page(page.to_owned(), site.to_owned(), &builder.renderers))
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