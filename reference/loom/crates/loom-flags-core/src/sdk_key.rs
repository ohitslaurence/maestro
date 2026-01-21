// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{EnvironmentId, UserId};

/// Unique identifier for an SDK key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SdkKeyId(pub Uuid);

impl SdkKeyId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for SdkKeyId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for SdkKeyId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for SdkKeyId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Authentication key for SDK clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkKey {
	pub id: SdkKeyId,
	pub environment_id: EnvironmentId,
	pub key_type: SdkKeyType,
	pub name: String,
	/// Argon2 hash
	pub key_hash: String,
	pub created_by: UserId,
	pub created_at: DateTime<Utc>,
	pub last_used_at: Option<DateTime<Utc>>,
	pub revoked_at: Option<DateTime<Utc>>,
}

impl SdkKey {
	/// Prefix for client-side SDK keys.
	pub const CLIENT_PREFIX: &'static str = "loom_sdk_client_";

	/// Prefix for server-side SDK keys.
	pub const SERVER_PREFIX: &'static str = "loom_sdk_server_";

	/// Checks if this key is revoked.
	pub fn is_revoked(&self) -> bool {
		self.revoked_at.is_some()
	}

	/// Revokes this key.
	pub fn revoke(&mut self) {
		self.revoked_at = Some(Utc::now());
	}

	/// Updates the last used timestamp.
	pub fn touch(&mut self) {
		self.last_used_at = Some(Utc::now());
	}

	/// Parses an SDK key string to extract its type and environment.
	///
	/// Returns (SdkKeyType, environment_name, random_part) or None if invalid.
	///
	/// The random part is always a 32-char hex string (UUID without dashes).
	/// Format: `loom_sdk_{type}_{env}_{random32hex}`
	pub fn parse_key(key: &str) -> Option<(SdkKeyType, String, String)> {
		let (key_type, rest) = if let Some(rest) = key.strip_prefix(Self::CLIENT_PREFIX) {
			(SdkKeyType::ClientSide, rest)
		} else if let Some(rest) = key.strip_prefix(Self::SERVER_PREFIX) {
			(SdkKeyType::ServerSide, rest)
		} else {
			return None;
		};

		// The random part is a 32-char hex string at the end, preceded by underscore
		// So we need at least 33 chars: {env}_{ 32-char-hex }
		if rest.len() < 34 {
			return None;
		}

		// Find the last underscore before the 32-char random suffix
		let random_start = rest.len() - 32;
		if !rest.is_char_boundary(random_start) || !rest.is_char_boundary(random_start - 1) {
			return None;
		}

		let separator_idx = random_start - 1;
		if rest.as_bytes().get(separator_idx) != Some(&b'_') {
			return None;
		}

		let env_name = &rest[..separator_idx];
		let random = &rest[random_start..];

		// Validate that random is 32 hex characters
		if random.len() != 32 || !random.chars().all(|c| c.is_ascii_hexdigit()) {
			return None;
		}

		// Env name must not be empty
		if env_name.is_empty() {
			return None;
		}

		Some((key_type, env_name.to_string(), random.to_string()))
	}

	/// Generates a new SDK key string (not the hash).
	///
	/// Format: `loom_sdk_{type}_{env}_{random}`
	pub fn generate_key(key_type: SdkKeyType, environment_name: &str) -> String {
		let random = Uuid::new_v4().to_string().replace('-', "");
		let prefix = match key_type {
			SdkKeyType::ClientSide => Self::CLIENT_PREFIX,
			SdkKeyType::ServerSide => Self::SERVER_PREFIX,
		};
		format!("{}{environment_name}_{random}", prefix)
	}
}

/// Type of SDK key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SdkKeyType {
	/// Safe for browser, single user context
	ClientSide,
	/// Secret, backend only, any user context
	ServerSide,
}

impl SdkKeyType {
	pub fn as_str(&self) -> &'static str {
		match self {
			SdkKeyType::ClientSide => "client_side",
			SdkKeyType::ServerSide => "server_side",
		}
	}
}

impl std::fmt::Display for SdkKeyType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.as_str())
	}
}

