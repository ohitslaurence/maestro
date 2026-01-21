// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;
use std::path::PathBuf;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, TangleSide};

#[derive(Debug, Clone, clap::Args)]
pub struct UntangleArgs {
	/// Path to the conflicted file
	pub path: PathBuf,

	/// Resolution strategy: ours, theirs, or base
	#[arg(short, long, default_value = "ours")]
	pub strategy: String,
}

pub async fn run(args: UntangleArgs) -> anyhow::Result<()> {
	let cwd = env::current_dir()?;
	let mut repo = SpoolRepo::open(&cwd)?;

	// Check for tangles (conflicts)
	let tangles = repo.tangles()?;

	if tangles.is_empty() {
		println!("{}", "No conflicts (tangles) to resolve".green());
		return Ok(());
	}

	let resolution = match args.strategy.as_str() {
		"ours" => TangleSide::Ours,
		"theirs" => TangleSide::Theirs,
		"base" => TangleSide::Base,
		other => anyhow::bail!("Unknown strategy '{}'. Use: ours, theirs, or base", other),
	};

	repo.untangle(&args.path, resolution)?;

	println!(
		"{} Resolved conflict in '{}' using {} side",
		"✓".green(),
		args.path.display().to_string().cyan(),
		args.strategy.yellow()
	);

	Ok(())
}
