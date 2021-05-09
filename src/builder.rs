#![warn(clippy::all)]

use comrak::ComrakOptions;
use extract_frontmatter::Extractor;
use grass::{Options, OutputStyle};
use liquid::{
	model::Value,
	partials::{InMemorySource, LazyCompiler},
	Object, ParserBuilder,
};
use log::{debug, error, info, trace, warn};
use rayon::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::{
	boxed::Box,
	error::Error,
	fs,
	path::{Path, PathBuf},
	process,
};

#[derive(Serialize, Clone, Debug)]
struct Site {
	pages: Vec<Page>,
	files: Vec<PathBuf>,
	data: Vec<Object>,
}

#[derive(Serialize, Clone, Debug)]
struct Page {
	path: PathBuf,
	data: Object,
	content: String,
}

#[derive(Deserialize, Clone, Debug)]
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

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Dirs {
	#[serde(default = "default_data_dir")]
	pub data_dir: PathBuf,

	#[serde(default = "default_layout_dir")]
	pub layout_dir: PathBuf,

	#[serde(default = "default_include_dir")]
	pub include_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
	PathBuf::from("_data")
}

fn default_layout_dir() -> PathBuf {
	PathBuf::from("_layouts")
}

fn default_include_dir() -> PathBuf {
	PathBuf::from("_includes")
}

impl Default for Dirs {
	fn default() -> Self {
		Dirs {
			data_dir: default_data_dir(),
			layout_dir: default_layout_dir(),
			include_dir: default_include_dir(),
		}
	}
}

#[derive(Deserialize, Clone, Debug)]
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
	fn default() -> Self {
		Renderers {
			data: true,
			liquid: true,
			sass: true,
			markdown: true,
			sanitizer: false,
			layout: true,
		}
	}
}

fn read_data(input: PathBuf) -> Option<Object> {
	trace!(
		"loading {:?}",
		input.as_path().file_name().unwrap_or_default()
	);

	match fs::read_to_string(&input) {
		Ok(text) => match toml::from_str(&text) {
			Ok(obj) => obj,
			Err(err) => {
				warn!("Unable to parse {:?}! {}", &input, err);
				None
			}
		},
		Err(err) => {
			warn!("Unable to read {:?}! {}", &input, err);
			None
		}
	}
}

fn read_path(input_dir: &Path) -> Vec<PathBuf> {
	match fs::read_dir(input_dir) {
		Ok(dir) => dir
			.filter_map(Result::ok)
			.filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
			.map(|e| e.path())
			.collect::<Vec<_>>(),
		Err(err) => {
			debug!("Unable to open {:?}! {}", input_dir, err);
			vec![]
		}
	}
}

fn create_page(input: PathBuf, defaults: &Object, renderers: &Renderers) -> Option<Page> {
	debug!(
		"loading {:?}",
		input.as_path().file_name().unwrap_or_default()
	);

	let input_str = fs::read_to_string(&input).unwrap_or_else(|err| {
		debug!("Unable to read {:?}! {}", &input, err);
		String::new()
	});

	if input_str.is_empty() {
		return None;
	}

	let mut page = Page {
		path: PathBuf::from(input.file_name().unwrap_or_default()),
		data: defaults.to_owned(),
		content: input_str,
	};

	let mut extractor = Extractor::new(&page.content);
	extractor.select_by_terminator("---");
	extractor.discard_first_line();

	let content = extractor.remove();
	if content.is_empty() {
		debug!("{:?} does not contain frontmatter", &input);
		return None;
	}

	if renderers.liquid {
		page.data
			.extend(toml::from_str(&extractor.extract()).unwrap_or_else(|err| {
				warn!("Unable to parse {:?}'s frontmatter! {}", &input, err);
				Object::new()
			}));
	}

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
		options.extension.header_ids = Some("".to_string());
		options.extension.footnotes = true;
	}
	options.extension.description_lists = true;
	options.extension.front_matter_delimiter = Some("---".to_string());
	options.parse.smart = true;
	options.render.unsafe_ = true;

	comrak::markdown_to_html(input, &options)
}

fn render_sass(input: String) -> Result<String, Box<grass::Error>> {
	let options = Options::default().style(OutputStyle::Compressed);

	grass::from_string(input, &options)
}

fn render_liquid(
	raw_template: &str,
	page: &Page,
	site: &Site,
	partials: InMemorySource,
) -> Result<String, liquid::Error> {
	ParserBuilder::with_stdlib()
		.partials(LazyCompiler::new(partials))
		.build()?
		.parse(raw_template)?
		.render(&liquid::object!({
				"site": site,
				"page": page,
		}))
}

fn build_site_page(
	mut page: Page,
	site: Site,
	renderers: &Renderers,
	partials: InMemorySource,
) -> Page {
	if renderers.liquid {
		debug!("building {:?}", &page.path);

		page.content = render_liquid(&page.content, &page, &site, partials).unwrap_or_else(|err| {
			error!("Unable to build {:?}! {}", &page.path, err);
			process::exit(exitcode::DATAERR);
		})
	};

	page
}

fn render_page(mut page: Page, renderers: &Renderers) -> Page {
	match page.path.as_path().extension().unwrap_or_default().to_str() {
		Some("md") if renderers.markdown => {
			debug!("generating {:?}", &page.path);
			page.content = render_markdown(&page.content, renderers);
			page.path.set_extension("html");
		}
		Some("scss") if renderers.sass => {
			debug!("generating {:?}", &page.path);
			page.content = render_sass(page.content.to_owned()).unwrap_or_else(|err| {
				error!("Unable to compile {:?}! {}", &page.path, err);
				process::exit(exitcode::DATAERR);
			});
			page.path.set_extension("css");
		}
		_ => (),
	}
	match page.path.as_path().extension().unwrap_or_default().to_str() {
		Some("html") if renderers.sanitizer => {
			debug!("sanitizing {:?}", &page.path);
			page.content = ammonia::clean(&page.content);
		}
		_ => (),
	}
	page
}

