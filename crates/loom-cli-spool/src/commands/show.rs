// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct ShowArgs {
	/// The stitch to show (hex string, can be abbreviated). Defaults to current (@).
	pub stitch: Option<String>,
}

pub async fn run(args: ShowArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;

	let stitch = if let Some(stitch_str) = args.stitch {
		// Parse the provided stitch ID
		let id = parse_stitch_id(&stitch_str)?;
		repo
			.trace("@")?
			.into_iter()
			.find(|s| s.id == id)
			.ok_or_else(|| anyhow::anyhow!("stitch not found"))?
	} else {
		// Get the current stitch (@)
		repo
			.trace("@")?
			.into_iter()
			.next()
			.ok_or_else(|| anyhow::anyhow!("no current stitch"))?
	};

	// Display stitch details
	let stitch_hex = hex::encode(&stitch.id.0[..8]);

	println!("{}: {}", "Stitch".bold(), stitch_hex.yellow());
	println!(
		"{}: {} <{}>",
		"Author".bold(),
		stitch.author.name,
		stitch.author.email
	);
	println!(
		"{}: {}",
		"Date".bold(),
		stitch.author.timestamp.format("%Y-%m-%d %H:%M:%S %z")
	);

	if !stitch.parents.is_empty() {
		let parent_strs: Vec<String> = stitch
			.parents
			.iter()
			.map(|p| hex::encode(&p.0[..8]))
			.collect();
		println!("{}: {}", "Parents".bold(), parent_strs.join(", "));
	}

	println!();

	if stitch.description.is_empty() {
		println!("{}", "(no description)".dimmed());
	} else {
		println!("{}", stitch.description);
	}

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
