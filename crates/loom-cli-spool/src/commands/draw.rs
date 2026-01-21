// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct DrawArgs {
	/// Remote to fetch from (default: origin)
	#[arg(default_value = "origin")]
	pub remote: String,

	/// Fetch all remotes
	#[arg(long)]
	pub all: bool,
}

pub async fn run(args: DrawArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let remote = if args.all { "(all)" } else { &args.remote };

	println!("{} Drawing from remote '{}'", "Fetch".bold(), remote.cyan());
	println!();

	match repo.draw(&args.remote) {
		Ok(()) => {
			println!("{} Successfully fetched from {}", "✓".green(), args.remote);
		}
		Err(e) => {
			println!("{} {}", "Error:".red().bold(), e);
			println!();
			println!(
				"{}",
				"Git fetch requires a colocated git repository (wind --git)".dimmed()
			);
		}
	}

	Ok(())
}
