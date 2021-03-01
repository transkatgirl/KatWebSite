#![warn(clippy::all)]

use comrak::ComrakOptions;
use extract_frontmatter::Extractor;
use glob::MatchOptions;
use liquid::{ParserBuilder, Object};
use log::{trace, warn, debug, error, info};
use rayon::prelude::*;
use serde_derive::{Serialize, Deserialize};
use std::{process, iter, fs, path, path::{Path, PathBuf}, error::Error, boxed::Box, ffi::OsStr};

// TODO:
// - finish implementing Site
//   - allow symlinking/copying from builder dir to output folder
//   - implement data parsing
// - implement more of jeykll liquid
// - implement layouts
//   - allow specifying layouts in frontmatter
// - implement html sanitizer
// - implement sass css
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
	//data: Vec<Object>,
}

#[derive(Serialize,Clone,Debug)]
struct Page {
	path: PathBuf,
	data: Option<Object>,
	content: String,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Builder {
	pub root: String,
	pub output: PathBuf,

	#[serde(default)]
	pub data: Vec<Data>,

	#[serde(default)]
	pub sass: Vec<Sass>,
	
	#[serde(default)]
	pub pages: Vec<PageBuilder>,

}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Sass {
	pub input: String,
	pub output: PathBuf,
}


#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct Data {
	#[serde(default)]
	pub input: String,
}

#[derive(Deserialize,Clone,Debug)]
#[serde(deny_unknown_fields)]
pub struct PageBuilder {
	pub input: String,
	pub sanitize: bool,
	pub layout: Option<PathBuf>,

	#[serde(default)]
	pub default_vars: Object,
}

fn create_page(input: PathBuf, output: PathBuf) -> Page {
	trace!("parsing frontmatter for {:?}", &input);

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input, err);
		process::exit(exitcode::IOERR);
	});

	let mut extractor = Extractor::new(&input_str);
	extractor.select_by_terminator("---");
	extractor.discard_first_line();

	let mut page = Page {
		path: output.join(&input.file_name().unwrap_or_default()),
		data: None,
		content: extractor.remove().to_owned(),
	};

	if page.content == "" {
		debug!("{:?} does not contain frontmatter", &input);
		page.content = input_str;
		return page
	}

	let data: Object = toml::from_str(&extractor.extract()).unwrap_or_else(|err| {
		error!("Unable to parse {:?}'s frontmatter! {}", &input, err);
		process::exit(exitcode::DATAERR);
	});

	page.data = Some(data);
	page
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

	if page.path.as_path().extension() == Some(OsStr::new("md")) {
		trace!("generating {:?}", &page.path);

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

		page.content = comrak::markdown_to_html(&liquified, &options);
		page.path.set_extension("html");
	} else {
		page.content = liquified;
	}

	page
}

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	let root = [builder.root.to_owned(), path::MAIN_SEPARATOR.to_string()].concat();
	debug!("starting builder for {:?}", root);

	fs::create_dir_all(&builder.output)?;

	for pagebuilder in &builder.pages {
		let glob = [root.as_str(), pagebuilder.input.as_str()].concat();
		let pages = glob::glob_with(&glob, MatchOptions{
			case_sensitive: false,
			require_literal_separator: false,
			require_literal_leading_dot: true
		})?
			.par_bridge().filter_map(Result::ok)
			.map(|fpath| create_page(fpath, builder.output.to_owned()))
			.collect::<Vec<_>>();

		let mut site = Site {
			pages: pages.to_owned(),
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
	}

	Ok(())
}
