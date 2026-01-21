// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::device::{VirtualDevice, VirtualTcpListener, VirtualTcpStream};
use crate::error::{EngineError, Result};
use crate::peers::{PeerConfig, PeerManager};
use crate::router::Router;
use defguard_boringtun::noise::{Tunn, TunnResult};
use loom_wgtunnel_common::{DerpMap, WgKeyPair, WgPublicKey};
use loom_wgtunnel_conn::{MagicConn, PeerEndpoint};
use std::net::{Ipv6Addr, SocketAddrV6};
use std::sync::Arc;
use tokio::sync::{watch, Mutex, RwLock};
use tracing::{debug, info, instrument, trace, warn};

const DEFAULT_MTU: u16 = 1280;

pub struct WgEngineConfig {
	pub private_key: WgKeyPair,
	pub address: Ipv6Addr,
	pub derp_map: DerpMap,
	pub home_derp: u16,
	pub mtu: u16,
}

impl Default for WgEngineConfig {
	fn default() -> Self {
		Self {
			private_key: WgKeyPair::generate(),
			address: "fd7a:115c:a1e0::1".parse().unwrap(),
			derp_map: DerpMap::default(),
			home_derp: 1,
			mtu: DEFAULT_MTU,
		}
	}
}

struct TunnelState {
	tunn: Mutex<Tunn>,
	peer_key: WgPublicKey,
}

pub struct WgEngine {
	config: WgEngineConfig,
	magic_conn: Arc<MagicConn>,
	peers: PeerManager,
	router: RwLock<Router>,
	device: VirtualDevice,
	tunnels: RwLock<Vec<TunnelState>>,
	shutdown_tx: watch::Sender<bool>,
	shutdown_rx: watch::Receiver<bool>,
	running: std::sync::atomic::AtomicBool,
}

impl WgEngine {
	#[instrument(skip(config), fields(address = %config.address, mtu = config.mtu, home_derp = config.home_derp))]
	pub async fn new(config: WgEngineConfig) -> Result<Self> {
		let device = VirtualDevice::new(config.address, config.mtu)?;

		let magic_conn = MagicConn::new(
			config.private_key.clone(),
			config.derp_map.clone(),
			config.home_derp,
		)
		.await?;

		let (shutdown_tx, shutdown_rx) = watch::channel(false);

		info!("created WireGuard engine");

		Ok(Self {
			config,
			magic_conn: Arc::new(magic_conn),
			peers: PeerManager::new(),
			router: RwLock::new(Router::new()),
			device,
			tunnels: RwLock::new(Vec::new()),
			shutdown_tx,
			shutdown_rx,
			running: std::sync::atomic::AtomicBool::new(false),
		})
	}

	#[instrument(skip(self))]
	pub async fn start(&self) -> Result<()> {
		if self.running.swap(true, std::sync::atomic::Ordering::SeqCst) {
			return Err(EngineError::AlreadyRunning);
		}

		info!("starting WireGuard engine");

		Ok(())
	}

	pub fn spawn_recv_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
		let engine = Arc::clone(&self);
		let mut shutdown_rx = self.shutdown_rx.clone();

