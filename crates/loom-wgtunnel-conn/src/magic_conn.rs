// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{ConnError, Result};
use crate::upgrade::{should_fallback_to_derp, upgrade_interval_with_jitter, DIRECT_STALE_TIMEOUT};
use loom_wgtunnel_common::{DerpMap, WgKeyPair, WgPublicKey};
use loom_wgtunnel_derp::{DerpClient, DerpFrame};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, instrument, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathType {
	Direct,
	Derp(u16),
}

#[derive(Debug)]
pub struct PeerEndpoint {
	pub direct: Option<SocketAddr>,
	pub derp_region: Option<u16>,
	pub last_direct: Option<Instant>,
	pub using_derp: bool,
}

impl PeerEndpoint {
	pub fn new() -> Self {
		Self {
			direct: None,
			derp_region: None,
			last_direct: None,
			using_derp: true,
		}
	}

	pub fn with_direct(addr: SocketAddr) -> Self {
		Self {
			direct: Some(addr),
			derp_region: None,
			last_direct: Some(Instant::now()),
			using_derp: false,
		}
	}

	pub fn with_derp(region: u16) -> Self {
		Self {
			direct: None,
			derp_region: Some(region),
			last_direct: None,
			using_derp: true,
		}
	}
}

impl Default for PeerEndpoint {
	fn default() -> Self {
		Self::new()
	}
}

pub struct MagicConn {
	our_key: WgKeyPair,
	udp: UdpSocket,
	derp_clients: Mutex<HashMap<u16, DerpClient>>,
	home_derp: u16,
	peer_endpoints: RwLock<HashMap<WgPublicKey, PeerEndpoint>>,
	derp_map: DerpMap,
}

impl MagicConn {
	#[instrument(skip(our_key, derp_map), fields(home_derp))]
	pub async fn new(our_key: WgKeyPair, derp_map: DerpMap, home_derp: u16) -> Result<Self> {
		let udp = UdpSocket::bind("0.0.0.0:0").await?;
		debug!(local_addr = ?udp.local_addr(), "bound UDP socket");

		Ok(Self {
			our_key,
			udp,
			derp_clients: Mutex::new(HashMap::new()),
			home_derp,
			peer_endpoints: RwLock::new(HashMap::new()),
			derp_map,
		})
	}

	#[instrument(skip(self, data), fields(peer = %peer, len = data.len()))]
	pub async fn send(&self, peer: &WgPublicKey, data: &[u8]) -> Result<PathType> {
		let endpoints = self.peer_endpoints.read().await;
		let endpoint = endpoints.get(peer);

		if let Some(ep) = endpoint {
			if let Some(direct) = ep.direct {
				let should_try_direct = !ep.using_derp
					|| ep
						.last_direct
						.is_some_and(|t| t.elapsed() < DIRECT_STALE_TIMEOUT);

				if should_try_direct {
					match self.udp.send_to(data, direct).await {
						Ok(_) => {
							debug!(?direct, "sent via direct UDP");
							return Ok(PathType::Direct);
						}
						Err(e) => {
							warn!(error = %e, ?direct, "direct send failed, falling back to DERP");
						}
					}
				}
			}
		}

		drop(endpoints);

		let region = {
			let endpoints = self.peer_endpoints.read().await;
			endpoints
				.get(peer)
				.and_then(|ep| ep.derp_region)
				.unwrap_or(self.home_derp)
		};

		self.ensure_derp_connection(region).await?;

		let mut clients = self.derp_clients.lock().await;
		if let Some(client) = clients.get_mut(&region) {
			client.send(peer, data).await?;
			debug!(region, "sent via DERP");
			return Ok(PathType::Derp(region));
		}

		Err(ConnError::NoPeerPath(peer.to_string()))
	}

	#[instrument(skip(self, buf), fields(buf_len = buf.len()))]
	pub async fn recv(&self, buf: &mut [u8]) -> Result<(WgPublicKey, usize, PathType)> {
		loop {
			tokio::select! {
				biased;

				result = self.udp.recv_from(buf) => {
					let (len, from) = result?;

					if len >= 32 {
						let mut key_bytes = [0u8; 32];
						key_bytes.copy_from_slice(&buf[..32]);
						let peer = WgPublicKey::from_bytes(key_bytes);

						{
							let mut endpoints = self.peer_endpoints.write().await;
							if let Some(ep) = endpoints.get_mut(&peer) {
								ep.direct = Some(from);
								ep.last_direct = Some(Instant::now());
								if ep.using_derp {
									info!(%peer, ?from, "discovered direct path from incoming packet");
									ep.using_derp = false;
								}
							}
						}

						debug!(?from, len, "received via direct UDP");
						return Ok((peer, len, PathType::Direct));
					}
				}

				result = self.recv_from_any_derp() => {
					let (peer, data, region) = result?;
					let len = data.len().min(buf.len());
					buf[..len].copy_from_slice(&data[..len]);
					debug!(region, len, "received via DERP");
					return Ok((peer, len, PathType::Derp(region)));
				}
			}
		}
	}

	async fn recv_from_any_derp(&self) -> Result<(WgPublicKey, Vec<u8>, u16)> {
		loop {
			let regions: Vec<u16> = {
				let clients = self.derp_clients.lock().await;
				clients.keys().copied().collect()
			};

			if regions.is_empty() {
				self.ensure_derp_connection(self.home_derp).await?;
				continue;
			}

			for region in regions {
				let mut clients = self.derp_clients.lock().await;
				if let Some(client) = clients.get_mut(&region) {
					match client.recv().await {
						Ok(frame) => match frame {
							DerpFrame::RecvPacket { src_key, data } => {
								let peer = WgPublicKey::from_bytes(src_key);
								return Ok((peer, data, region));
							}
							DerpFrame::PeerGone { peer_key } => {
								let peer = WgPublicKey::from_bytes(peer_key);
								info!(%peer, "peer gone notification from DERP");
								continue;
							}
							DerpFrame::PeerPresent { peer_key } => {
								let peer = WgPublicKey::from_bytes(peer_key);
								debug!(%peer, region, "peer present on DERP");
								continue;
							}
							DerpFrame::KeepAlive => {
								continue;
							}
							_ => {
								continue;
							}
						},
						Err(e) => {
							warn!(region, error = %e, "DERP recv failed");
							continue;
						}
					}
				}
			}

			tokio::time::sleep(std::time::Duration::from_millis(10)).await;
		}
	}

