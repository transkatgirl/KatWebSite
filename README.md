# KatWebSite
A minimal static site generator and web server.

## Installation
KatWebSite is currently unfinished beta software, and does not offer stable releases at this time.

### Downloading CI builds
KatWebSite uses GitHub Actions to test and compile every commit on different operating systems and CPU architectures.

The [Rust workflow](https://github.com/katattakd/KatWebSite/actions/workflows/rust.yml?query=is%3Asuccess) contains the compiled binaries as artifacts, which can be downloaded for any commit within the past 30 days. Due to a limitation of GitHub Actions, downloading artifacts currently requires an account.

### Building from source
1. Install a [Rust compiler](https://www.rust-lang.org/learn/get-started) to your device.
2. Download the contents of the repository. This can be done by either using the Git CLI (`git clone https://github.com/katattakd/KatWebSite`) or by downloading a [zip archive of the repo's contents](https://github.com/katattakd/KatWebSite/archive/main.zip).
3. Open a terminal inside the downloaded repository, and run `cargo build --release` (or without `--release` for a debug build).
   - The compiled output will be in either `target/release/katwebsite` or `target/debug/katwebsite`.

## Configuration / Usage
A comprehensive documentation of KatWebSite's features and configuration can be found inside the [core.md](https://github.com/katattakd/KatWebSite/blob/main/docs/core.md) file. It's highly recommended that you read this before attempting to use KatWebSite.

KatWebSite also has a basic CLI interface, which can be used to load different config files or change the logging verbosity. For a list of CLI flags, run `katsite --help`.

