// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Access token management for CLI and VS Code bearer token authentication.
//!
//! Access tokens are user-level bearer tokens with 60-day sliding expiry.
//! Tokens are stored hashed with SHA-256 and are only shown once at creation.

use crate::{SessionType, UserId};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Duration for access token sliding expiry (60 days).
pub const ACCESS_TOKEN_EXPIRY_DAYS: i64 = 60;

/// Number of random bytes in an access token (produces 64 hex chars).
pub const ACCESS_TOKEN_BYTES: usize = 32;

/// Prefix for all Loom access tokens.
pub const ACCESS_TOKEN_PREFIX: &str = "lt_";

/// A stored access token for CLI/VS Code authentication.
///
/// The plaintext token is only available at creation time and stored hashed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessToken {
	/// Unique identifier for this access token.
	pub id: Uuid,
	/// User who owns this token.
	pub user_id: UserId,
	/// SHA-256 hash of the token (the actual token is never stored).
	pub token_hash: String,
	/// Human-readable label (e.g., "MacBook CLI", "Work VS Code").
	pub label: String,
	/// Type of session this token is for.
	pub session_type: SessionType,
	/// When the token was created.
	pub created_at: DateTime<Utc>,
	/// When the token was last used.
	pub last_used_at: Option<DateTime<Utc>>,
	/// When the token expires (sliding, extended on each use).
	pub expires_at: DateTime<Utc>,
	/// Client IP address from last use.
	pub ip_address: Option<String>,
	/// User agent string from last use.
	pub user_agent: Option<String>,
	/// City from GeoIP lookup.
	pub geo_city: Option<String>,
	/// Country from GeoIP lookup.
	pub geo_country: Option<String>,
	/// When the token was revoked (None if active).
	pub revoked_at: Option<DateTime<Utc>>,
}

impl AccessToken {
	/// Create a new access token.
	///
	/// Returns both the AccessToken struct and the plaintext token string.
	/// The plaintext token is only available at creation time and must be
	/// shown to the user immediately.
	pub fn new(
		user_id: UserId,
		label: impl Into<String>,
		session_type: SessionType,
	) -> (Self, String) {
		let (plaintext_token, token_hash) = generate_access_token();
		let now = Utc::now();
		let expires_at = now + Duration::days(ACCESS_TOKEN_EXPIRY_DAYS);

		let access_token = Self {
			id: Uuid::new_v4(),
			user_id,
			token_hash,
			label: label.into(),
			session_type,
			created_at: now,
			last_used_at: None,
			expires_at,
			ip_address: None,
			user_agent: None,
			geo_city: None,
			geo_country: None,
			revoked_at: None,
		};

		(access_token, plaintext_token)
	}

	/// Check if the token has expired.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Check if the token has been revoked.
	pub fn is_revoked(&self) -> bool {
		self.revoked_at.is_some()
	}

	/// Check if the token is valid (not expired and not revoked).
	pub fn is_valid(&self) -> bool {
		!self.is_expired() && !self.is_revoked()
	}

	/// Extend the token expiry (sliding expiry on use).
	///
	/// Updates `last_used_at` to now and extends `expires_at` by 60 days.
	pub fn extend(&mut self) {
		let now = Utc::now();
		self.last_used_at = Some(now);
		self.expires_at = now + Duration::days(ACCESS_TOKEN_EXPIRY_DAYS);
	}

	/// Revoke the token.
	///
	/// Sets `revoked_at` to the current time.
	pub fn revoke(&mut self) {
		self.revoked_at = Some(Utc::now());
	}

	/// Verify a plaintext token against this token's hash.
	pub fn verify(&self, plaintext_token: &str) -> bool {
		verify_access_token(plaintext_token, &self.token_hash)
	}

	/// Set the IP address.
	pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
		self.ip_address = Some(ip.into());
		self
	}

	/// Set the user agent.
	pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
		self.user_agent = Some(ua.into());
		self
	}

	/// Set the geo location.
	pub fn with_geo(mut self, city: Option<String>, country: Option<String>) -> Self {
		self.geo_city = city;
		self.geo_country = country;
		self
	}

	/// Update metadata on token use.
	pub fn update_metadata(
		&mut self,
		ip_address: Option<String>,
		user_agent: Option<String>,
		geo_city: Option<String>,
		geo_country: Option<String>,
	) {
		self.ip_address = ip_address;
		self.user_agent = user_agent;
		self.geo_city = geo_city;
		self.geo_country = geo_country;
	}
}

