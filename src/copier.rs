#![warn(clippy::all)]

use fs_extra::dir::{copy, CopyOptions};
use log::info;
use serde_derive::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize, Clone, Debug)]
pub struct Copier {
	pub input_dir: PathBuf,
	pub output: PathBuf,

	#[serde(default)]
	pub overwrite: bool,
}

pub fn run_copier(copier: &Copier) -> fs_extra::error::Result<u64> {
	info!("Copying {:?} to {:?}", &copier.input_dir, &copier.output);

	copy(
		&copier.input_dir,
		&copier.output,
		&CopyOptions {
			overwrite: copier.overwrite,
			skip_exist: !copier.overwrite,
			buffer_size: 64000, // ignored for copy_items
			copy_inside: true,
			content_only: true,
			depth: 0,
		},
	)
}
