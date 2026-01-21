// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct TensionArgs {}

pub async fn run(_args: TensionArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;
	let status = repo.tension()?;

	// Display the current stitch ID
	let stitch_hex = hex::encode(&status.current_stitch.0[..8]);
	println!("Stitch: {}", stitch_hex.yellow());

	if !status.description.is_empty() {
		println!("  {}", status.description);
	}

	if status.is_empty {
		println!("  {}", "(empty - no changes)".dimmed());
	} else {
		// Show file changes
		if !status.added.is_empty() {
			println!("\n{}", "Added:".green());
			for file in &status.added {
				println!("  + {}", file.green());
			}
		}

		if !status.modified.is_empty() {
			println!("\n{}", "Modified:".yellow());
			for file in &status.modified {
				println!("  ~ {}", file.yellow());
			}
		}

		if !status.removed.is_empty() {
			println!("\n{}", "Removed:".red());
			for file in &status.removed {
				println!("  - {}", file.red());
			}
		}

		// If all change lists are empty but not marked as empty, there are working copy changes
		if status.added.is_empty() && status.modified.is_empty() && status.removed.is_empty() {
			println!("  {}", "(working copy has changes)".cyan());
		}
	}

	Ok(())
}
