// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct DuplicateArgs {
	/// The stitch to duplicate (hex string). Defaults to current (@).
	pub stitch: Option<String>,
}

pub async fn run(args: DuplicateArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let stitch_id = if let Some(ref s) = args.stitch {
		parse_stitch_id(s)?
	} else {
		repo.tension()?.current_stitch
	};

	let stitch_hex = hex::encode(&stitch_id.0[..8]);

	let new_id = repo.duplicate(&stitch_id)?;
	let new_hex = hex::encode(&new_id.0[..8]);

	println!(
		"{} Duplicated stitch {} -> {}",
		"✓".green(),
		stitch_hex.dimmed(),
		new_hex.yellow()
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
