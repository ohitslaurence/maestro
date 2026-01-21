// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct SnipArgs {
	/// The stitch ID to abandon (hex string, can be abbreviated)
	pub stitch: String,
}

pub async fn run(args: SnipArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	// Parse the stitch ID from hex
	let stitch_id = parse_stitch_id(&args.stitch)?;

	repo.snip(&stitch_id)?;

	let stitch_hex = hex::encode(&stitch_id.0[..8]);
	println!(
		"{} Snipped (abandoned) stitch {}",
		"✓".green(),
		stitch_hex.red()
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
