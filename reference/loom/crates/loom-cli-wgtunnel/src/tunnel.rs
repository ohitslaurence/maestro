// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::client::CreateSessionResponse;
use crate::error::Result;
use loom_wgtunnel_common::{DerpMap, WgKeyPair, WgPublicKey};
use loom_wgtunnel_engine::{PeerConfig, WgEngine, WgEngineConfig};
use std::collections::HashMap;
use std::net::Ipv6Addr;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use tracing::{info, instrument};

#[derive(Debug, Clone)]
pub struct TunnelConfig {
	pub private_key: WgKeyPair,
	pub address: Ipv6Addr,
	pub derp_map: DerpMap,
	pub home_derp: u16,
}

impl TunnelConfig {
	pub fn new(private_key: WgKeyPair, address: Ipv6Addr, derp_map: DerpMap, home_derp: u16) -> Self {
		Self {
			private_key,
			address,
			derp_map,
			home_derp,
		}
	}
}

#[derive(Debug)]
pub struct TunnelStatus {
	pub running: bool,
	pub our_ip: Option<String>,
	pub connected_weavers: Vec<WeaverConnectionStatus>,
}

#[derive(Debug)]
pub struct WeaverConnectionStatus {
	pub weaver_id: String,
	pub ip: String,
	pub path_type: String,
	pub last_handshake: Option<String>,
}

#[allow(dead_code)]
struct WeaverSession {
	weaver_id: String,
	weaver_ip: Ipv6Addr,
	weaver_public_key: WgPublicKey,
	session_id: String,
}

pub struct TunnelManager {
	engine: Arc<WgEngine>,
	weavers: RwLock<HashMap<String, WeaverSession>>,
	shutdown_tx: watch::Sender<bool>,
	shutdown_rx: watch::Receiver<bool>,
}

impl TunnelManager {
	#[instrument(skip(config), fields(address = %config.address, home_derp = config.home_derp))]
	pub async fn start(config: TunnelConfig) -> Result<Self> {
		let engine_config = WgEngineConfig {
			private_key: config.private_key,
			address: config.address,
			derp_map: config.derp_map,
			home_derp: config.home_derp,
			mtu: 1280,
		};

		let engine = WgEngine::new(engine_config).await?;
		engine.start().await?;

		let engine = Arc::new(engine);

		let recv_handle = Arc::clone(&engine).spawn_recv_loop();
		let send_handle = Arc::clone(&engine).spawn_send_loop();
		let timer_handle = Arc::clone(&engine).spawn_timer_loop();

		tokio::spawn(async move {
			tokio::select! {
				_ = recv_handle => {}
				_ = send_handle => {}
				_ = timer_handle => {}
			}
		});

		let (shutdown_tx, shutdown_rx) = watch::channel(false);

		info!("tunnel manager started");

		Ok(Self {
			engine,
			weavers: RwLock::new(HashMap::new()),
			shutdown_tx,
			shutdown_rx,
		})
	}

	#[instrument(skip(self, session), fields(weaver_id = %weaver_id))]
	pub async fn add_weaver(&self, weaver_id: &str, session: &CreateSessionResponse) -> Result<()> {
		let weaver_ip: Ipv6Addr = session
			.weaver
			.ip
			.parse()
			.map_err(|e| crate::error::CliError::Other(format!("invalid weaver IP: {}", e)))?;

		let weaver_public_key = WgPublicKey::from_base64(&session.weaver.public_key)
			.map_err(|e| crate::error::CliError::Other(format!("invalid public key: {}", e)))?;

		let peer_config = PeerConfig::new(weaver_public_key)
			.with_allowed_ip(weaver_ip)
			.with_derp_region(session.weaver.derp_home_region)
			.with_persistent_keepalive(25);

		self.engine.add_peer(peer_config).await?;

		let weaver_session = WeaverSession {
			weaver_id: weaver_id.to_string(),
			weaver_ip,
			weaver_public_key,
			session_id: session.session_id.clone(),
		};

		let mut weavers = self.weavers.write().await;
		weavers.insert(weaver_id.to_string(), weaver_session);

		info!("added weaver to tunnel");
		Ok(())
	}

	#[instrument(skip(self))]
	pub async fn remove_weaver(&self, weaver_id: &str) -> Result<()> {
		let mut weavers = self.weavers.write().await;
		if let Some(session) = weavers.remove(weaver_id) {
			self.engine.remove_peer(&session.weaver_public_key).await?;
			info!("removed weaver from tunnel");
		}
		Ok(())
	}

	pub async fn status(&self) -> TunnelStatus {
		let running = self.engine.is_running();
		let our_ip = if running {
			Some(self.engine.address().to_string())
		} else {
			None
		};

		let weavers = self.weavers.read().await;
		let mut connected_weavers = Vec::new();

		for (id, session) in weavers.iter() {
			connected_weavers.push(WeaverConnectionStatus {
				weaver_id: id.clone(),
				ip: session.weaver_ip.to_string(),
				path_type: "derp".to_string(),
				last_handshake: None,
			});
		}

		TunnelStatus {
			running,
			our_ip,
			connected_weavers,
		}
	}

	pub async fn get_weaver_ip(&self, weaver_id: &str) -> Option<Ipv6Addr> {
		let weavers = self.weavers.read().await;
		weavers.get(weaver_id).map(|s| s.weaver_ip)
	}

	#[instrument(skip(self))]
	pub async fn shutdown(&self) {
		info!("shutting down tunnel manager");
		let _ = self.shutdown_tx.send(true);
		self.engine.shutdown().await;
	}

	pub async fn wait(&self) {
		let mut rx = self.shutdown_rx.clone();
		while !*rx.borrow() {
			if rx.changed().await.is_err() {
				break;
			}
		}
	}

	pub fn engine(&self) -> &Arc<WgEngine> {
		&self.engine
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_tunnel_config_new() {
		let keypair = WgKeyPair::generate();
		let address: Ipv6Addr = "fd7a:115c:a1e0:2::1".parse().unwrap();
		let derp_map = DerpMap::default();

		let config = TunnelConfig::new(keypair.clone(), address, derp_map, 1);

		assert_eq!(config.address, address);
		assert_eq!(config.home_derp, 1);
	}
}
