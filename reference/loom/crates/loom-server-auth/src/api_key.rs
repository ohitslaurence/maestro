// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! API key management for organization-level programmatic access.
//!
//! API keys are org-level only (not user-level), use action-based scopes,
//! and are stored hashed with Argon2.

use crate::argon2_config::argon2_instance;
use crate::{ApiKeyId, ApiKeyScope, OrgId, UserId};
use argon2::password_hash::{
	rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

/// Prefix for all Loom API keys.
pub const API_KEY_PREFIX: &str = "lk_";

/// Number of random bytes in an API key (produces 64 hex chars).
pub const API_KEY_BYTES: usize = 32;

/// A stored API key (token is hashed, never stored in plaintext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
	/// Unique identifier for the API key.
	pub id: ApiKeyId,
	/// Organization this key belongs to.
	pub org_id: OrgId,
	/// Human-readable name for the key.
	pub name: String,
	/// Argon2 hash of the token (the actual key is never stored).
	pub token_hash: String,
	/// Scopes granted to this key.
	pub scopes: Vec<ApiKeyScope>,
	/// User who created the key.
	pub created_by: UserId,
	/// When the key was created.
	pub created_at: DateTime<Utc>,
	/// When the key was last used (for activity tracking).
	pub last_used_at: Option<DateTime<Utc>>,
	/// When the key was revoked (None if active).
	pub revoked_at: Option<DateTime<Utc>>,
	/// User who revoked the key (None if active).
	pub revoked_by: Option<UserId>,
}

impl ApiKey {
	/// Create a new API key.
	///
	/// Returns both the ApiKey struct and the plaintext key string.
	/// The plaintext key is only available at creation time.
	pub fn new(
		org_id: OrgId,
		name: impl Into<String>,
		scopes: Vec<ApiKeyScope>,
		created_by: UserId,
	) -> (Self, String) {
		let (plaintext_key, token_hash) = generate_api_key();
		let now = Utc::now();

		let api_key = Self {
			id: ApiKeyId::generate(),
			org_id,
			name: name.into(),
			token_hash,
			scopes,
			created_by,
			created_at: now,
			last_used_at: None,
			revoked_at: None,
			revoked_by: None,
		};

		(api_key, plaintext_key)
	}

	/// Check if the key is currently active (not revoked).
	pub fn is_active(&self) -> bool {
		self.revoked_at.is_none()
	}

	/// Revoke the API key.
	pub fn revoke(&mut self, revoked_by: UserId) {
		self.revoked_at = Some(Utc::now());
		self.revoked_by = Some(revoked_by);
	}

	/// Update the last_used_at timestamp.
	pub fn mark_used(&mut self) {
		self.last_used_at = Some(Utc::now());
	}

	/// Check if the key has a specific scope.
	pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
		self.scopes.contains(&scope)
	}

	/// Verify a plaintext key against this key's hash.
	pub fn verify(&self, plaintext_key: &str) -> bool {
		verify_api_key(plaintext_key, &self.token_hash)
	}
}

/// Usage log entry for API key access tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyUsage {
	/// Unique identifier for this usage record.
	pub id: Uuid,
	/// The API key that was used.
	pub api_key_id: ApiKeyId,
	/// When the request was made.
	pub timestamp: DateTime<Utc>,
	/// Client IP address.
	pub ip_address: Option<IpAddr>,
	/// API endpoint accessed.
	pub endpoint: String,
	/// HTTP method used.
	pub method: String,
}

impl ApiKeyUsage {
	/// Create a new usage log entry.
	pub fn new(api_key_id: ApiKeyId, endpoint: impl Into<String>, method: impl Into<String>) -> Self {
		Self {
			id: Uuid::new_v4(),
			api_key_id,
			timestamp: Utc::now(),
			ip_address: None,
			endpoint: endpoint.into(),
			method: method.into(),
		}
	}

	/// Set the IP address.
	pub fn with_ip(mut self, ip: IpAddr) -> Self {
		self.ip_address = Some(ip);
		self
	}
}

