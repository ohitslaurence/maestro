// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::commands::tunnel::CliContext;
use crate::keys;
use clap::{Args, Subcommand};
use console::style;
use tracing::instrument;

#[derive(Debug, Subcommand)]
pub enum DevicesCommands {
	/// List registered devices
	List,
	/// Register current device
	Register(RegisterArgs),
	/// Revoke a device
	Revoke(RevokeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct RegisterArgs {
	/// Human-readable name for this device
	#[arg(long)]
	pub name: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct RevokeArgs {
	/// Device ID to revoke
	pub device_id: String,
}

#[instrument(skip(ctx))]
pub async fn handle_list(ctx: &CliContext) -> anyhow::Result<()> {
	let client = ctx.client()?;

	let devices = client.list_devices().await?;

	if devices.is_empty() {
		println!("No devices registered");
		return Ok(());
	}

	println!(
		"{:<38} {:<44} {:<20} {}",
		style("ID").bold().underlined(),
		style("PUBLIC KEY").bold().underlined(),
		style("NAME").bold().underlined(),
		style("CREATED").bold().underlined()
	);

	for device in devices {
		let name = device.name.as_deref().unwrap_or("-");
		let created = device.created_at.format("%Y-%m-%d %H:%M").to_string();
		let pub_key_short = if device.public_key.len() > 40 {
			format!("{}...", &device.public_key[..40])
		} else {
			device.public_key.clone()
		};

		println!(
			"{:<38} {:<44} {:<20} {}",
			device.id,
			style(pub_key_short).dim(),
			name,
			style(created).dim()
		);
	}

	Ok(())
}

#[instrument(skip(ctx))]
pub async fn handle_register(args: RegisterArgs, ctx: &CliContext) -> anyhow::Result<()> {
	let client = ctx.client()?;

	let keypair = keys::get_or_create_device_key(&ctx.config_dir).await?;

	let device_id = uuid::Uuid::new_v4().to_string();
	let device = client
		.register_device(&device_id, keypair.public_key(), args.name.as_deref())
		.await?;

	println!("{} Device registered", style("✓").green().bold());
	println!("  ID:         {}", style(&device.id).cyan());
	println!("  Public Key: {}", style(&device.public_key).dim());
	if let Some(name) = &device.name {
		println!("  Name:       {}", name);
	}

	Ok(())
}

#[instrument(skip(ctx))]
pub async fn handle_revoke(args: RevokeArgs, ctx: &CliContext) -> anyhow::Result<()> {
	let client = ctx.client()?;

	client.revoke_device(&args.device_id).await?;

	println!(
		"{} Device {} revoked",
		style("✓").green().bold(),
		style(&args.device_id).cyan()
	);

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_devices_commands_parse() {
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(subcommand)]
			cmd: DevicesCommands,
		}

		let cli = TestCli::parse_from(["test", "list"]);
		assert!(matches!(cli.cmd, DevicesCommands::List));

		let cli = TestCli::parse_from(["test", "revoke", "device-123"]);
		match cli.cmd {
			DevicesCommands::Revoke(args) => {
				assert_eq!(args.device_id, "device-123");
			}
			_ => panic!("expected Revoke command"),
		}
	}

	#[test]
	fn test_register_args_parse() {
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(subcommand)]
			cmd: DevicesCommands,
		}

		let cli = TestCli::parse_from(["test", "register", "--name", "My Laptop"]);
		match cli.cmd {
			DevicesCommands::Register(args) => {
				assert_eq!(args.name, Some("My Laptop".to_string()));
			}
			_ => panic!("expected Register command"),
		}
	}
}