		tokio::spawn(async move {
			let mut buf = vec![0u8; 65536];
			let mut dst_buf = vec![0u8; 65536];

			loop {
				tokio::select! {
					biased;

					_ = shutdown_rx.changed() => {
						if *shutdown_rx.borrow() {
							info!("recv loop shutting down");
							break;
						}
					}

					result = engine.magic_conn.recv(&mut buf) => {
						match result {
							Ok((peer, len, path)) => {
								trace!(%peer, len, ?path, "received packet from MagicConn");

								let tunnels = engine.tunnels.read().await;
								for state in tunnels.iter() {
									if state.peer_key == peer {
										let mut tunn = state.tunn.lock().await;
										let result = tunn.decapsulate(None, &buf[..len], &mut dst_buf);
										drop(tunn);

										match result {
											TunnResult::Done => {
												trace!("packet processed, no output");
											}
											TunnResult::WriteToNetwork(data) => {
												trace!(len = data.len(), "sending handshake response");
												if let Err(e) = engine.magic_conn.send(&peer, data).await {
													warn!(%peer, error = %e, "failed to send handshake response");
												}
											}
											TunnResult::WriteToTunnelV6(data, _) => {
												trace!(len = data.len(), "decrypted packet for virtual device");
												if let Err(e) = engine.device.receive_packet(data) {
													warn!(error = %e, "failed to receive packet into virtual device");
												}
											}
											TunnResult::WriteToTunnelV4(data, _) => {
												trace!(len = data.len(), "received IPv4 packet (unexpected)");
												if let Err(e) = engine.device.receive_packet(data) {
													warn!(error = %e, "failed to receive IPv4 packet into virtual device");
												}
											}
											TunnResult::Err(e) => {
												debug!(%peer, ?e, "tunnel decapsulate error");
											}
										}
										break;
									}
								}
							}
							Err(e) => {
								warn!(error = %e, "MagicConn recv error");
							}
						}
					}
				}
			}
		})
	}

	pub fn spawn_send_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
		let engine = Arc::clone(&self);
		let mut shutdown_rx = self.shutdown_rx.clone();

		tokio::spawn(async move {
			let mut dst_buf = vec![0u8; 65536];

			loop {
				tokio::select! {
					biased;

					_ = shutdown_rx.changed() => {
						if *shutdown_rx.borrow() {
							info!("send loop shutting down");
							break;
						}
					}

					_ = tokio::time::sleep(std::time::Duration::from_millis(1)) => {
						while let Some(packet) = engine.device.transmit_packet() {
							if packet.len() < 40 {
								continue;
							}

							let dst_ip = extract_ipv6_dst(&packet);
							if let Some(dst) = dst_ip {
								let router = engine.router.read().await;
								if let Some(peer_key) = router.route(dst).cloned() {
									drop(router);

									let tunnels = engine.tunnels.read().await;
									for state in tunnels.iter() {
										if state.peer_key == peer_key {
											let mut tunn = state.tunn.lock().await;
											let result = tunn.encapsulate(&packet, &mut dst_buf);
											drop(tunn);

											match result {
												TunnResult::WriteToNetwork(data) => {
													trace!(len = data.len(), %peer_key, "sending encrypted packet");
													if let Err(e) = engine.magic_conn.send(&peer_key, data).await {
														warn!(%peer_key, error = %e, "failed to send encrypted packet");
													}
												}
												TunnResult::Done => {
													trace!("encapsulate done, no output");
												}
												TunnResult::Err(e) => {
													debug!(%peer_key, ?e, "tunnel encapsulate error");
												}
												_ => {}
											}
											break;
										}
									}
								}
							}
						}
					}
				}
			}
		})
	}

	pub fn spawn_timer_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
		let engine = Arc::clone(&self);
		let mut shutdown_rx = self.shutdown_rx.clone();

		tokio::spawn(async move {
			let mut dst_buf = vec![0u8; 65536];

			loop {
				tokio::select! {
					biased;

					_ = shutdown_rx.changed() => {
						if *shutdown_rx.borrow() {
							info!("timer loop shutting down");
							break;
						}
					}

					_ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
						let tunnels = engine.tunnels.read().await;
						for state in tunnels.iter() {
							let mut tunn = state.tunn.lock().await;
							let result = tunn.update_timers(&mut dst_buf);
							let peer_key = state.peer_key;
							drop(tunn);

							match result {
								TunnResult::WriteToNetwork(data) => {
									trace!(len = data.len(), peer = %peer_key, "sending keepalive/handshake");
									if let Err(e) = engine.magic_conn.send(&peer_key, data).await {
										warn!(peer = %peer_key, error = %e, "failed to send timer packet");
									}
								}
								TunnResult::Done => {}
								TunnResult::Err(e) => {
									debug!(peer = %peer_key, ?e, "timer update error");
								}
								_ => {}
							}
						}
					}
				}
			}
		})
	}

	#[instrument(skip(self, peer), fields(peer = %peer.public_key))]
	pub async fn add_peer(&self, peer: PeerConfig) -> Result<()> {
		let peer_key = peer.public_key;

		let tunn = Tunn::new(
			defguard_boringtun::x25519::StaticSecret::from(
				*self.config.private_key.private_key().expose_bytes(),
			),
			defguard_boringtun::x25519::PublicKey::from(*peer_key.as_bytes()),
			None,
			peer.persistent_keepalive,
			0,
			None,
		);

		{
			let mut tunnels = self.tunnels.write().await;
			tunnels.push(TunnelState {
				tunn: Mutex::new(tunn),
				peer_key,
			});
		}

		{
			let mut router = self.router.write().await;
			for ip in &peer.allowed_ips {
				router.add_route(*ip, peer_key);
			}
		}

		let endpoint = if let Some(direct) = peer.endpoint {
			PeerEndpoint::with_direct(direct)
		} else if let Some(region) = peer.derp_region {
			PeerEndpoint::with_derp(region)
		} else {
			PeerEndpoint::new()
		};

		self.magic_conn.add_peer(peer_key, endpoint).await;

		self.peers.add(peer).await?;

		info!("added peer to WireGuard engine");
		Ok(())
	}

	#[instrument(skip(self), fields(peer = %public_key))]
	pub async fn remove_peer(&self, public_key: &WgPublicKey) -> Result<()> {
		{
			let mut tunnels = self.tunnels.write().await;
			tunnels.retain(|s| s.peer_key != *public_key);
		}

		{
			let mut router = self.router.write().await;
			router.remove_peer(public_key);
		}

		self.magic_conn.remove_peer(public_key).await;

		self
			.peers
			.remove(public_key)
			.await
			.ok_or_else(|| EngineError::PeerNotFound(public_key.to_string()))?;

		info!("removed peer from WireGuard engine");
		Ok(())
	}

	pub fn public_key(&self) -> WgPublicKey {
		*self.config.private_key.public_key()
	}

	pub fn address(&self) -> Ipv6Addr {
		self.config.address
	}

	#[instrument(skip(self), fields(port))]
	pub async fn tcp_listener(&self, port: u16) -> Result<VirtualTcpListener> {
		let (handle, local_addr) = self.device.listen(port)?;
		Ok(VirtualTcpListener::new(
			self.device.clone(),
			handle,
			local_addr,
		))
	}

	#[instrument(skip(self), fields(%addr))]
	pub async fn tcp_connect(&self, addr: SocketAddrV6) -> Result<VirtualTcpStream> {
		let handle = self.device.connect(addr)?;
		let stream = VirtualTcpStream::new(self.device.clone(), handle);
		stream.wait_connected().await?;
		Ok(stream)
	}

	#[instrument(skip(self))]
	pub async fn shutdown(&self) {
		info!("shutting down WireGuard engine");
		let _ = self.shutdown_tx.send(true);
		self
			.running
			.store(false, std::sync::atomic::Ordering::SeqCst);

		self.magic_conn.close().await;

		{
			let mut tunnels = self.tunnels.write().await;
			tunnels.clear();
		}
	}

	pub async fn wait(&self) {
		let mut rx = self.shutdown_rx.clone();
		while !*rx.borrow() {
			if rx.changed().await.is_err() {
				break;
			}
		}
	}

	pub fn is_running(&self) -> bool {
		self.running.load(std::sync::atomic::Ordering::SeqCst)
	}

	pub async fn peer_count(&self) -> usize {
		self.peers.peer_count().await
	}

	pub fn magic_conn(&self) -> &Arc<MagicConn> {
		&self.magic_conn
	}

	pub fn device(&self) -> &VirtualDevice {
		&self.device
	}
}

