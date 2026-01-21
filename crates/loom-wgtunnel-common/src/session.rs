// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::Ipv6Addr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
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

impl Default for SessionId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for SessionId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for SessionId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.parse()?))
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(Uuid);

impl DeviceId {
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

impl Default for DeviceId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for DeviceId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for DeviceId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.parse()?))
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaverId(Uuid);

impl WeaverId {
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

impl Default for WeaverId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for WeaverId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for WeaverId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(s.parse()?))
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
	pub id: SessionId,
	pub device_id: DeviceId,
	pub weaver_id: WeaverId,
	pub client_ip: Ipv6Addr,
	pub created_at: DateTime<Utc>,
	#[serde(default)]
	pub last_handshake: Option<DateTime<Utc>>,
}

impl SessionInfo {
	pub fn new(device_id: DeviceId, weaver_id: WeaverId, client_ip: Ipv6Addr) -> Self {
		Self {
			id: SessionId::new(),
			device_id,
			weaver_id,
			client_ip,
			created_at: Utc::now(),
			last_handshake: None,
		}
	}

	pub fn update_handshake(&mut self) {
		self.last_handshake = Some(Utc::now());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn session_id_roundtrip() {
		let id = SessionId::new();
		let s = id.to_string();
		let parsed: SessionId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn device_id_roundtrip() {
		let id = DeviceId::new();
		let s = id.to_string();
		let parsed: DeviceId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn weaver_id_roundtrip() {
		let id = WeaverId::new();
		let s = id.to_string();
		let parsed: WeaverId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn session_info_serialization() {
		let session = SessionInfo::new(
			DeviceId::new(),
			WeaverId::new(),
			"fd7a:115c:a1e0:2::1".parse().unwrap(),
		);

		let json = serde_json::to_string(&session).unwrap();
		let deserialized: SessionInfo = serde_json::from_str(&json).unwrap();

		assert_eq!(session.id, deserialized.id);
		assert_eq!(session.device_id, deserialized.device_id);
		assert_eq!(session.weaver_id, deserialized.weaver_id);
		assert_eq!(session.client_ip, deserialized.client_ip);
	}

	#[test]
	fn update_handshake() {
		let mut session = SessionInfo::new(
			DeviceId::new(),
			WeaverId::new(),
			"fd7a:115c:a1e0:2::1".parse().unwrap(),
		);

		assert!(session.last_handshake.is_none());

		session.update_handshake();

		assert!(session.last_handshake.is_some());
	}
}
