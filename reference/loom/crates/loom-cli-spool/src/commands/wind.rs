// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct WindArgs {
	/// Create a colocated Git repository for Git interoperability
	#[arg(long)]
	pub git: bool,
}

pub async fn run(args: WindArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;

	// Initialize the spool repository
	let repo = SpoolRepo::wind(&path, args.git)?;

	println!(
		"{} Initialized spool repository at {}",
		"✓".green(),
		repo.root().display()
	);

	if args.git {
		println!("  {} Colocated with Git", "→".cyan());
	}

	Ok(())
}
