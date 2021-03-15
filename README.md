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

## Running the demo page
KatWebSite's demo page and documentation can be built and run by running `katwebsite -c examples/config.toml`. You can then access the page through http://localhost:8080.

Note: If you're using the CI builds, you will have to download the additional-files artifact and extract it's contents into the *same directory as the KatWebSite binary* before attempting to run the demo page. All further documentation assumes this step has already been completed.

It's highly recommended that you read the built-in documentation before attempting to use KatWebSite, even if you are already familiar with other static site generators / web servers.

