// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{EngineError, Result};
use loom_wgtunnel_common::WgPublicKey;
use std::collections::HashMap;
use std::net::{Ipv6Addr, SocketAddr};
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

#[derive(Debug, Clone)]
pub struct PeerConfig {
	pub public_key: WgPublicKey,
	pub allowed_ips: Vec<Ipv6Addr>,
	pub endpoint: Option<SocketAddr>,
	pub derp_region: Option<u16>,
	pub persistent_keepalive: Option<u16>,
}

impl PeerConfig {
	pub fn new(public_key: WgPublicKey) -> Self {
		Self {
			public_key,
			allowed_ips: Vec::new(),
			endpoint: None,
			derp_region: None,
			persistent_keepalive: None,
		}
	}

	pub fn with_allowed_ip(mut self, ip: Ipv6Addr) -> Self {
		self.allowed_ips.push(ip);
		self
	}

	pub fn with_allowed_ips(mut self, ips: Vec<Ipv6Addr>) -> Self {
		self.allowed_ips = ips;
		self
	}

	pub fn with_endpoint(mut self, endpoint: SocketAddr) -> Self {
		self.endpoint = Some(endpoint);
		self
	}

	pub fn with_derp_region(mut self, region: u16) -> Self {
		self.derp_region = Some(region);
		self
	}

	pub fn with_persistent_keepalive(mut self, seconds: u16) -> Self {
		self.persistent_keepalive = Some(seconds);
		self
	}
}

#[derive(Debug, Clone)]
pub struct PeerState {
	pub config: PeerConfig,
	pub last_handshake: Option<Instant>,
	pub rx_bytes: u64,
	pub tx_bytes: u64,
}

impl PeerState {
	pub fn new(config: PeerConfig) -> Self {
		Self {
			config,
			last_handshake: None,
			rx_bytes: 0,
			tx_bytes: 0,
		}
	}
}

pub struct PeerManager {
	peers: RwLock<HashMap<WgPublicKey, PeerState>>,
}

impl PeerManager {
	pub fn new() -> Self {
		Self {
			peers: RwLock::new(HashMap::new()),
		}
	}

	#[instrument(skip(self), fields(peer = %config.public_key))]
	pub async fn add(&self, config: PeerConfig) -> Result<()> {
		let public_key = config.public_key;
		let state = PeerState::new(config);

		let mut peers = self.peers.write().await;
		if peers.contains_key(&public_key) {
			return Err(EngineError::WireGuard(format!(
				"peer {} already exists",
				public_key
			)));
		}

		peers.insert(public_key, state);
		debug!("added peer");
		Ok(())
	}

	#[instrument(skip(self), fields(peer = %key))]
	pub async fn remove(&self, key: &WgPublicKey) -> Option<PeerState> {
		let mut peers = self.peers.write().await;
		let removed = peers.remove(key);
		if removed.is_some() {
			debug!("removed peer");
		}
		removed
	}

	pub async fn get(&self, key: &WgPublicKey) -> Option<PeerState> {
		let peers = self.peers.read().await;
		peers.get(key).cloned()
	}

	pub async fn list(&self) -> Vec<PeerState> {
		let peers = self.peers.read().await;
		peers.values().cloned().collect()
	}

	#[instrument(skip(self), fields(peer = %key))]
	pub async fn update_handshake(&self, key: &WgPublicKey) {
		let mut peers = self.peers.write().await;
		if let Some(state) = peers.get_mut(key) {
			state.last_handshake = Some(Instant::now());
			debug!("updated handshake time");
		}
	}

	#[instrument(skip(self), fields(peer = %key, rx, tx))]
	pub async fn update_traffic(&self, key: &WgPublicKey, rx: u64, tx: u64) {
		let mut peers = self.peers.write().await;
		if let Some(state) = peers.get_mut(key) {
			state.rx_bytes = rx;
			state.tx_bytes = tx;
			debug!("updated traffic counters");
		}
	}

	pub async fn peer_count(&self) -> usize {
		let peers = self.peers.read().await;
		peers.len()
	}

	pub async fn has_peer(&self, key: &WgPublicKey) -> bool {
		let peers = self.peers.read().await;
		peers.contains_key(key)
	}
}

impl Default for PeerManager {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_wgtunnel_common::WgKeyPair;

	#[tokio::test]
	async fn test_add_and_get_peer() {
		let manager = PeerManager::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let config =
			PeerConfig::new(public_key.clone()).with_allowed_ip("fd7a:115c:a1e0::2".parse().unwrap());

		manager.add(config).await.unwrap();

		let state = manager.get(&public_key).await.unwrap();
		assert_eq!(state.config.allowed_ips.len(), 1);
		assert!(state.last_handshake.is_none());
	}

	#[tokio::test]
	async fn test_remove_peer() {
		let manager = PeerManager::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let config = PeerConfig::new(public_key.clone());
		manager.add(config).await.unwrap();

		assert!(manager.has_peer(&public_key).await);

		let removed = manager.remove(&public_key).await;
		assert!(removed.is_some());
		assert!(!manager.has_peer(&public_key).await);
	}

	#[tokio::test]
	async fn test_update_handshake() {
		let manager = PeerManager::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let config = PeerConfig::new(public_key.clone());
		manager.add(config).await.unwrap();

		let state = manager.get(&public_key).await.unwrap();
		assert!(state.last_handshake.is_none());

		manager.update_handshake(&public_key).await;

		let state = manager.get(&public_key).await.unwrap();
		assert!(state.last_handshake.is_some());
	}

	#[tokio::test]
	async fn test_update_traffic() {
		let manager = PeerManager::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let config = PeerConfig::new(public_key.clone());
		manager.add(config).await.unwrap();

		manager.update_traffic(&public_key, 1000, 500).await;

		let state = manager.get(&public_key).await.unwrap();
		assert_eq!(state.rx_bytes, 1000);
		assert_eq!(state.tx_bytes, 500);
	}

	#[tokio::test]
	async fn test_list_peers() {
		let manager = PeerManager::new();

		for _ in 0..3 {
			let keypair = WgKeyPair::generate();
			let config = PeerConfig::new(keypair.public_key().clone());
			manager.add(config).await.unwrap();
		}

		let peers = manager.list().await;
		assert_eq!(peers.len(), 3);
	}

	#[tokio::test]
	async fn test_duplicate_peer_error() {
		let manager = PeerManager::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let config1 = PeerConfig::new(public_key.clone());
		let config2 = PeerConfig::new(public_key.clone());

		manager.add(config1).await.unwrap();
		let result = manager.add(config2).await;

		assert!(result.is_err());
	}
}
