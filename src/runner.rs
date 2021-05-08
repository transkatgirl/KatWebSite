#![warn(clippy::all)]

use log::{error, info};
use serde_derive::Deserialize;
use std::process::Command;

#[derive(Deserialize, Clone, Debug)]
pub struct Runner {
	pub command: String,

	#[serde(default)]
	pub args: Vec<String>,
}

pub fn run_runner(runner: &Runner) -> bool {
	if runner.args.is_empty() {
		info!("Running {:?}", &runner.command);
	} else {
		info!("Running {:?} with args {:?}", &runner.command, &runner.args);
	}

	match Command::new(&runner.command).args(&runner.args).status() {
		Ok(status) => {
			if !status.success() {
				error!("Command {:?} exited with an error!", &runner.command);
				return false;
			}

			true
		}
		Err(err) => {
			error!("Unable to run {:?}! {}", &runner.command, err);

			false
		}
	}
}
