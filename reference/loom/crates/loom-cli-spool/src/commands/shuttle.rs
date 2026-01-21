// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct ShuttleArgs {
	/// Remote to push to (default: origin)
	#[arg(default_value = "origin")]
	pub remote: String,

	/// Pins (bookmarks/branches) to push
	#[arg(long)]
	pub pins: Vec<String>,

	/// Push all pins
	#[arg(long)]
	pub all: bool,
}

pub async fn run(args: ShuttleArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let pins: Vec<String> = if args.all {
		vec!["(all)".to_string()]
	} else if args.pins.is_empty() {
		vec!["main".to_string()]
	} else {
		args.pins.clone()
	};

	println!(
		"{} Shuttling to remote '{}'",
		"Push".bold(),
		args.remote.cyan()
	);
	println!("Pins: {}", pins.join(", ").yellow());
	println!();

	match repo.shuttle(&args.remote, &pins) {
		Ok(()) => {
			println!("{} Successfully pushed to {}", "✓".green(), args.remote);
		}
		Err(e) => {
			println!("{} {}", "Error:".red().bold(), e);
			println!();
			println!(
				"{}",
				"Git push requires a colocated git repository (wind --git)".dimmed()
			);
		}
	}

	Ok(())
}