	pub async fn add_peer(&self, peer: WgPublicKey, endpoint: PeerEndpoint) {
		let mut endpoints = self.peer_endpoints.write().await;
		endpoints.insert(peer, endpoint);
		debug!(%peer, "added peer");
	}

	pub async fn remove_peer(&self, peer: &WgPublicKey) {
		let mut endpoints = self.peer_endpoints.write().await;
		endpoints.remove(peer);
		debug!(%peer, "removed peer");
	}

	pub async fn path_type(&self, peer: &WgPublicKey) -> Option<PathType> {
		let endpoints = self.peer_endpoints.read().await;
		endpoints.get(peer).map(|ep| {
			if ep.using_derp {
				PathType::Derp(ep.derp_region.unwrap_or(self.home_derp))
			} else {
				PathType::Direct
			}
		})
	}

	#[instrument(skip(self), fields(region))]
	async fn ensure_derp_connection(&self, region: u16) -> Result<()> {
		{
			let clients = self.derp_clients.lock().await;
			if clients.contains_key(&region) {
				return Ok(());
			}
		}

		let region_info = self
			.derp_map
			.regions
			.get(&region)
			.ok_or(ConnError::UnknownDerpRegion(region))?;

		let node = region_info
			.nodes
			.first()
			.ok_or(ConnError::UnknownDerpRegion(region))?;

		info!(
			region,
			host = %node.host_name,
			"connecting to DERP server"
		);

		let client = DerpClient::connect(node, &self.our_key).await?;

		let mut clients = self.derp_clients.lock().await;
		clients.insert(region, client);

		Ok(())
	}

	pub fn spawn_upgrade_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
		tokio::spawn(async move {
			loop {
				let interval = upgrade_interval_with_jitter();
				tokio::time::sleep(interval).await;

				let peers: Vec<WgPublicKey> = {
					let endpoints = self.peer_endpoints.read().await;
					endpoints.keys().cloned().collect()
				};

				for peer in peers {
					let should_probe = {
						let endpoints = self.peer_endpoints.read().await;
						endpoints
							.get(&peer)
							.map(|ep| ep.using_derp && ep.direct.is_some())
							.unwrap_or(false)
					};

					if should_probe {
						let direct = {
							let endpoints = self.peer_endpoints.read().await;
							endpoints.get(&peer).and_then(|ep| ep.direct)
						};

						if let Some(addr) = direct {
							match crate::upgrade::probe_direct(&self.udp, addr, &self.our_key).await {
								Ok(true) => {
									let mut endpoints = self.peer_endpoints.write().await;
									if let Some(ep) = endpoints.get_mut(&peer) {
										ep.using_derp = false;
										ep.last_direct = Some(Instant::now());
										info!(%peer, ?addr, "upgraded to direct connection");
									}
								}
								Ok(false) => {
									debug!(%peer, "probe failed, staying on DERP");
								}
								Err(e) => {
									warn!(%peer, error = %e, "probe error");
								}
							}
						}
					}

					let should_fallback = {
						let endpoints = self.peer_endpoints.read().await;
						endpoints
							.get(&peer)
							.map(|ep| !ep.using_derp && should_fallback_to_derp(ep.last_direct))
							.unwrap_or(false)
					};

					if should_fallback {
						let mut endpoints = self.peer_endpoints.write().await;
						if let Some(ep) = endpoints.get_mut(&peer) {
							ep.using_derp = true;
							info!(%peer, "falling back to DERP due to stale direct connection");
						}
					}
				}
			}
		})
	}

	pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
		self.udp.local_addr()
	}

	pub async fn close(&self) {
		let mut clients = self.derp_clients.lock().await;
		clients.clear();
		debug!("closed all DERP connections");
	}

	pub fn public_key(&self) -> &WgPublicKey {
		self.our_key.public_key()
	}

	pub fn home_derp_region(&self) -> u16 {
		self.home_derp
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_peer_endpoint_new() {
		let ep = PeerEndpoint::new();
		assert!(ep.direct.is_none());
		assert!(ep.derp_region.is_none());
		assert!(ep.last_direct.is_none());
		assert!(ep.using_derp);
	}

	#[test]
	fn test_peer_endpoint_with_direct() {
		let addr: SocketAddr = "192.168.1.1:51820".parse().unwrap();
		let ep = PeerEndpoint::with_direct(addr);
		assert_eq!(ep.direct, Some(addr));
		assert!(!ep.using_derp);
		assert!(ep.last_direct.is_some());
	}

	#[test]
	fn test_peer_endpoint_with_derp() {
		let ep = PeerEndpoint::with_derp(1);
		assert!(ep.direct.is_none());
		assert_eq!(ep.derp_region, Some(1));
		assert!(ep.using_derp);
	}

	#[test]
	fn test_path_type_eq() {
		assert_eq!(PathType::Direct, PathType::Direct);
		assert_eq!(PathType::Derp(1), PathType::Derp(1));
		assert_ne!(PathType::Direct, PathType::Derp(1));
		assert_ne!(PathType::Derp(1), PathType::Derp(2));
	}
}
