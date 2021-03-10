#![warn(clippy::all)]

use comrak::ComrakOptions;
use extract_frontmatter::Extractor;
use grass::{Options, OutputStyle};
use liquid::{ParserBuilder, Object, model::Value};
use log::{trace, warn, debug, error, info};
use rayon::prelude::*;
use serde_derive::{Serialize, Deserialize};
use std::{process, iter, fs, path, path::{Path, PathBuf}, error::Error, boxed::Box, ffi::OsStr};

// TODO:
// - implement more of jeykll liquid
//   - implement file includes
// - possible feature: implement file minifiers (html, css, js)
// - possible feature: implement media optimization
// - code cleanup
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

// FIXME
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
		layout_dir: PathBuf::from("_layouts"),
		include_dir: PathBuf::from("_includes"),
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

	#[serde(default)]
	pub sanitizer: bool,

	#[serde(default)]
	pub layout: bool,
}

impl Default for Renderers {
	fn default() -> Self { Renderers {
		data: true,
		liquid: true,
		sass: true,
		markdown: true,
		sanitizer: false,
		layout: true,
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

fn create_page(input: PathBuf, output: PathBuf, defaults: &Object, renderers: &Renderers) -> Option<Page> {
	if renderers.liquid {
		trace!("parsing frontmatter for {:?}", &input);
	} else {
		trace!("loading {:?}", &input);
	}

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		error!("Unable to read {:?}! {}", &input, err);
		process::exit(exitcode::IOERR);
	});

	let mut page = Page {
		path: output.join(&input.file_name().unwrap_or_default()),
		data: defaults.to_owned(),
		content: input_str,
	};

	if !renderers.liquid {
		return Some(page)
	}

	let mut extractor = Extractor::new(&page.content);
	extractor.select_by_terminator("---");
	extractor.discard_first_line();

	let content = extractor.remove();
	if content.is_empty() {
		debug!("{:?} does not contain frontmatter", &input);
		return None
	}

	page.data.extend(toml::from_str(&extractor.extract()).unwrap_or_else(|err| {
		warn!("Unable to parse {:?}'s frontmatter! {}", &input, err);
		Object::new()
	}));

	page.content = content.to_owned();

	Some(page)
}

fn render_markdown(input: &str, renderers: &Renderers) -> String {
	let mut options = ComrakOptions::default();
	options.extension.strikethrough = true;
	options.extension.table = true;
	options.extension.autolink = true;
	options.extension.tasklist = true;
	options.extension.superscript = true;
	if !renderers.sanitizer {
		options.extension.header_ids = Some("user-content-".to_string());
		options.extension.footnotes = true;
	}
	options.extension.description_lists = true;
	options.extension.front_matter_delimiter = Some("---".to_string());
	options.parse.smart = true;
	options.render.github_pre_lang = true;
	options.render.unsafe_ = true;

	comrak::markdown_to_html(input, &options)
}

fn render_sass(input: String) -> Result<String, Box<grass::Error>> {
	let options = Options::default()
		.style(OutputStyle::Compressed);

	grass::from_string(input, &options)
}

fn render_liquid(raw_template: &str, page: &Page, site: &Site) -> Result<String, liquid::Error> {
	let parsed_template = ParserBuilder::with_stdlib()
		.build()?
		.parse(raw_template)?;

	parsed_template.render(&liquid::object!({
		"site": site,
		"page": page,
	}))
}

fn build_site_page(mut page: Page, site: Site, renderers: &Renderers) -> Option<Page> {
	if renderers.liquid && page.path.as_path().extension() != Some(OsStr::new("liquid")) {
		trace!("building {:?}", &page.path);

		page.content = render_liquid(&page.content, &page, &site).unwrap_or_else(|err| {
			error!("Unable to build {:?}! {}", &page.path, err);
			process::exit(exitcode::DATAERR);
		})
	};

	match page.path.as_path().extension().unwrap_or_default().to_str() {
		Some("md") if renderers.markdown => {
			trace!("generating {:?}", &page.path);
			page.content = render_markdown(&page.content, renderers);
			page.path.set_extension("html");
		},
		Some("sass") if renderers.sass => {
			trace!("generating {:?}", &page.path);
			page.content = render_sass(page.content.to_owned()).unwrap_or_else(|err| {
				error!("Unable to compile {:?}! {}", &page.path, err);
				process::exit(exitcode::DATAERR);
			});
			page.path.set_extension("css");
		},
		_ => (),
	}

	Some(page)
}

fn complete_site_page(mut page: Page, site: Site, renderers: &Renderers, input_dir: &Path, dirs: &Dirs) -> Page {
	match page.path.as_path().extension().unwrap_or_default().to_str() {
		Some("html") if renderers.sanitizer => {
			trace!("sanitizing {:?}", &page.path);
			page.content = ammonia::clean(&page.content);
		},
		_ => (),
	}

	if !renderers.layout {
		return page
	}

	if let Some(Value::Scalar(template)) = page.data.get("layout") {
		trace!("building layout for {:?}", &page.path);

		let template_path = input_dir.join(&dirs.layout_dir).join(template.to_owned().into_string());

		match fs::read_to_string(&template_path) {
			Ok(template_content) => {
				page.content = render_liquid(&template_content, &page, &site).unwrap_or_else(|err| {
					error!("Unable to build layout for {:?}! {}", &page.path, err);
					process::exit(exitcode::DATAERR);
				});

				match template_path.as_path().extension().unwrap_or_default().to_str() {
					Some(ext) => page.path.set_extension(ext),
					_ => true,
				};
			},
			Err(err) => {
				warn!("Unable to load {:?}! {}", &template_path, err);
			}
		}
	}

	page
}

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	debug!("starting builder for {:?}", &builder.input_dir);

	if builder.output.as_path().exists() {
		fs::remove_dir_all(&builder.output)?;
	}
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

	let files = input.iter()
		.filter_map(|path| path.file_name())
		.map(PathBuf::from)
		.collect::<Vec<_>>();

	let pages = input.iter()
		.par_bridge()
		.filter_map(|path| create_page(path.to_owned(), builder.output.to_owned(), &builder.default_vars, &builder.renderers))
		.collect::<Vec<_>>();

	let mut site = Site {
		pages,
		files,
		data,
	};

	site.pages = site.pages.iter().par_bridge()
		.filter_map(|page| build_site_page(page.to_owned(), site.to_owned(), &builder.renderers))
		.collect::<Vec<_>>();

	site.pages = site.pages.iter().par_bridge()
		.map(|page| complete_site_page(page.to_owned(), site.to_owned(), &builder.renderers, &builder.input_dir, &builder.default_dirs))
		.collect::<Vec<_>>();

	site.pages.iter().par_bridge().for_each(|page| {
		fs::write(&page.path, &page.content).unwrap_or_else(|err| {
			error!("Unable to write to {:?}! {}", &page.path, err);
			process::exit(exitcode::IOERR);
		});
	});

	site.files.iter()
		.filter(|p| !builder.output.as_path().join(p).exists())
		.par_bridge().for_each(|p| {
			let input_file = builder.input_dir.as_path().join(p);
			let output_file = builder.output.as_path().join(p);

			trace!("symlinking {:?}", &input_file);
			fs::hard_link(&input_file, &output_file).unwrap_or_else(|err| {
				trace!("Unable to symlink to {:?}! {}", &output_file, err);

				trace!("copying {:?}...", &input_file);
				fs::copy(&input_file, &output_file).unwrap_or_else(|err| {
					error!("Unable to copy to {:?}! {}", &output_file, err);
					process::exit(exitcode::IOERR);
				});
			});
		});

	Ok(())
}
