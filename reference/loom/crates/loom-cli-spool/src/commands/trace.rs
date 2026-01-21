// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct TraceArgs {
	/// Revset query (defaults to @ for current working copy, use all() for full history)
	#[arg(default_value = "@")]
	pub revset: String,

	/// Maximum number of stitches to show
	#[arg(short, long, default_value = "10")]
	pub limit: usize,
}

pub async fn run(args: TraceArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;

	let stitches = repo.trace(&args.revset)?;

	for (i, stitch) in stitches.iter().take(args.limit).enumerate() {
		let stitch_hex = hex::encode(&stitch.id.0[..8]);

		// Show separator between stitches
		if i > 0 {
			println!();
		}

		// Stitch ID and description
		let id_display = if stitch.is_knotted {
			stitch_hex.green().to_string()
		} else {
			stitch_hex.yellow().to_string()
		};

		println!("{} {}", "○".cyan(), id_display);

		if !stitch.description.is_empty() {
			// Show first line of description
			let first_line = stitch.description.lines().next().unwrap_or("");
			println!("  {}", first_line);
		} else {
			println!("  {}", "(no description)".dimmed());
		}

		// Author and timestamp
		println!(
			"  {} {} <{}>",
			"Author:".dimmed(),
			stitch.author.name,
			stitch.author.email
		);

		println!(
			"  {} {}",
			"Date:".dimmed(),
			stitch.author.timestamp.format("%Y-%m-%d %H:%M:%S")
		);
	}

	if stitches.len() > args.limit {
		println!(
			"\n{}",
			format!("... and {} more", stitches.len() - args.limit).dimmed()
		);
	}

	Ok(())
}
