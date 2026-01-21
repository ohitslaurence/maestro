// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::client::WgTunnelClient;
use crate::keys;
use crate::tunnel::{TunnelConfig, TunnelManager};
use clap::{Args, Subcommand};
use console::style;
use loom_common_secret::SecretString;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument};
use url::Url;

#[derive(Debug, Subcommand)]
pub enum TunnelCommands {
	/// Start the WireGuard tunnel
	Up(TunnelUpArgs),
	/// Stop the WireGuard tunnel
	Down,
	/// Show tunnel status
	Status,
}

#[derive(Debug, Clone, Args)]
pub struct TunnelUpArgs {
	/// Connect only to this weaver
	#[arg(long)]
	pub weaver: Option<String>,

	/// Run in background
	#[arg(long)]
	pub detach: bool,
}

pub struct CliContext {
	pub server_url: Url,
	pub auth_token: SecretString,
	pub config_dir: std::path::PathBuf,
	pub tunnel_manager: RwLock<Option<Arc<TunnelManager>>>,
}

impl CliContext {
	pub fn new(server_url: Url, auth_token: SecretString, config_dir: std::path::PathBuf) -> Self {
		Self {
			server_url,
			auth_token,
			config_dir,
			tunnel_manager: RwLock::new(None),
		}
	}

	pub fn client(&self) -> crate::error::Result<WgTunnelClient> {
		WgTunnelClient::new(self.server_url.clone(), self.auth_token.clone())
	}
}

#[instrument(skip(ctx))]
pub async fn handle_up(args: TunnelUpArgs, ctx: &CliContext) -> anyhow::Result<()> {
	let client = ctx.client()?;

	let keypair = keys::get_or_create_device_key(&ctx.config_dir).await?;
	info!(public_key = %keypair.public_key(), "loaded device key");

	let device = client.ensure_device_registered(&keypair).await?;
	info!(device_id = %device.id, "device registered");

	let derp_map = client.get_derp_map().await?;

	let weaver_id = args.weaver.as_ref().ok_or_else(|| {
		anyhow::anyhow!("--weaver is required for 'tunnel up' (multi-weaver mode not yet implemented)")
	})?;

	let session = client.create_session(weaver_id, &device.id).await?;
	info!(session_id = %session.session_id, client_ip = %session.client_ip, "created session");

	let client_ip: std::net::Ipv6Addr = session
		.client_ip
		.parse()
		.map_err(|e| anyhow::anyhow!("invalid client IP: {}", e))?;

	let tunnel_config = TunnelConfig::new(
		keypair,
		client_ip,
		derp_map,
		session.weaver.derp_home_region,
	);

	let manager = TunnelManager::start(tunnel_config).await?;
	manager.add_weaver(weaver_id, &session).await?;

	let manager = Arc::new(manager);

	{
		let mut tunnel_guard = ctx.tunnel_manager.write().await;
		*tunnel_guard = Some(Arc::clone(&manager));
	}

	println!("{} Tunnel started", style("✓").green().bold());
	println!("  Client IP: {}", style(&session.client_ip).cyan());
	println!("  Weaver IP: {}", style(&session.weaver.ip).cyan());
	println!("  Weaver:    {}", style(weaver_id).cyan());

	if args.detach {
		println!("\nRunning in background. Use 'loom tunnel down' to stop.");
	} else {
		println!("\nPress Ctrl+C to stop the tunnel...");

		tokio::select! {
			_ = tokio::signal::ctrl_c() => {
				println!("\n{} Shutting down...", style("→").yellow());
			}
			_ = manager.wait() => {}
		}

		manager.shutdown().await;
		client.delete_session(&session.session_id).await.ok();

		println!("{} Tunnel stopped", style("✓").green().bold());
	}

	Ok(())
}

#[instrument(skip(ctx))]
pub async fn handle_down(ctx: &CliContext) -> anyhow::Result<()> {
	let manager = {
		let mut tunnel_guard = ctx.tunnel_manager.write().await;
		tunnel_guard.take()
	};

	if let Some(manager) = manager {
		manager.shutdown().await;
		println!("{} Tunnel stopped", style("✓").green().bold());
	} else {
		println!("{} No tunnel is running", style("!").yellow().bold());
	}

	Ok(())
}

#[instrument(skip(ctx))]
pub async fn handle_status(ctx: &CliContext) -> anyhow::Result<()> {
	let tunnel_guard = ctx.tunnel_manager.read().await;

	if let Some(manager) = tunnel_guard.as_ref() {
		let status = manager.status().await;

		if status.running {
			println!("{} Tunnel is running", style("●").green().bold());
			if let Some(ip) = status.our_ip {
				println!("  Our IP: {}", style(ip).cyan());
			}
			println!();

			if status.connected_weavers.is_empty() {
				println!("  No weavers connected");
			} else {
				println!("  Connected weavers:");
				for weaver in &status.connected_weavers {
					println!(
						"    {} {} ({})",
						style("•").green(),
						weaver.weaver_id,
						weaver.ip
					);
					println!("      Path: {}", style(&weaver.path_type).dim());
					if let Some(handshake) = &weaver.last_handshake {
						println!("      Last handshake: {}", style(handshake).dim());
					}
				}
			}
		} else {
			println!("{} Tunnel is stopped", style("●").red().bold());
		}
	} else {
		println!("{} Tunnel is not running", style("●").dim());
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_tunnel_up_args_parse() {
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(subcommand)]
			cmd: TunnelCommands,
		}

		let cli = TestCli::parse_from(["test", "up", "--weaver", "my-weaver", "--detach"]);
		match cli.cmd {
			TunnelCommands::Up(args) => {
				assert_eq!(args.weaver, Some("my-weaver".to_string()));
				assert!(args.detach);
			}
			_ => panic!("expected Up command"),
		}
	}
}