/// Generate a new access token.
///
/// Returns a tuple of (plaintext_token, sha256_hash).
/// The plaintext token format is: `lt_` + 64 hex characters (32 bytes).
pub fn generate_access_token() -> (String, String) {
	use rand::Rng;
	let mut rng = rand::thread_rng();
	let bytes: [u8; ACCESS_TOKEN_BYTES] = rng.gen();
	let token = format!("{}{}", ACCESS_TOKEN_PREFIX, hex::encode(bytes));
	let hash = hash_access_token(&token);
	(token, hash)
}

/// Hash an access token using SHA-256.
///
/// The resulting hash can be safely stored in the database.
/// SHA-256 is sufficient for high-entropy random tokens (32+ bytes).
/// This must match the hash function used in auth_middleware for lookup.
pub fn hash_access_token(token: &str) -> String {
	use sha2::{Digest, Sha256};
	let mut hasher = Sha256::new();
	hasher.update(token.as_bytes());
	hex::encode(hasher.finalize())
}

/// Verify an access token against its stored SHA-256 hash.
///
/// Returns true if the token matches the hash.
pub fn verify_access_token(token: &str, hash: &str) -> bool {
	hash_access_token(token) == hash
}

/// Check if a string looks like a valid access token format.
pub fn is_valid_access_token_format(token: &str) -> bool {
	if let Some(hex_part) = token.strip_prefix(ACCESS_TOKEN_PREFIX) {
		hex_part.len() == ACCESS_TOKEN_BYTES * 2 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
	} else {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	mod token_generation {
		use super::*;

		#[test]
		fn generates_token_with_correct_prefix() {
			let (token, _hash) = generate_access_token();
			assert!(token.starts_with(ACCESS_TOKEN_PREFIX));
		}

		#[test]
		fn generates_token_with_correct_length() {
			let (token, _hash) = generate_access_token();
			// lt_ (3 chars) + 64 hex chars = 67 chars
			assert_eq!(
				token.len(),
				ACCESS_TOKEN_PREFIX.len() + ACCESS_TOKEN_BYTES * 2
			);
		}

		#[test]
		fn generates_token_with_valid_hex() {
			let (token, _hash) = generate_access_token();
			let hex_part = token.strip_prefix(ACCESS_TOKEN_PREFIX).unwrap();
			assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_tokens() {
			let tokens: HashSet<_> = (0..100).map(|_| generate_access_token().0).collect();
			assert_eq!(tokens.len(), 100, "All tokens should be unique");
		}

		#[test]
		fn generated_token_verifies_against_hash() {
			let (token, hash) = generate_access_token();
			assert!(verify_access_token(&token, &hash));
		}
	}

	mod hash_verification {
		use super::*;

		#[test]
		fn hash_produces_hex_sha256_format() {
			let hash = hash_access_token("test_token");
			assert_eq!(hash.len(), 64, "SHA-256 produces 64 hex characters");
			assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn same_token_produces_same_hash() {
			let hash1 = hash_access_token("test_token");
			let hash2 = hash_access_token("test_token");
			assert_eq!(hash1, hash2, "SHA-256 is deterministic");
		}

		#[test]
		fn correct_token_verifies() {
			let token = "lt_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_access_token(token);
			assert!(verify_access_token(token, &hash));
		}

		#[test]
		fn wrong_token_fails_verification() {
			let token = "lt_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_access_token(token);
			let wrong_token = "lt_ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
			assert!(!verify_access_token(wrong_token, &hash));
		}

		#[test]
		fn invalid_hash_fails_verification() {
			assert!(!verify_access_token("any_token", "invalid_hash"));
		}
	}

	mod access_token_struct {
		use super::*;

		#[test]
		fn new_creates_valid_token() {
			let user_id = UserId::generate();
			let (access_token, plaintext) = AccessToken::new(user_id, "MacBook CLI", SessionType::Cli);

			assert!(access_token.is_valid());
			assert!(plaintext.starts_with(ACCESS_TOKEN_PREFIX));
			assert_eq!(access_token.label, "MacBook CLI");
			assert_eq!(access_token.user_id, user_id);
			assert_eq!(access_token.session_type, SessionType::Cli);
		}

		#[test]
		fn new_creates_token_with_60_day_expiry() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);

			let expected_expiry = access_token.created_at + Duration::days(ACCESS_TOKEN_EXPIRY_DAYS);
			let diff = (access_token.expires_at - expected_expiry)
				.num_seconds()
				.abs();
			assert!(diff < 1, "Expiry should be ~60 days from creation");
		}

		#[test]
		fn new_creates_token_with_unique_ids() {
			let user_id = UserId::generate();
			let (token1, _) = AccessToken::new(user_id, "Token 1", SessionType::Cli);
			let (token2, _) = AccessToken::new(user_id, "Token 2", SessionType::Cli);

			assert_ne!(token1.id, token2.id);
		}

		#[test]
		fn is_expired_returns_false_for_new_token() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			assert!(!access_token.is_expired());
		}

		#[test]
		fn is_expired_returns_true_for_expired_token() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			access_token.expires_at = Utc::now() - Duration::seconds(1);
			assert!(access_token.is_expired());
		}

		#[test]
		fn is_revoked_returns_false_for_active_token() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			assert!(!access_token.is_revoked());
		}

		#[test]
		fn is_revoked_returns_true_after_revoke() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			access_token.revoke();
			assert!(access_token.is_revoked());
			assert!(access_token.revoked_at.is_some());
		}

		#[test]
		fn is_valid_returns_true_for_active_unexpired_token() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			assert!(access_token.is_valid());
		}

		#[test]
		fn is_valid_returns_false_for_expired_token() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			access_token.expires_at = Utc::now() - Duration::seconds(1);
			assert!(!access_token.is_valid());
		}

		#[test]
		fn is_valid_returns_false_for_revoked_token() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			access_token.revoke();
			assert!(!access_token.is_valid());
		}

		#[test]
		fn extend_updates_last_used_at() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			assert!(access_token.last_used_at.is_none());

			std::thread::sleep(std::time::Duration::from_millis(10));
			access_token.extend();

			assert!(access_token.last_used_at.is_some());
		}

		#[test]
		fn extend_resets_expiry_to_60_days() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);
			access_token.expires_at = Utc::now() + Duration::days(1);

			access_token.extend();

			let expected_expiry = Utc::now() + Duration::days(ACCESS_TOKEN_EXPIRY_DAYS);
			let diff = (access_token.expires_at - expected_expiry)
				.num_seconds()
				.abs();
			assert!(diff < 1, "Expiry should be reset to ~60 days");
		}

		#[test]
		fn verify_works_with_stored_token() {
			let (access_token, plaintext) =
				AccessToken::new(UserId::generate(), "Test", SessionType::Cli);

			assert!(access_token.verify(&plaintext));
			assert!(!access_token.verify("wrong_token"));
		}

		#[test]
		fn builder_methods_set_metadata() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::VsCode);
			let access_token = access_token
				.with_ip("192.168.1.1")
				.with_user_agent("vscode/1.85.0")
				.with_geo(Some("Sydney".to_string()), Some("Australia".to_string()));

			assert_eq!(access_token.ip_address, Some("192.168.1.1".to_string()));
			assert_eq!(access_token.user_agent, Some("vscode/1.85.0".to_string()));
			assert_eq!(access_token.geo_city, Some("Sydney".to_string()));
			assert_eq!(access_token.geo_country, Some("Australia".to_string()));
		}

		#[test]
		fn update_metadata_updates_all_fields() {
			let (mut access_token, _) = AccessToken::new(UserId::generate(), "Test", SessionType::Cli);

			access_token.update_metadata(
				Some("10.0.0.1".to_string()),
				Some("loom-cli/1.0".to_string()),
				Some("Melbourne".to_string()),
				Some("Australia".to_string()),
			);

			assert_eq!(access_token.ip_address, Some("10.0.0.1".to_string()));
			assert_eq!(access_token.user_agent, Some("loom-cli/1.0".to_string()));
			assert_eq!(access_token.geo_city, Some("Melbourne".to_string()));
			assert_eq!(access_token.geo_country, Some("Australia".to_string()));
		}
	}

	mod format_validation {
		use super::*;

		#[test]
		fn is_valid_access_token_format_accepts_valid_token() {
			let (token, _) = generate_access_token();
			assert!(is_valid_access_token_format(&token));
		}

		#[test]
		fn is_valid_access_token_format_rejects_wrong_prefix() {
			let token = "xx_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			assert!(!is_valid_access_token_format(token));
		}

		#[test]
		fn is_valid_access_token_format_rejects_api_key_prefix() {
			let token = "lk_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
			assert!(!is_valid_access_token_format(token));
		}

		#[test]
		fn is_valid_access_token_format_rejects_short_token() {
			let token = "lt_0123456789abcdef";
			assert!(!is_valid_access_token_format(token));
		}

		#[test]
		fn is_valid_access_token_format_rejects_non_hex() {
			let token = "lt_ghijklmnopqrstuv0123456789abcdef0123456789abcdef0123456789abcdef";
			assert!(!is_valid_access_token_format(token));
		}
	}

	mod session_types {
		use super::*;

		#[test]
		fn creates_cli_token() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "CLI", SessionType::Cli);
			assert_eq!(access_token.session_type, SessionType::Cli);
		}

		#[test]
		fn creates_vscode_token() {
			let (access_token, _) = AccessToken::new(UserId::generate(), "VS Code", SessionType::VsCode);
			assert_eq!(access_token.session_type, SessionType::VsCode);
		}
	}
}
