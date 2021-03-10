# KatWebSite
A minimal static site generator and web server.

## Installation
KatWebSite is currently beta software and must be compiled from source. Pre-compiled builds may be available in the future.

### Building from source
1. Install a [Rust compiler](https://www.rust-lang.org/learn/get-started) to your device.
2. Download the contents of the repository. This can be done by either using the Git CLI (`git clone https://github.com/katattakd/KatWebSite`) or by downloading a [zip archive of the repo's contents](https://github.com/katattakd/KatWebSite/archive/main.zip).
3. Open a terminal inside the downloaded repository, and run `cargo build --release` (or without `--release` for a debug build).
   - The compiled output will be in either `target/release/katwebsite` or `target/debug/katwebsite`.

## Usage

### CLI arguments
KatWebSite has a basic cli interface, which can be used to change the config file that gets loaded or increase/decrease the logging verbosity. You can view a list of all CLI flags by running `katsite --help`.

### Configuration
KatWebSite's configuration is specified in the `config.toml` file by default. An example file containing all the configuration options (along with default values if they exist) can be found in `example.toml`. To prevent partial breakage when upgrading/downgrading, KatWebSite will fail to parse the configuration if unknown values are specified.

#### HTTP server
KatWebSite comes with a built-in HTTP(S) server that is robust and performant enough that it can be exposed directly to the public facing internet, if desired. It supports HTTP/2, has a secure TLS stack, has on-the-fly Gzip / Brotli compression, and can easily be configured through the config file. However, this server has a fairly minimal feature set, and more complex sites may be better off putting it behind a reverse proxy or disabling it entirely.

#### Site building
KatWebSite's site builder tries to be flexible yet simple, and avoids forcing a rigid structure on you. Here's how it handles input files:
- [builder.input_dir] - The root directory for the builder.
  - [filetype: directory] - Subdirectories in the builder directory are ignored. This may be useful if you are trying to nest different site builders in a directory.
  - [filetype: symlink] - Soft symbolic links are canonicalized and created in the output directory without being parsed. This may be useful for static asset directories which should not be processed, or just copying the contents of a file with frontmatter instead of having it get parsed.
  - [file.starts_with: "---"] - If Liquid templating is enabled, files must contain frontmatter opening and closing tags (`---`) to be parsed. If Liquid templating is disabled, all files are parsed.
  - [contents.is_parsed] - If a file is parsed, it goes through the following processing stages before being written to the output directory:
    - [parse_frontmatter()] - If Liquid templating is enabled, the frontmatter inside a file is parsed as TOML.
    - [build_liquid()] - If Liquid templating is enabled, all Liquid code inside the file is compiled.
    - *.md - If markdown is enabled, all markdown files are compiled into HTML.
    - *.sass - If SASS is enabled, all SASS files are compiled into CSS.
    - [apply_layout()] - If layouts are enabled, they are applied to the input file
  - [!contents.is_parsed] - If a file is not parsed, it is either hard linked into the output directory, or copied when hard linking is not possible.
  * [default_dirs] - Input directories specific to a specific renderer. These are specified relative to the input directory, but are not required to be inside the input directory.
    * [data_dir] - If data parsing is enabled, all TOML files in this directory are parsed into Liquid variables.
    * [layout_dir] - If layouts are enabled, all files in this directory can be loaded as a Liquid layout.
    * [include_dir] - If Liquid templating is enabled, all files in this directory can be used as includes. Files outside this directory cannot be used in includes.

#### Liquid
KatWebSite uses the [Liquid templating language](https://shopify.github.io/liquid/), with a slightly expanded [standard library](https://docs.rs/liquid-lib/0.22.0/liquid_lib/). KatWebSite-specific (not from liquid-lib) additions are listed below:
- [variables]
  - site: 
    - pages: Array of [type:Page] - All parsed files in the input directory.
    - files: Array of String - All files (besides symlinks and directories) in the input directory.
    - data: Array of Variables - All data parsed from files in the data directory.
  - page: [type: Page] - The current page being processed
  - [type: Page]:
    - path: String - The filename of the current page.
    - data: Variables - All data parsed from the frontmatter.
    - content: String - The contents of the current page. This is primarily intended for use in layouts, but can be called from ordinary pages too. If called from an ordinary page, the data here will be the unparsed version of the file.