fn complete_site_page(
	mut page: Page,
	site: Site,
	renderers: &Renderers,
	input_dir: &Path,
	dirs: &Dirs,
	partials: InMemorySource,
) -> Page {
	if !renderers.layout {
		return page;
	}

	if let Some(Value::Scalar(template)) = page.data.get("layout") {
		let layout = template.to_owned().into_string();

		if layout.is_empty() {
			return page;
		}

		debug!("laying out {:?}", &page.path);

		let template_path = input_dir.join(&dirs.layout_dir).join(layout);

		match fs::read_to_string(&template_path) {
			Ok(template_content) => {
				page.content = render_liquid(&template_content, &page, &site, partials)
					.unwrap_or_else(|err| {
						error!("Unable to build layout for {:?}! {}", &page.path, err);
						process::exit(exitcode::DATAERR);
					});

				match template_path
					.as_path()
					.extension()
					.unwrap_or_default()
					.to_str()
				{
					Some("") | None => true,
					Some(ext) => page.path.set_extension(ext),
				};
			}
			Err(err) => {
				warn!("Unable to load {:?}! {}", &template_path, err);
			}
		}
	}

	page
}

pub fn run_builder(builder: &Builder) -> Result<(), Box<dyn Error>> {
	info!("Generating pages in {:?}", &builder.input_dir);

	if builder.output.as_path().exists() {
		fs::remove_dir_all(&builder.output)?;
	}

	fs::create_dir_all(&builder.output)?;

	let mut partials = InMemorySource::new();
	if builder.renderers.liquid {
		for file in read_path(
			&builder
				.input_dir
				.as_path()
				.join(&builder.default_dirs.include_dir),
		) {
			trace!(
				"loading {:?}",
				file.as_path().file_name().unwrap_or_default()
			);
			partials.add(
				file.file_stem()
					.unwrap_or_default()
					.to_str()
					.unwrap_or_default(),
				fs::read_to_string(&file).unwrap_or_else(|err| {
					warn!("Unable to read {:?}! {}", &file, err);
					String::new()
				}),
			);
		}
	}

	let data = if builder.renderers.data {
		read_path(
			&builder
				.input_dir
				.as_path()
				.join(&builder.default_dirs.data_dir),
		)
		.iter()
		.par_bridge()
		.filter_map(|path| read_data(path.to_owned()))
		.collect::<Vec<_>>()
	} else {
		vec![]
	};

	let input = read_path(&builder.input_dir);

	let files = input
		.iter()
		.filter_map(|path| path.file_name())
		.map(PathBuf::from)
		.par_bridge()
		.inspect(|p| {
			let input_file = builder.input_dir.as_path().join(p);
			let output_file = builder.output.as_path().join(p);

			trace!(
				"copying {:?}",
				input_file.as_path().file_name().unwrap_or_default()
			);
			fs::copy(&input_file, &output_file).unwrap_or_else(|err| {
				error!("Unable to copy to {:?}! {}", &output_file, err);
				process::exit(exitcode::IOERR);
			});
		})
		.collect::<Vec<_>>();

	let pages = input
		.iter()
		.par_bridge()
		.filter_map(|path| create_page(path.to_owned(), &builder.default_vars, &builder.renderers))
		.collect::<Vec<_>>();

	let mut site = Site { pages, files, data };

	site.pages = site
		.pages
		.iter()
		.par_bridge()
		.map(|page| {
			build_site_page(
				page.to_owned(),
				site.to_owned(),
				&builder.renderers,
				partials.to_owned(),
			)
		})
		.inspect(|page| {
			let output_file = builder.output.as_path().join(&page.path);

			trace!("writing {:?}", &page.path);
			fs::write(&output_file, &page.content).unwrap_or_else(|err| {
				error!("Unable to write to {:?}! {}", &output_file, err);
				process::exit(exitcode::IOERR);
			});
		})
		.map(|page| render_page(page, &builder.renderers))
		.collect::<Vec<_>>();

	site.pages
		.iter()
		.par_bridge()
		.map(|page| {
			complete_site_page(
				page.to_owned(),
				site.to_owned(),
				&builder.renderers,
				&builder.input_dir,
				&builder.default_dirs,
				partials.to_owned(),
			)
		})
		.for_each(|page| {
			let output_file = builder.output.as_path().join(&page.path);

			trace!("writing {:?}", &page.path);
			fs::write(&output_file, &page.content).unwrap_or_else(|err| {
				error!("Unable to write to {:?}! {}", &output_file, err);
				process::exit(exitcode::IOERR);
			});
		});

	if let Ok(dir) = fs::read_dir(&builder.input_dir) {
		dir.filter_map(Result::ok)
			.filter(|e| e.file_type().map(|t| t.is_symlink()).unwrap_or(false))
			.par_bridge()
			.for_each(|p| {
				trace!("symlinking {:?}...", p.path());

				let input_file = p.path().canonicalize().unwrap_or_else(|_| p.path());
				let output_file = builder.output.as_path().join(p.file_name());

				#[allow(deprecated)]
				fs::soft_link(input_file, &output_file).unwrap_or_else(|err| {
					error!("Unable to copy to {:?}! {}", &output_file, err);
					process::exit(exitcode::IOERR);
				});
			});
	}

	Ok(())
}
