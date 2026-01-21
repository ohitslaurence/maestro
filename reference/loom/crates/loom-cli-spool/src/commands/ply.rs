// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct PlyArgs {
	/// The stitch to squash (hex string)
	pub source: String,

	/// The stitch to squash into (hex string)
	pub dest: String,
}

pub async fn run(args: PlyArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let source_id = parse_stitch_id(&args.source)?;
	let dest_id = parse_stitch_id(&args.dest)?;

	repo.ply(&source_id, &dest_id)?;

	let source_hex = hex::encode(&source_id.0[..8]);
	let dest_hex = hex::encode(&dest_id.0[..8]);

	println!(
		"{} Plied (squashed) {} into {}",
		"✓".green(),
		source_hex.yellow(),
		dest_hex.cyan()
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
