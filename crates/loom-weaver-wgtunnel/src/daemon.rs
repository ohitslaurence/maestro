// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::config::WeaverWgConfig;
use crate::error::{DaemonError, PeerError, Result};
use crate::peer_handler::{PeerEvent, PeerHandler};
use crate::registration::Registration;
use loom_wgtunnel_common::{DerpMap, WgKeyPair, WgPublicKey};
use loom_wgtunnel_engine::{PeerConfig, WgEngine, WgEngineConfig};
use std::net::Ipv6Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, error, info, instrument, warn};

pub struct WeaverWgDaemon {
	engine: Option<Arc<WgEngine>>,
	registration: Registration,
	peer_handler: PeerHandler,
	config: WeaverWgConfig,
	keypair: WgKeyPair,
	shutdown_tx: watch::Sender<bool>,
	shutdown_rx: watch::Receiver<bool>,
}

impl WeaverWgDaemon {
	#[instrument(skip(config), fields(weaver_id = %config.weaver_id))]
	pub async fn new(config: WeaverWgConfig) -> Result<Self> {
		let keypair = WgKeyPair::generate();

		info!("generated ephemeral WireGuard keypair");

		let registration = Registration::new(config.server_url.clone(), config.weaver_id.clone())?;

		let peer_handler = PeerHandler::new();

		let (shutdown_tx, shutdown_rx) = watch::channel(false);

		Ok(Self {
			engine: None,
			registration,
			peer_handler,
			config,
			keypair,
			shutdown_tx,
			shutdown_rx,
		})
	}

	#[instrument(skip(self))]
	pub async fn run(&mut self) -> Result<()> {
		if !self.config.enabled {
			info!("WireGuard tunnel disabled, exiting");
			return Ok(());
		}

		info!("starting weaver WireGuard daemon");

		let svid = self.registration.get_svid().await?;
		info!("obtained SVID for authentication");

		let registration_response = self
			.registration
			.register(self.keypair.public_key(), self.config.derp_home_region)
			.await?;

		let assigned_ip: Ipv6Addr = registration_response
			.assigned_ip
			.parse()
			.map_err(crate::error::RegistrationError::IpParse)?;
		info!(%assigned_ip, "registered with server, got assigned IP");

		let derp_map: DerpMap =
			serde_json::from_value(registration_response.derp_map).unwrap_or_else(|e| {
				warn!(error = %e, "failed to parse DERP map, using default");
				DerpMap::default()
			});

		let engine_config = WgEngineConfig {
			private_key: self.keypair.clone(),
			address: assigned_ip,
			derp_map,
			home_derp: self.config.derp_home_region.unwrap_or(1),
			mtu: self.config.mtu,
		};

		let engine = Arc::new(WgEngine::new(engine_config).await?);
		engine.start().await?;

		let _recv_handle = Arc::clone(&engine).spawn_recv_loop();
		let _send_handle = Arc::clone(&engine).spawn_send_loop();
		let _timer_handle = Arc::clone(&engine).spawn_timer_loop();

		self.engine = Some(Arc::clone(&engine));

		info!("WireGuard engine started");

		self
			.peer_handler
			.connect(&self.config.server_url, &self.config.weaver_id, &svid)
			.await?;

		info!("connected to peer stream");

		let heartbeat_interval = Duration::from_secs(self.config.heartbeat_interval_secs);
		let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
		heartbeat_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

		let mut shutdown_rx = self.shutdown_rx.clone();

		loop {
			tokio::select! {
				biased;

				_ = shutdown_rx.changed() => {
					if *shutdown_rx.borrow() {
						info!("shutdown signal received");
						break;
					}
				}

				event = self.peer_handler.next() => {
					match event {
						Some(Ok(PeerEvent::PeerAdded { public_key, allowed_ip, session_id })) => {
							info!(%session_id, "peer added");
							if let Err(e) = self.handle_peer_added(&engine, &public_key, &allowed_ip).await {
								error!(error = %e, %session_id, "failed to add peer");
							}
						}
						Some(Ok(PeerEvent::PeerRemoved { public_key, session_id })) => {
							info!(%session_id, "peer removed");
							if let Err(e) = self.handle_peer_removed(&engine, &public_key).await {
								error!(error = %e, %session_id, "failed to remove peer");
							}
						}
						Some(Err(e)) => {
							warn!(error = %e, "peer stream error");
							if matches!(e, PeerError::StreamEnded) {
								break;
							}
						}
						None => {
							info!("peer stream ended");
							break;
						}
					}
				}

				_ = heartbeat_timer.tick() => {
					if let Err(e) = self.registration.heartbeat().await {
						warn!(error = %e, "failed to send heartbeat");
					}
				}
			}
		}

		info!("cleaning up weaver WireGuard daemon");

		if let Err(e) = self.registration.unregister().await {
			warn!(error = %e, "failed to unregister from server");
		}

		engine.shutdown().await;
		self.peer_handler.close();

		info!("weaver WireGuard daemon stopped");

		Ok(())
	}