fn extract_ipv6_dst(packet: &[u8]) -> Option<Ipv6Addr> {
	if packet.len() < 40 {
		return None;
	}

	let version = packet[0] >> 4;
	if version != 6 {
		return None;
	}

	let mut dst_bytes = [0u8; 16];
	dst_bytes.copy_from_slice(&packet[24..40]);
	Some(Ipv6Addr::from(dst_bytes))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_extract_ipv6_dst() {
		let mut packet = vec![0u8; 40];
		packet[0] = 0x60;

		let dst: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();
		packet[24..40].copy_from_slice(&dst.octets());

		let extracted = extract_ipv6_dst(&packet).unwrap();
		assert_eq!(extracted, dst);
	}

	#[test]
	fn test_extract_ipv6_dst_too_short() {
		let packet = vec![0u8; 20];
		assert!(extract_ipv6_dst(&packet).is_none());
	}

	#[test]
	fn test_extract_ipv6_dst_wrong_version() {
		let mut packet = vec![0u8; 40];
		packet[0] = 0x45;
		assert!(extract_ipv6_dst(&packet).is_none());
	}

	#[test]
	fn test_wg_engine_config_default() {
		let config = WgEngineConfig::default();
		assert_eq!(config.mtu, 1280);
		assert_eq!(config.home_derp, 1);
		assert_eq!(
			config.address,
			"fd7a:115c:a1e0::1".parse::<Ipv6Addr>().unwrap()
		);
	}
}
