// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::commands::tunnel::CliContext;
use crate::keys;
use crate::tunnel::{TunnelConfig, TunnelManager};
use clap::Args;
use console::style;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, instrument, warn};

#[derive(Debug, Clone, Args)]
pub struct SshArgs {
	/// Weaver ID to connect to
	pub weaver_id: String,

	/// SSH port (default: 22)
	#[arg(long, default_value = "22")]
	pub port: u16,

	/// SSH user (default: loom)
	#[arg(long, default_value = "loom")]
	pub user: String,

	/// SSH identity file
	#[arg(long, short = 'i')]
	pub identity: Option<String>,

	/// Additional SSH arguments
	#[arg(last = true)]
	pub ssh_args: Vec<String>,
}

#[instrument(skip(ctx), fields(weaver_id = %args.weaver_id))]
pub async fn handle_ssh(args: SshArgs, ctx: &CliContext) -> anyhow::Result<()> {
	let client = ctx.client()?;

	let keypair = keys::get_or_create_device_key(&ctx.config_dir).await?;
	info!(public_key = %keypair.public_key(), "loaded device key");

	let device = client.ensure_device_registered(&keypair).await?;
	info!(device_id = %device.id, "device registered");

	let derp_map = client.get_derp_map().await?;

	let session = client.create_session(&args.weaver_id, &device.id).await?;
	info!(
		session_id = %session.session_id,
		client_ip = %session.client_ip,
		weaver_ip = %session.weaver.ip,
		"created session"
	);

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
	manager.add_weaver(&args.weaver_id, &session).await?;

	println!(
		"{} Connected to weaver {}",
		style("✓").green().bold(),
		style(&args.weaver_id).cyan()
	);

	tokio::time::sleep(std::time::Duration::from_millis(500)).await;

	let known_hosts_path = ctx.config_dir.join("known_hosts");

	let ssh_target = format!("{}@{}", args.user, session.weaver.ip);

	let mut cmd = Command::new("ssh");
	cmd
		.arg("-o")
		.arg(format!("UserKnownHostsFile={}", known_hosts_path.display()))
		.arg("-o")
		.arg("StrictHostKeyChecking=accept-new")
		.arg("-p")
		.arg(args.port.to_string());

	if let Some(identity) = &args.identity {
		cmd.arg("-i").arg(identity);
	}

	cmd.arg(&ssh_target);
	cmd.args(&args.ssh_args);

	cmd.stdin(Stdio::inherit());
	cmd.stdout(Stdio::inherit());
	cmd.stderr(Stdio::inherit());

	info!(target = %ssh_target, "spawning SSH");

	let status = cmd.status().await?;

	info!("SSH exited with status: {:?}", status.code());

	manager.shutdown().await;

	if let Err(e) = client.delete_session(&session.session_id).await {
		warn!("failed to delete session: {}", e);
	}

	println!("{} Disconnected", style("✓").green().bold());

	std::process::exit(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_ssh_args_parse() {
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(flatten)]
			args: SshArgs,
		}

		let cli = TestCli::parse_from(["test", "my-weaver"]);
		assert_eq!(cli.args.weaver_id, "my-weaver");
		assert_eq!(cli.args.port, 22);
		assert_eq!(cli.args.user, "loom");
	}

	#[test]
	fn test_ssh_args_with_options() {
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(flatten)]
			args: SshArgs,
		}

		let cli = TestCli::parse_from([
			"test",
			"my-weaver",
			"--port",
			"2222",
			"--user",
			"root",
			"-i",
			"~/.ssh/id_rsa",
			"--",
			"-L",
			"3000:localhost:3000",
		]);
		assert_eq!(cli.args.weaver_id, "my-weaver");
		assert_eq!(cli.args.port, 2222);
		assert_eq!(cli.args.user, "root");
		assert_eq!(cli.args.identity, Some("~/.ssh/id_rsa".to_string()));
		assert_eq!(cli.args.ssh_args, vec!["-L", "3000:localhost:3000"]);
	}
}
