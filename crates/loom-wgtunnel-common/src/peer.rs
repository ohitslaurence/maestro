// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::keys::WgPublicKey;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{Ipv6Addr, SocketAddr};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerId(Uuid);

impl PeerId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}

	pub fn from_uuid(uuid: Uuid) -> Self {
		Self(uuid)
	}

	pub fn as_uuid(&self) -> &Uuid {
		&self.0
	}
}

impl Default for PeerId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for PeerId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for PeerId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.parse()?))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
	pub id: PeerId,
	pub public_key: WgPublicKey,
	pub assigned_ip: Ipv6Addr,
	#[serde(default)]
	pub derp_region: Option<u16>,
	#[serde(default)]
	pub endpoint: Option<SocketAddr>,
	#[serde(default)]
	pub last_seen: Option<DateTime<Utc>>,
}

impl PeerInfo {
	pub fn new(public_key: WgPublicKey, assigned_ip: Ipv6Addr) -> Self {
		Self {
			id: PeerId::new(),
			public_key,
			assigned_ip,
			derp_region: None,
			endpoint: None,
			last_seen: None,
		}
	}

	pub fn with_id(id: PeerId, public_key: WgPublicKey, assigned_ip: Ipv6Addr) -> Self {
		Self {
			id,
			public_key,
			assigned_ip,
			derp_region: None,
			endpoint: None,
			last_seen: None,
		}
	}

	pub fn update_endpoint(&mut self, endpoint: SocketAddr) {
		self.endpoint = Some(endpoint);
		self.last_seen = Some(Utc::now());
	}

	pub fn update_derp_region(&mut self, region: u16) {
		self.derp_region = Some(region);
		self.last_seen = Some(Utc::now());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::keys::WgKeyPair;

	#[test]
	fn peer_id_roundtrip() {
		let id = PeerId::new();
		let s = id.to_string();
		let parsed: PeerId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn peer_info_serialization() {
		let keypair = WgKeyPair::generate();
		let peer = PeerInfo::new(
			*keypair.public_key(),
			"fd7a:115c:a1e0:1::1".parse().unwrap(),
		);

		let json = serde_json::to_string(&peer).unwrap();
		let deserialized: PeerInfo = serde_json::from_str(&json).unwrap();

		assert_eq!(peer.id, deserialized.id);
		assert_eq!(peer.public_key, deserialized.public_key);
		assert_eq!(peer.assigned_ip, deserialized.assigned_ip);
	}

	#[test]
	fn update_endpoint() {
		let keypair = WgKeyPair::generate();
		let mut peer = PeerInfo::new(
			*keypair.public_key(),
			"fd7a:115c:a1e0:1::1".parse().unwrap(),
		);

		assert!(peer.endpoint.is_none());
		assert!(peer.last_seen.is_none());

		peer.update_endpoint("1.2.3.4:51820".parse().unwrap());

		assert!(peer.endpoint.is_some());
		assert!(peer.last_seen.is_some());
	}

	#[test]
	fn update_derp_region() {
		let keypair = WgKeyPair::generate();
		let mut peer = PeerInfo::new(
			*keypair.public_key(),
			"fd7a:115c:a1e0:1::1".parse().unwrap(),
		);

		assert!(peer.derp_region.is_none());

		peer.update_derp_region(1);

		assert_eq!(peer.derp_region, Some(1));
		assert!(peer.last_seen.is_some());
	}
}
