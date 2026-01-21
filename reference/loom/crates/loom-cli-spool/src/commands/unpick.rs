// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct UnpickArgs {
	/// Number of operations to undo (default: 1)
	#[arg(short, long, default_value = "1")]
	pub count: usize,
}

pub async fn run(args: UnpickArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	if args.count != 1 {
		anyhow::bail!("Only single-operation undo is currently supported");
	}

	// Get current state before undo
	let status_before = repo.tension()?;
	let stitch_before = hex::encode(&status_before.current_stitch.0[..8]);

	repo.unpick()?;

	// Get state after undo
	let status_after = repo.tension()?;
	let stitch_after = hex::encode(&status_after.current_stitch.0[..8]);

	println!("{} Undid last operation", "✓".green());
	println!();
	println!("Before: stitch {}", stitch_before.dimmed());
	println!("After:  stitch {}", stitch_after.cyan());

	Ok(())
}
