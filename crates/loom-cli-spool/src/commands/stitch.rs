// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct StitchArgs {}

pub async fn run(_args: StitchArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	let stitch_id = repo.stitch()?;
	let stitch_hex = hex::encode(&stitch_id.0[..8]);

	println!(
		"{} Created new stitch: {}",
		"✓".green(),
		stitch_hex.yellow()
	);

	Ok(())
}
