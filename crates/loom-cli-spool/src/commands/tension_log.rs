// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct TensionLogArgs {
	/// Number of operations to show
	#[arg(short, long, default_value = "10")]
	pub limit: usize,
}

pub async fn run(args: TensionLogArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let repo = SpoolRepo::open(&path)?;

	let entries = repo.tension_log(args.limit)?;

	println!("{}", "Operation Log (Tension Log)".bold());
	println!("{}", "=".repeat(60));
	println!();

	if entries.is_empty() {
		println!("{}", "No operations in history".dimmed());
	} else {
		for entry in entries {
			let op_hex = hex::encode(&entry.operation_id.0[..8]);
			let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M:%S");

			println!(
				"{} {} {}",
				op_hex.yellow(),
				timestamp.to_string().dimmed(),
				entry.description
			);
		}
	}

	println!();
	println!("Use '{}' to undo the last operation", "spool unpick".cyan());

	Ok(())
}