/// Generate a new API key.
///
/// Returns a tuple of (plaintext_key, argon2_hash).
/// The plaintext key format is: `lk_` + 64 hex characters (32 bytes).
pub fn generate_api_key() -> (String, String) {
	use rand::Rng;
	let mut rng = rand::thread_rng();
	let bytes: [u8; API_KEY_BYTES] = rng.gen();
	let key = format!("{}{}", API_KEY_PREFIX, hex::encode(bytes));
	let hash = hash_api_key(&key);
	(key, hash)
}

/// Hash an API key using Argon2.
///
/// The resulting hash can be safely stored in the database.
/// Uses production-strength parameters in release builds,
/// and fast test parameters in test builds.
pub fn hash_api_key(key: &str) -> String {
	let salt = SaltString::generate(&mut OsRng);
	let argon2 = argon2_instance();
	argon2
		.hash_password(key.as_bytes(), &salt)
		.expect("Argon2 hashing should not fail")
		.to_string()
}

/// Verify an API key against its stored Argon2 hash.
///
/// Returns true if the key matches the hash.
pub fn verify_api_key(key: &str, hash: &str) -> bool {
	let parsed_hash = match PasswordHash::new(hash) {
		Ok(h) => h,
		Err(_) => return false,
	};
	argon2_instance()
		.verify_password(key.as_bytes(), &parsed_hash)
		.is_ok()
}

/// Extract the key ID portion from a full API key string.
///
/// Returns None if the key doesn't have the correct prefix.
pub fn parse_api_key_prefix(key: &str) -> Option<&str> {
	key.strip_prefix(API_KEY_PREFIX)
}

