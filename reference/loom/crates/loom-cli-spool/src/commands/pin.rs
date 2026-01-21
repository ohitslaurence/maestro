// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::env;

use colored::Colorize;
use loom_common_spool::SpoolRepo;

#[derive(Debug, Clone, clap::Args)]
pub struct PinArgs {
	#[command(subcommand)]
	pub command: PinSubcommand,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum PinSubcommand {
	/// List all pins (bookmarks)
	List,
	/// Create a new pin at the current stitch
	Create {
		/// Name of the pin to create
		name: String,
	},
	/// Delete a pin
	Delete {
		/// Name of the pin to delete
		name: String,
	},
	/// Move a pin to the current stitch
	Move {
		/// Name of the pin to move
		name: String,
	},
}

pub async fn run(args: PinArgs) -> anyhow::Result<()> {
	let path = env::current_dir()?;
	let mut repo = SpoolRepo::open(&path)?;

	match args.command {
		PinSubcommand::List => {
			let pins = repo.pins()?;

			if pins.is_empty() {
				println!("{}", "No pins defined".dimmed());
			} else {
				println!("{}", "Pins:".bold());
				for pin in pins {
					let target_hex = hex::encode(&pin.target.0[..8]);
					println!("  {} -> {}", pin.name.cyan(), target_hex.yellow());
				}
			}
		}
		PinSubcommand::Create { name } => {
			repo.pin_create(&name)?;

			let status = repo.tension()?;
			let stitch_hex = hex::encode(&status.current_stitch.0[..8]);

			println!(
				"{} Created pin '{}' at stitch {}",
				"✓".green(),
				name.cyan(),
				stitch_hex.yellow()
			);
		}
		PinSubcommand::Delete { name } => {
			repo.pin_delete(&name)?;

			println!("{} Deleted pin '{}'", "✓".green(), name.cyan());
		}
		PinSubcommand::Move { name } => {
			repo.pin_move(&name)?;

			let status = repo.tension()?;
			let stitch_hex = hex::encode(&status.current_stitch.0[..8]);

			println!(
				"{} Moved pin '{}' to stitch {}",
				"✓".green(),
				name.cyan(),
				stitch_hex.yellow()
			);
		}
	}

	Ok(())
}
