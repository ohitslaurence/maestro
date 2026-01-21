// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;
use std::path::PathBuf;

use colored::Colorize;
use loom_common_spool::{SpoolRepo, StitchId};

#[derive(Debug, Clone, clap::Args)]
pub struct MendArgs {
	/// Path to restore (or all if not specified)
	pub path: Option<PathBuf>,

	/// Restore from a specific stitch
	#[arg(short, long)]
	pub from: Option<String>,
}

pub async fn run(args: MendArgs) -> anyhow::Result<()> {
	let cwd = env::current_dir()?;
	let mut repo = SpoolRepo::open(&cwd)?;

	let from_id = if let Some(ref s) = args.from {
		Some(parse_stitch_id(s)?)
	} else {
		None
	};

	let from_desc = if let Some(ref id) = from_id {
		hex::encode(&id.0[..8])
	} else {
		"parent".to_string()
	};

	repo.mend(args.path.as_deref(), from_id.as_ref())?;

	if let Some(ref path) = args.path {
		println!(
			"{} Restored '{}' from {}",
			"✓".green(),
			path.display().to_string().cyan(),
			from_desc.yellow()
		);
	} else {
		println!(
			"{} Restored all files from {}",
			"✓".green(),
			from_desc.yellow()
		);
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
