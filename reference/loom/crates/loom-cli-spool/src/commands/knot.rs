// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct KnotArgs {
	/// The description/message for this knot
	#[arg(short, long)]
	pub message: Option<String>,
}

pub async fn run(args: KnotArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let message = args.message.unwrap_or_default();

	if message.is_empty() {
		anyhow::bail!("Message required. Use -m/--message to provide a description.");
	}

	repo.knot(&message)?;

	println!("{} Knotted current stitch", "✓".green());
	println!("  {}", message.dimmed());

	Ok(())
}