/// Check if a string looks like a valid API key format.
pub fn is_valid_api_key_format(key: &str) -> bool {
	if let Some(hex_part) = key.strip_prefix(API_KEY_PREFIX) {
		hex_part.len() == API_KEY_BYTES * 2 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
	} else {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	mod key_generation {
		use super::*;

		#[test]
		fn generates_key_with_correct_prefix() {
			let (key, _hash) = generate_api_key();
			assert!(key.starts_with(API_KEY_PREFIX));
		}

		#[test]
		fn generates_key_with_correct_length() {
			let (key, _hash) = generate_api_key();
			// lk_ (3 chars) + 64 hex chars = 67 chars
			assert_eq!(key.len(), API_KEY_PREFIX.len() + API_KEY_BYTES * 2);
		}

		#[test]
		fn generates_key_with_valid_hex() {
			let (key, _hash) = generate_api_key();
			let hex_part = key.strip_prefix(API_KEY_PREFIX).unwrap();
			assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_keys() {
			let keys: HashSet<_> = (0..100).map(|_| generate_api_key().0).collect();
			assert_eq!(keys.len(), 100, "All keys should be unique");
		}

		#[test]
		fn generated_key_verifies_against_hash() {
			let (key, hash) = generate_api_key();
			assert!(verify_api_key(&key, &hash));
		}
	}

	mod hash_verification {
		use super::*;

		#[test]
		fn hash_produces_argon2_format() {
			let hash = hash_api_key("test_key");
			assert!(hash.starts_with("$argon2"));
		}

		#[test]
		fn same_key_produces_different_hashes() {
			let hash1 = hash_api_key("test_key");
			let hash2 = hash_api_key("test_key");
			assert_ne!(
				hash1, hash2,
				"Different salts should produce different hashes"
			);
		}

		#[test]
		fn correct_key_verifies() {
			let key = "lk_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_api_key(key);
			assert!(verify_api_key(key, &hash));
		}

		#[test]
		fn wrong_key_fails_verification() {
			let key = "lk_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_api_key(key);
			let wrong_key = "lk_ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
			assert!(!verify_api_key(wrong_key, &hash));
		}

		#[test]
		fn invalid_hash_fails_verification() {
			assert!(!verify_api_key("any_key", "invalid_hash"));
		}
	}

	mod api_key_struct {
		use super::*;

		#[test]
		fn new_creates_active_key() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let (api_key, plaintext) =
				ApiKey::new(org_id, "Test Key", vec![ApiKeyScope::ThreadsRead], user_id);

			assert!(api_key.is_active());
			assert!(plaintext.starts_with(API_KEY_PREFIX));
			assert_eq!(api_key.name, "Test Key");
			assert_eq!(api_key.org_id, org_id);
			assert_eq!(api_key.created_by, user_id);
		}

		#[test]
		fn revoke_marks_key_inactive() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let (mut api_key, _) = ApiKey::new(org_id, "Test", vec![ApiKeyScope::ThreadsRead], user_id);

			let revoker = UserId::generate();
			api_key.revoke(revoker);

			assert!(!api_key.is_active());
			assert!(api_key.revoked_at.is_some());
			assert_eq!(api_key.revoked_by, Some(revoker));
		}

		#[test]
		fn has_scope_returns_correct_result() {
			let (api_key, _) = ApiKey::new(
				OrgId::generate(),
				"Test",
				vec![ApiKeyScope::ThreadsRead, ApiKeyScope::LlmUse],
				UserId::generate(),
			);

			assert!(api_key.has_scope(ApiKeyScope::ThreadsRead));
			assert!(api_key.has_scope(ApiKeyScope::LlmUse));
			assert!(!api_key.has_scope(ApiKeyScope::ThreadsWrite));
			assert!(!api_key.has_scope(ApiKeyScope::ThreadsDelete));
		}

		#[test]
		fn verify_works_with_stored_key() {
			let (api_key, plaintext) = ApiKey::new(
				OrgId::generate(),
				"Test",
				vec![ApiKeyScope::ThreadsRead],
				UserId::generate(),
			);

			assert!(api_key.verify(&plaintext));
			assert!(!api_key.verify("wrong_key"));
		}

		#[test]
		fn mark_used_updates_timestamp() {
			let (mut api_key, _) = ApiKey::new(
				OrgId::generate(),
				"Test",
				vec![ApiKeyScope::ThreadsRead],
				UserId::generate(),
			);

			assert!(api_key.last_used_at.is_none());
			api_key.mark_used();
			assert!(api_key.last_used_at.is_some());
		}
	}

	mod format_validation {
		use super::*;

		#[test]
		fn is_valid_api_key_format_accepts_valid_key() {
			let (key, _) = generate_api_key();
			assert!(is_valid_api_key_format(&key));
		}

		#[test]
		fn is_valid_api_key_format_rejects_wrong_prefix() {
			let key = "xx_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			assert!(!is_valid_api_key_format(key));
		}

		#[test]
		fn is_valid_api_key_format_rejects_short_key() {
			let key = "lk_0123456789abcdef";
			assert!(!is_valid_api_key_format(key));
		}

		#[test]
		fn is_valid_api_key_format_rejects_non_hex() {
			let key = "lk_ghijklmnopqrstuv0123456789abcdef0123456789abcdef0123456789abcdef";
			assert!(!is_valid_api_key_format(key));
		}

		#[test]
		fn parse_api_key_prefix_extracts_hex() {
			let key = "lk_0123456789abcdef";
			assert_eq!(parse_api_key_prefix(key), Some("0123456789abcdef"));
		}

		#[test]
		fn parse_api_key_prefix_returns_none_for_wrong_prefix() {
			assert_eq!(parse_api_key_prefix("xx_abc"), None);
		}
	}

	mod api_key_usage {
		use super::*;

		#[test]
		fn new_creates_usage_record() {
			let api_key_id = ApiKeyId::generate();
			let usage = ApiKeyUsage::new(api_key_id, "/api/threads", "GET");

			assert_eq!(usage.api_key_id, api_key_id);
			assert_eq!(usage.endpoint, "/api/threads");
			assert_eq!(usage.method, "GET");
			assert!(usage.ip_address.is_none());
		}

		#[test]
		fn with_ip_sets_address() {
			let usage = ApiKeyUsage::new(ApiKeyId::generate(), "/api/threads", "GET")
				.with_ip("192.168.1.1".parse().unwrap());

			assert_eq!(usage.ip_address, Some("192.168.1.1".parse().unwrap()));
		}
	}
}