impl std::str::FromStr for SdkKeyType {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"client_side" => Ok(SdkKeyType::ClientSide),
			"server_side" => Ok(SdkKeyType::ServerSide),
			_ => Err(format!("invalid SDK key type: {}", s)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_generate_key() {
		let client_key = SdkKey::generate_key(SdkKeyType::ClientSide, "prod");
		assert!(client_key.starts_with("loom_sdk_client_prod_"));

		let server_key = SdkKey::generate_key(SdkKeyType::ServerSide, "dev");
		assert!(server_key.starts_with("loom_sdk_server_dev_"));
	}

	#[test]
	fn test_parse_key_valid() {
		// 32-char hex random suffix
		let (key_type, env, random) =
			SdkKey::parse_key("loom_sdk_client_prod_abc123def456abc123def456abc123de").unwrap();
		assert_eq!(key_type, SdkKeyType::ClientSide);
		assert_eq!(env, "prod");
		assert_eq!(random, "abc123def456abc123def456abc123de");

		let (key_type, env, random) =
			SdkKey::parse_key("loom_sdk_server_dev_1234567890abcdef1234567890abcdef").unwrap();
		assert_eq!(key_type, SdkKeyType::ServerSide);
		assert_eq!(env, "dev");
		assert_eq!(random, "1234567890abcdef1234567890abcdef");

		// Env name with underscore
		let (key_type, env, random) =
			SdkKey::parse_key("loom_sdk_server_my_test_env_1234567890abcdef1234567890abcdef").unwrap();
		assert_eq!(key_type, SdkKeyType::ServerSide);
		assert_eq!(env, "my_test_env");
		assert_eq!(random, "1234567890abcdef1234567890abcdef");
	}

	#[test]
	fn test_parse_key_invalid() {
		assert!(SdkKey::parse_key("invalid_key").is_none());
		assert!(SdkKey::parse_key("loom_sdk_").is_none());
		assert!(SdkKey::parse_key("loom_sdk_unknown_prod_abc").is_none());
		assert!(SdkKey::parse_key("").is_none());
		// Too short random part
		assert!(SdkKey::parse_key("loom_sdk_client_prod_abc123").is_none());
		// Missing underscore before random
		assert!(SdkKey::parse_key("loom_sdk_client_1234567890abcdef1234567890abcdef").is_none());
	}

	#[test]
	fn test_sdk_key_type_str() {
		assert_eq!(SdkKeyType::ClientSide.as_str(), "client_side");
		assert_eq!(SdkKeyType::ServerSide.as_str(), "server_side");

		assert_eq!(
			"client_side".parse::<SdkKeyType>().unwrap(),
			SdkKeyType::ClientSide
		);
		assert_eq!(
			"server_side".parse::<SdkKeyType>().unwrap(),
			SdkKeyType::ServerSide
		);
	}

	proptest! {
		/// Test that generated keys can be parsed back to recover the type and environment.
		#[test]
		fn sdk_key_roundtrip(env_name in "[a-z][a-z0-9_]{1,20}", is_client in proptest::bool::ANY) {
			let key_type = if is_client { SdkKeyType::ClientSide } else { SdkKeyType::ServerSide };
			let generated = SdkKey::generate_key(key_type, &env_name);
			let parsed = SdkKey::parse_key(&generated);

			prop_assert!(parsed.is_some(), "Failed to parse generated key: {}", generated);
			let (parsed_type, parsed_env, _random) = parsed.unwrap();
			prop_assert_eq!(parsed_type, key_type);
			prop_assert_eq!(parsed_env, env_name);
		}

		/// Test that generated keys are unique.
		#[test]
		fn sdk_keys_are_unique(env_name in "[a-z][a-z0-9_]{1,10}") {
			let key1 = SdkKey::generate_key(SdkKeyType::ServerSide, &env_name);
			let key2 = SdkKey::generate_key(SdkKeyType::ServerSide, &env_name);
			prop_assert_ne!(key1, key2, "Generated keys should be unique");
		}

		/// Test that random garbage doesn't parse as a valid key.
		#[test]
		fn random_strings_dont_parse(garbage in "[a-zA-Z0-9_]{0,50}") {
			// Only strings that happen to match the prefix pattern should parse
			if !garbage.starts_with("loom_sdk_client_") && !garbage.starts_with("loom_sdk_server_") {
				prop_assert!(SdkKey::parse_key(&garbage).is_none());
			}
		}
	}
}
