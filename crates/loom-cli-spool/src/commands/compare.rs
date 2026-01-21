// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct CompareArgs {
	/// First stitch (defaults to parent of @)
	#[arg(short = 'f', long)]
	pub from: Option<String>,

	/// Second stitch (defaults to @)
	#[arg(short = 't', long)]
	pub to: Option<String>,
}

pub async fn run(args: CompareArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;

	// Get the tension status which includes change info
	let status = repo.tension()?;

	let from_desc = args.from.as_deref().unwrap_or("parent");
	let to_desc = args.to.as_deref().unwrap_or("@");

	println!(
		"{} {} {} {}",
		"Comparing".bold(),
		from_desc.cyan(),
		"to".bold(),
		to_desc.cyan()
	);
	println!();

	// Currently we show basic status; full diff requires tree comparison
	if status.is_empty {
		println!("{}", "No changes between stitches".dimmed());
	} else {
		println!("{}: Working copy has modifications", "Status".bold());

		if !status.added.is_empty() {
			println!("\n{}:", "Added".green().bold());
			for file in &status.added {
				println!("  {} {}", "+".green(), file);
			}
		}

		if !status.modified.is_empty() {
			println!("\n{}:", "Modified".yellow().bold());
			for file in &status.modified {
				println!("  {} {}", "~".yellow(), file);
			}
		}

		if !status.removed.is_empty() {
			println!("\n{}:", "Removed".red().bold());
			for file in &status.removed {
				println!("  {} {}", "-".red(), file);
			}
		}
	}

	// Note: Full diff output would require tree-to-tree comparison
	// which is not yet implemented in the library layer
	if args.from.is_some() || args.to.is_some() {
		println!();
		println!(
			"{}",
			"Note: Full diff between arbitrary stitches not yet implemented".dimmed()
		);
	}

	Ok(())
}