	async fn handle_peer_added(
		&self,
		engine: &Arc<WgEngine>,
		public_key: &str,
		allowed_ip: &str,
	) -> Result<()> {
		let peer_public_key = WgPublicKey::from_base64(public_key)
			.map_err(|e| DaemonError::Config(crate::error::ConfigError::Parse(e.to_string())))?;

		let peer_ip: Ipv6Addr = allowed_ip
			.parse()
			.map_err(|e| DaemonError::Registration(crate::error::RegistrationError::IpParse(e)))?;

		let peer_config = PeerConfig {
			public_key: peer_public_key,
			allowed_ips: vec![peer_ip],
			endpoint: None,
			derp_region: None,
			persistent_keepalive: Some(25),
		};

		engine.add_peer(peer_config).await?;

		debug!(%allowed_ip, "added peer to WireGuard engine");

		Ok(())
	}

	async fn handle_peer_removed(&self, engine: &Arc<WgEngine>, public_key: &str) -> Result<()> {
		let peer_public_key = WgPublicKey::from_base64(public_key)
			.map_err(|e| DaemonError::Config(crate::error::ConfigError::Parse(e.to_string())))?;

		engine.remove_peer(&peer_public_key).await?;

		debug!("removed peer from WireGuard engine");

		Ok(())
	}

	pub fn shutdown(&self) {
		let _ = self.shutdown_tx.send(true);
	}

	pub async fn wait(&self) {
		let mut rx = self.shutdown_rx.clone();
		while !*rx.borrow() {
			if rx.changed().await.is_err() {
				break;
			}
		}
	}

	pub fn public_key(&self) -> &WgPublicKey {
		self.keypair.public_key()
	}

	pub fn assigned_ip(&self) -> Option<Ipv6Addr> {
		self.registration.assigned_ip()
	}

	pub fn is_running(&self) -> bool {
		self
			.engine
			.as_ref()
			.map(|e| e.is_running())
			.unwrap_or(false)
	}
}

impl std::fmt::Debug for WeaverWgDaemon {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WeaverWgDaemon")
			.field("weaver_id", &self.config.weaver_id)
			.field("assigned_ip", &self.registration.assigned_ip())
			.field("is_running", &self.is_running())
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_daemon_new() {
		let config = WeaverWgConfig::new_insecure(
			"https://loom.example.com".parse().unwrap(),
			"weaver-test".to_string(),
		);
		let daemon = WeaverWgDaemon::new(config).await.unwrap();
		assert!(!daemon.is_running());
		assert!(daemon.assigned_ip().is_none());
	}

	#[tokio::test]
	async fn test_daemon_disabled() {
		let mut config = WeaverWgConfig::new_insecure(
			"https://loom.example.com".parse().unwrap(),
			"weaver-test".to_string(),
		);
		config.enabled = false;

		let mut daemon = WeaverWgDaemon::new(config).await.unwrap();
		let result = daemon.run().await;
		assert!(result.is_ok());
	}
}
