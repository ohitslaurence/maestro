// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{ConnError, Result};
use loom_wgtunnel_common::WgKeyPair;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, instrument};

pub const UPGRADE_INTERVAL: Duration = Duration::from_secs(30);

pub const DIRECT_STALE_TIMEOUT: Duration = Duration::from_secs(60);

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

const PROBE_MAGIC: &[u8] = b"LOOM_PROBE";

#[instrument(skip(socket, our_key), fields(peer = %peer_endpoint))]
pub async fn probe_direct(
	socket: &UdpSocket,
	peer_endpoint: SocketAddr,
	our_key: &WgKeyPair,
) -> Result<bool> {
	let mut probe_data = Vec::with_capacity(PROBE_MAGIC.len() + 32);
	probe_data.extend_from_slice(PROBE_MAGIC);
	probe_data.extend_from_slice(our_key.public_key().as_bytes());

	socket.send_to(&probe_data, peer_endpoint).await?;

	debug!("sent probe packet, waiting for response");

	let mut buf = [0u8; 128];
	match timeout(PROBE_TIMEOUT, socket.recv_from(&mut buf)).await {
		Ok(Ok((len, from))) => {
			if from == peer_endpoint
				&& len >= PROBE_MAGIC.len()
				&& &buf[..PROBE_MAGIC.len()] == PROBE_MAGIC
			{
				debug!("received valid probe response");
				Ok(true)
			} else {
				debug!(?from, len, "received unexpected response");
				Ok(false)
			}
		}
		Ok(Err(e)) => {
			debug!(error = %e, "probe recv failed");
			Err(ConnError::Io(e))
		}
		Err(_) => {
			debug!("probe timed out");
			Ok(false)
		}
	}
}

pub fn should_upgrade(
	using_derp: bool,
	last_direct: Option<std::time::Instant>,
	direct_endpoint: Option<SocketAddr>,
) -> bool {
	if !using_derp {
		return false;
	}

	if direct_endpoint.is_none() {
		return true;
	}

	if let Some(last) = last_direct {
		if last.elapsed() > DIRECT_STALE_TIMEOUT {
			return true;
		}
	}

	true
}

pub fn should_fallback_to_derp(last_direct: Option<std::time::Instant>) -> bool {
	match last_direct {
		Some(last) => last.elapsed() > DIRECT_STALE_TIMEOUT,
		None => true,
	}
}

pub fn upgrade_interval_with_jitter() -> Duration {
	let jitter_ms = fastrand::u64(0..5000);
	UPGRADE_INTERVAL + Duration::from_millis(jitter_ms)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::net::{Ipv4Addr, SocketAddrV4};
	use std::time::Instant;

	fn make_addr() -> SocketAddr {
		SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 51820))
	}

	#[test]
	fn test_should_upgrade_when_using_derp() {
		assert!(should_upgrade(true, None, None));
		assert!(should_upgrade(true, None, Some(make_addr())));
		assert!(should_upgrade(
			true,
			Some(Instant::now()),
			Some(make_addr())
		));
	}

	#[test]
	fn test_should_not_upgrade_when_direct() {
		assert!(!should_upgrade(
			false,
			Some(Instant::now()),
			Some(make_addr())
		));
	}

	#[test]
	fn test_should_fallback_when_stale() {
		let old_time = Instant::now() - Duration::from_secs(120);
		assert!(should_fallback_to_derp(Some(old_time)));
	}

	#[test]
	fn test_should_not_fallback_when_fresh() {
		let recent = Instant::now() - Duration::from_secs(10);
		assert!(!should_fallback_to_derp(Some(recent)));
	}

	#[test]
	fn test_should_fallback_when_never_connected() {
		assert!(should_fallback_to_derp(None));
	}

	#[test]
	fn test_upgrade_interval_with_jitter() {
		let interval = upgrade_interval_with_jitter();
		assert!(interval >= UPGRADE_INTERVAL);
		assert!(interval <= UPGRADE_INTERVAL + Duration::from_secs(5));
	}
}
