// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct UnravelArgs {
	/// The stitch to split (hex string). Defaults to current (@).
	pub stitch: Option<String>,

	/// Interactive mode for selecting changes
	#[arg(short, long)]
	pub interactive: bool,
}

pub async fn run(args: UnravelArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;

	let stitch_hex = if let Some(ref s) = args.stitch {
		let id = parse_stitch_id(s)?;
		hex::encode(&id.0[..8])
	} else {
		let status = repo.tension()?;
		hex::encode(&status.current_stitch.0[..8])
	};

	// Unravel (split) is complex and requires interactive selection
	// For now, provide a helpful message
	println!(
		"{} Unravel requires interactive change selection",
		"Note:".yellow().bold()
	);
	println!();
	println!("To split stitch {}:", stitch_hex.cyan());
	println!(
		"  1. Use 'spool edit {}' to make the stitch editable",
		stitch_hex
	);
	println!("  2. Create new stitches with 'spool stitch'");
	println!("  3. Move changes between stitches using file operations");
	println!();
	println!(
		"{}",
		"Full interactive unravel not yet implemented".dimmed()
	);

	Ok(())
}

fn parse_stitch_id(s: &str) -> anyhow::Result<StitchId> {
	let bytes = hex::decode(s)?;

	if bytes.len() > 16 {
		anyhow::bail!("stitch ID too long");
	}

	let mut arr = [0u8; 16];
	arr[..bytes.len()].copy_from_slice(&bytes);

	Ok(StitchId(arr))
}
