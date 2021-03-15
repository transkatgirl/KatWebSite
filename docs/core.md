# KatWebSite Core Docs
This file is intended to provide complete documentation for the KatWebSite binary. However, it is not intended to be the entirety of KatWebSite's documentation. There will be additional files in the docs folder that document other components of KatWebSite, such as it's built-in input files.

If the documentation does not match the program's current behavior or is missing important information, this is considered a bug, and should be reported as such.

**THIS DOCUMENT IS A WORK IN PROGRESS**

**TODO: Make sure all links are valid after document is completed!**

### Table of Contents

---

## Configuration

### Loading and parsing configuration files
By default, KatWebSite loads configuration from a file named "config.toml" in the working directory. However, starting the KatWebSite binary with the `--config <file path>` flag will load the specified configuration file, and change the working directory of KatWebSite to the folder the configuration is contained in.

Configuration files *must* be written in TOML. You may want to read the [full documentation of TOML syntax](https://toml.io/en/v1.0.0), or the [TOML reference](https://toml.io/en/),

Configuration options with no default value must be explicitly specified, or loading the configuration file will fail. In addition, having values in the configuration file that are not used by the program will also load to fail the configuration file. These are both intentional design choices; not bugs. The config file format may change slightly between releases, and making the config parser very strict can prevent subtle issues from cropping up after a major update.

If the order a config option is loaded in is not explicitly specified in this document, the loading order may change between releases, and should not be relied upon. And, unless explicitly specified, config strings do not support wildcards, formatting, or RegEx of any kind.

Directories in the config file are all relative to the program's working directory, unless otherwise specified.

---

### Site generator configuration
KatWebSite allows you to turn various input files into a generated website through Builders. You can specify as many builders as needed through `[[builder]]` blocks in the configuration file, and Builders will always be run to completion before the web server is started.

If you do not intend to use the site generator, specifying no `[[builder]]` blocks in the configuration will automatically disable it.

Each `[[builder]]` block requires two values:
- `input_dir` - The directory input files are loaded from.
- `output` - The directory output files are written to.

An example of a basic `[[builder]]` block is written below:
```toml
[[builder]]
input_dir = "html/localhost"
output = ".site/localhost"
```

Builders are run one-at-a-time, in the order they're specified in. However, the tasks that Builders run can be heavily multi-threaded. You can change the maximum number of threads multi-threaded tasks can use with the `RAYON_NUM_THREADS` environment variable.

However, Builders do not work like other site generators, and it's heavily recommended that you read the [Site generation - Overview section](#overview) before adding any Builders to your configuration file.

#### Enabling or disabling Builder Renderers
Builders generate your site through the use of Renderers. Renderers may parse and compile input files, including those not in the `builder.input_dir`, and will write the finished output to `builder.output`. Some Renderers may only run on certain types of files, and others may even prevent certain types of files from being used.

If a `[builder.renderers]` block is not specified, all Renderers are enabled, except those that are intended to restrict functionality instead of adding it (like an HTML sanitizer). However, when a `[builder.renderers]` block is specified, all Renderers in the section are disabled by default, and must be enabled individually in the configuration.

All the implemented Renderers at the time of writing are listed below:
- `data` - Loads data files contained in `builder.default_dirs.data_dirs` and parses them into Liquid variables.
- `liquid` - Enables per-file Liquid templating, and discards all input files which don't contain frontmatter.
- `sass` - Compiles SASS files into CSS.
- `markdown` - Compiles Markdown files into HTML.
- `sanitizer` - Heavily sanitizes untrusted HTML files.
- `layout` - Applies Liquid layouts from `builder.default_dirs.layout_dir` to files right before they're written to `builder.output`.

The listed Renderers all use the `builder.input_dir` folder as input unless otherwise specified, and are run in the same order they're listed in above. You can learn more about the various Renderers in the [Site generation section](#site-generation).

An enable of a `[[builder]]` block with all Renderers enabled is written below:
```toml
[[builder]]
input_dir = "html/localhost"
output = ".site/localhost"

[builder.renderers]
data = true
liquid = true
sass = true
markdown = true
sanitizer = true
layout = true
```

#### Configuring additional Render inputs
Some renderers may need to use input files that aren't stored within the `builder.input_dir` root directory. KatWebSite contains sensible defaults for the location these inputs should be loaded from, but some users may find it necessary to override these. This can be done through the use of the `[builder.default_dirs]` block.

At the time of writing, there are only three paths you can change using this block:
- `data_dir` - The folder that the [data Renderer](#the-data-renderer) loads input files from.
- `layout_dir` - The folder that the [layout Renderer](#liquid-layouts) loads Liquid layouts from.
- `include_dir` - The folder that the [liquid Renderer](#liquid-templating) loads Liquid includes from.

All directories under the `[default_dirs]` block are relative to the `builder.input_dir` directory. However, they are *not required* to be within that directory.

An example of a `[builder.default_dirs]` block, showing the default values being explicitly re-stated, is written below.
```toml
# Root [[builder]] block omitted for clarity.

[builder.default_dirs]
data_dir = "_data"
layout_dir = "_layouts"
include_dir = "_includes"
```

#### Configuring Liquid defaults
The [liquid Renderer](#liquid-templating) loads per-page variables from frontmatter, but in many cases, a variable value is used in many files, and it may be desirable to make that variable's value the default for all pages. This can be done with the `[builder.default_vars]` block.

These blocks can specify values in the same way as frontmatter. The only difference is that they're specifying Liquid variables for all pages, instead of a single page. Per-page frontmatter variables will override these defaults.

An example of a `[builder.default_vars]` block is shown below:
```toml
# Root [[builder]] block omitted for clarity.

[builder.default_vars]
title = "My page"
```

---

### Web server configuration
KatWebSite allows you to create different routing configurations depending on the HTTP `Host` header using "virtual hosts". You can specify as many virtual hosts as needed through `[[vhost]]` blocks in the configuration file, and virtual hosts will always be initialized in the order they're specified in.

Each `[[vhost]]` block requires a `host` value, which specifies the `Host` header value that it matches. Port numbers are automatically removed during matching.

An example of a basic `[[vhost]]` block is written below:
```toml
[[vhost]]
host = "localhost"
```

Additional info: 
- Path deserialization and wildcards are handled by `actix-web`. Because of this, any `[[vhost]]` sub-blocks that use path deserialization or wildcards will need to specify `/` as a special case to handle requests going to the root directory.
- `[[vhost.redir]]` blocks will be loaded before `[[vhost.files]]` blocks, and all types of a specific sub-block (for example: two instances of `[[vhost.files]]`) will be loaded in the order they're specified in.
- Duplicate `/` characters in a URL segment will be combined into a single `/` during processing (for example: `/redir////yeet` becomes `/redir/yeet`).

#### Configuring HTTP redirects
HTTP redirects are configured on a per-vhost basis through the use of `[[vhost.redir]]` blocks.

Each `[[vhost.redir]]` block can contain up to three options:
- `target` - The URL segment (for example: `/redir`) that this redirect targets, defaults to `/`. Supports wildcards (`/redir*`) and path deserialization (`/redir/{path:.*}`).
- `dest` - The URL this redirect points to (for example: `https://example.com`). If path deserialization is being used, deserialized values will be appended to the end of this URL.
- `permanent` - If the redirect should be permanent (HTTP 308) or temporary (HTTP 307), defaults to temporary.
  - Permanent redirects are cached by browsers for long periods of time, and are treated by search engines like the URL it redirects to. Because of this, permanent redirects have limited uses, and should only be used when you are sure they will not need to be removed later on.
  - Temporary redirects are not cached or treated specially by search engines, and as a result, have more potential applications (like a "coming soon page").

An example of a `[[vhost.redir]]` block is shown below:
```toml
[[vhost]]
host = "localhost"

[[vhost.redir]]
target = "/redir"
dest = "https://example.com"
permanent = false
```

Another example, showing a `[[vhost.redir]]` block that uses path deserialization, is shown below.
```toml
# Root [[vhost]] block omitted for clarity

[[vhost.redir]]
target = "/oldlogin/{path:.*}"
dest = "https://site.com/newlogin"
permanent = true
```

#### Configuring HTTP file handlers
File-serving is configured on a per-vhost basis through the use of `[[vhost.files]]` blocks.

Each `[[vhost.files]]` block can contain up to two options:
- `mount` - The base URL segment (for example: `/static`) that this handler targets, defaults to `/`. This matches all URL segments that start with this value, not just the exact value.
- `file_dir` - The directory that files are served from. Unlike `[[builder]]` blocks, sub-directories and symbolic links will be served normally, but hidden files and directories will not be served.

An example of a `[[vhost.files]]` block is shown below:
```toml
# Root [[vhost]] block omitted for clarity

[[vhost.files]]
mount = "/static"
file_dir = "html/static"
```

Additional Notes:
- File handlers detect MIME type purely based on file extension, assume all text files are UTF-8, and defaults to `application/octet-stream` if the file extension could not be recognized. This may cause issues for files which have incorrect and/or rarely used extensions.
- When serving a directory, the file handler will attempt to find an `index.html` file to serve. If no such file is present, the handler will return a 404 instead of generating a list of files in the directory.
- Only the GET and HEAD HTTP methods are supported for file handler requests.
- Subdirectory roots can be retrieved by clients using either the `/dir` or `/dir/` URL segments. The server will not attempt to enforce a "correct" way to retrieve subdirectories through the use of redirects.

#### Configuring TLS
TLS certificates are configured on a per-vhost basis through the use of `[vhost.tls]` blocks. If this block is omitted, the virtual host will only be accessible over HTTP.

Each `[vhost.tls]` block can contain up to two values:
- `pemfiles` - A list of PEM files for the virtual host. The contents of them are automatically detected, you may specify as many of them as needed, and they will be loaded in the order specified.
- `http_dest` - This specifies the destination for an automatic HTTP -> HTTPS redirect. If this is specified, all HTTP vhost requests will be redirected to `http_dest`. If this is omitted, HTTP vhost requests will be handled the same way as HTTPS vhost requests.

An example of a `[vhost.tls]` block is shown below:

```toml
# Root [[vhost]] block omitted for clarity

[vhost.tls]
pemfiles = [
	"ssl/localhost_cert.pem",
	"ssl/localhost_key.pem"
]
http_dest = "https://localhost:8181"
```

Additional notes:
- If multiple private keys are specified, only the first one found will be used.
- Certificates must be in x509 format, and private keys must be in PKCS8 format.
  - Private keys can be converted to PKCS8 using the following command: `openssl pkcs8 -topk8 -nocrypt -in input.pem -out output.pem`

#### Setting default HTTP headers
Although the web-server adds many useful HTTP headers to the response, the set of default headers is very minimal, and some users may wish to expand it. This can be done with the `[headers]` block.

The `[headers]` block sets the default HTTP header values for *all* requests, and is specified as a series of key = value pairs. This block will only add headers to the response if they are not already present, it will not overwrite existing headers. The order HTTP headers are returned in may change between requests and should not be relied on.

An example of a `[headers]` block is shown below:
```toml
[headers]
server = "KatWebSite"
```

The best practices for which HTTP headers should be included in your response is out of the scope for this document.

#### Global web server configuration
A `[server]` block must be specified in the configuration for the web server to start. If it is not specified, all `[[vhost]]` and `[headers]` blocks will be ignored.

Each `[server]` block can contain up to three options:
- `http_bind` - A list of all address:port pairs that the HTTP listener will attempt to bind to.
- `tls_bind` - A list of all address:port pairs that the HTTPS listener will attempt to bind to.
- `log_format` - The format used for request logs, has the same syntax as [actix-web's logger middleware](https://docs.rs/actix-web/3.3.2/actix_web/middleware/struct.Logger.html). Defaults to `%{Host}i %a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %D`.

An example of a `[server]` block is shown below:
```toml
[server]
http_bind = ["[::1]:8080", "127.0.0.1:8080"]
tls_bind = ["[::1]:8181", "127.0.0.1:8181"]
log_format = "%{Host}i %a \"%r\" %s %b \"%{Referer}i\" \"%{User-Agent}i\" %D"
```

Additional info:
- If neither `http_bind` or `tls_bind` are specified, the web server will not start.
- The web server supports the following technologies:
  - HTTP/1.1 & HTTP/2
  - On-the-fly Gzip/Brotli compression
  - Chunked transfer-encodinng
  - Partial requests and content-type detection for file handlers
  - A secure TLS 1.2 & 1.3 stack

---

## Logging
KatWebSite defaults to a fairly minimal amount of logging, to prevent the screen from being cluttered by information that most users would consider unimportant. However, you can increase the amount of information displayed in the log by starting KatWebSite with the `--verbose` flag, or the `-v` shorthand. This flag can be specified up to three times to further increase the logging, however most users will only need to specify the flag once or twice.

In addition, other users may find the default amount of logging too verbose for their use case and may want to decrease it. This can be done with `--quiet` flag, or the `-q` shorthand. This flag can be specified up to two times. The first time it's specifified, only warnings, errors, and HTTP request logs will be displayed. The second time it's specified, only errors will be displayed.

Note that the log level is actually controlled by an integer value, and these flags just increase/decrease it by 1. Because of this, both the `--quiet` and `--verbose` flags can be used at the same time, albeit with the result of them canceling each-other out (`-qv` would increase and decrease the log level, resulting in no net change).

### Advanced Logging
Some power users may want to have finer control over the logging than the CLI flags provide. Because KatWebSite uses `flexi_logger` internally, this is possible through environment variables.

The [`RUST_LOG`](https://docs.rs/env_logger/0.8.3/env_logger/#enabling-logging) environment variable can be used to get very fine grained control over which messages are printed to the log, completely overriding the CLI log level flags in the process of doing so.

The [`RUST_LOG_STYLE`](https://docs.rs/env_logger/0.8.3/env_logger/#disabling-colors) environment variable can be used to forcibly enable or disable colored logging messages.

The [`FLEXI_LOGGER_PALETTE`](https://docs.rs/flexi_logger/0.17.1/flexi_logger/struct.Logger.html#method.set_palette) environment variable can be used to change the color palette used for logging.

---

## Site generation

### Overview
Builders are run, one-at-a-time, before the web server is started. However, they run internal tasks, called Renderers, which are heavily multi-threaded. You can change the maximum number of threads available to Renderers using the `RAYON_NUM_THREADS` environment variable (defaults to the number of CPUs available).

Renderers are not typically run on their own, but in groups, to improve performance. Individual [Renderers can be enabled or disabled](#enabling-or-disabling-builder-renderers) in the configuration file.

The processing chain that Builders run is below:
1. Liquid include building
   - If the Liquid Renderer is enabled, all files in the Builder's `include_dir` are loaded into RAM, for later use in the Liquid renderer.
2. Data loading
   - If the data Renderer is enabled, all files in the Builder's `data_dir` are loaded and parsed as Liquid variables.
3. File scanning
   - All files in the Builder's `input_dir` are found and loaded into a list for use in later Renderers. However, there are some exceptions:
     - Subfolders are intentionally ignored, so that Builders with different configurations can be nested inside each-other.
     - Soft symbolic links are not loaded as ordinary files, but are later re-created in the Builder's `output` directory. This may be useful if you want to have the Builder "copy" over a folder full of static assets.
4. Page creation
   - All Files found by the file scanning are loaded into RAM and converted into Page objects.
   - If the Liquid Renderer is enabled, only files that contain frontmatter opening and closing tags (`---`) will be converted into Page objects. The frontmatter is then removed from the Page's content, and parsed as TOML into the Page's data section.
5. Site creation
   - All data found by the data Renderer, all files found by the file scanner, and all Page objects created are converted into a Site object for further processing.
6. Page building (part 1)
   - The Site object, along with the Liquid includes, is used to begin building all the Pages inside the Site.
     1. If the Liquid renderer is enabled, any Liquid inside the Page is rendered, using the Site object and Liquid includes as input.
     2. If the Markdown renderer is enabled and the Page contains a `.md` extension, the Page is rendered from Markdown to HTML.
     3. If the SASS renderer is enabled and the Page contains a `.sass` extension, the Page is rendered from SASS to CSS.
7. Page building (part 2)
   - The Site object, along with the Liquid includes, is used to finish building all the Pages inside the Site.
     1. If the HTML sanitizer is enabled and the Page contains HTML, the Page's HTML is sanitized.
     2. If the Layout renderer is enabled and a `layout` Liquid variable is set, the specified Liquid layout is loaded from `layout_dir` and applied to the Page.
8. Page writing
   - All Page objects inside the Site are written to the Builder's `output` directory.
9. File re-linking
   1. All soft symbolic links in the Builder's `input_dir` directory are found and turned into absolute paths.
   2. The soft symbolic links are re-created in the Builder's `output` directory, using the absolute path generated.
10. File copying
    - Files that were found by the file scanning, but were ignored by the page creation, are hard symbolic linked into the Builder's `output` directory. If hard symbolic linking is not possible, the file is copied instead.

### Liquid templating
The Liquid Renderer allows Pages to use the [Liquid templating language](https://shopify.github.io/liquid/) to dynamically generate Page content at build time. This Renderer uses an expanded version of the Liquid standard library provided by [`liquid-lib`](https://docs.rs/liquid-lib/0.22.0/liquid_lib), to allow for extra functionality like `{% include %}` blocks.

#### Frontmatter
This Renderer also handles loading frontmatter variables from Pages. If the Liquid renderer is enabled, frontmatter is removed from input files, and the text inside that frontmatter is parsed as TOML and turned into Liquid variables.

The beginning and end of frontmatter text is marked with `---`. An example of frontmatter above a Markdown document is shown below:

```markdown
---
title = "Hello world"
---
# My site
Welcome to my site!

```

#### Liquid variables
The allows you to access a limited portion of the Builder's current state through Liquid variables. As of the time of writing, the following variables can be accessed by Liquid templates:
- `site:` - The Builder's Site object
  - `pages: Array of [Type: Page]` - A list of all Pages in a site.
  - `files: Array of String` - A list of all files in the Builder's `input_dir` directory, except for soft symbolic links and subdirectories. This only contains filenames, not absolute paths.
  - `data: Array of Variables` - A list of all data loaded by the data Renderer. If the data Renderer is disabled, this array will have a length of zero.
- `page: [Type: Page]` - The Page currently being processed.
- `[Type: Page]:`
  - `path: String` - The filename of the Page.
  - `data: Variables` - All data loaded from the Page's frontmatter, if any.
  - `content: String` - The contents of the current page. If this is being called from a Page, the content will be the raw contents of the Page object. If this is being called from a Layout, the content will be the rendered output of the Page object.

#### The data Renderer
The data Renderer loads files from the `builder.default_dirs.data_dir` folder, and parses them into the site.data Liquid variable.

As of the time of writing, this Renderer assumes all files in this folder are valid TOML files, and attempts to parse them as such. In the future, support for other file types may be added.

**TODO: Fix loading order, add support for other file types**

### Liquid layouts
The layout Renderer can be used to apply Liquid layouts to pages, and runs as the final step in the processing chain. If the `layout` liquid variable is set, the Renderer will load the specified file from the `builder.default_dirs.layout_dir` folder, and render it like an ordinary Liquid template.

The rendering of a layout file is almost identical to the rendering of a Page, except for some important differences:
- The `page.content` Liquid variable contains the Page's rendered content instead of it's raw text, and should be used to fill in the content of the rendered file.
- If the layout Renderer is enabled but the Liquid Renderer is disabled, layouts will be unable to access frontmatter data (instead, `page.data` will contain an exact copy of the default Liquid variables, if any), frontmatter will not be removed from the `page.content` variable, and layouts will also be unable to access Liquid includes.

### File-type dependent Renderers
Some Renderers may only activate on certain file types. During a segment of the build chain (see the [site generation overview](#overview) for a list of all segments), only one file-type dependent renderer can be run at a time.

File types are detected purely based on file extension. If a file is not what it claims to be, the Renderer's parser may throw an error, generate nonsensical output, or both.

#### Markdown Renderer
The Markdown Renderer compiles Markdown files into HTML, and only activates on files with the `.md` extension. `Comrak` is used as the [CommonMark](https://commonmark.org/help/) renderer, with both [GFM and Comrak extensions](https://docs.rs/comrak/0.9.1/comrak/struct.ComrakExtensionOptions.html) enabled.

#### SASS CSS Renderer
The SASS Renderer compiles SASS files into CSS, and only activates on files with the `.sass` extension. [`Grass`](https://lib.rs/crates/grass) is used as the SASS compiler, and it lacks some major features found in [Dart Sass](https://sass-lang.com/documentation), such as the indented syntax, CSS imports, `@forward`, and compressed output. However, despite these major issues, and a few minor ones not listed here, the SASS Renderer is still capable of compiling most SASS files without issue.

#### HTML sanitizer Renderer
The HTML sanitizer Renderer can be used to sanitize untrusted HTML in a very restrictive way. `Ammonia` is as the HTML sanitizer, which is based on the [Servo browser engine](https://servo.org). Therefore, the sanitizer should be very robust and suitable for user provided input.
