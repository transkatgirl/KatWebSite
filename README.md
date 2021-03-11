# KatWebSite
A minimal static site generator and web server.

## Installation
KatWebSite is currently beta software and must be compiled from source. Pre-compiled builds may be available in the future.

### Building from source
1. Install a [Rust compiler](https://www.rust-lang.org/learn/get-started) to your device.
2. Download the contents of the repository. This can be done by either using the Git CLI (`git clone https://github.com/katattakd/KatWebSite`) or by downloading a [zip archive of the repo's contents](https://github.com/katattakd/KatWebSite/archive/main.zip).
3. Open a terminal inside the downloaded repository, and run `cargo build --release` (or without `--release` for a debug build).
   - The compiled output will be in either `target/release/katwebsite` or `target/debug/katwebsite`.

## Configuration / Usage
A comprehensive documentation of KatWebSite's features and configuration can be found inside the [example.toml](https://github.com/katattakd/KatWebSite/blob/main/example.toml) file. It's highly recommended that you read this before attempting to use KatWebSite.

KatWebSite also has a basic CLI interface, which can be used to load different config files or change the logging verbosity. For a list of CLI flags, run `katsite --help`.

