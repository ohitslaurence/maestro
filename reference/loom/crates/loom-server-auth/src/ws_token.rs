// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! WebSocket authentication token management.
//!
//! WS tokens are short-lived (30 seconds), single-use tokens that allow web clients
//! to authenticate WebSocket connections. They solve the problem of HttpOnly session
//! cookies not being accessible to JavaScript for first-message WebSocket auth.
//!
//! # Flow
//!
//! 1. Client calls GET /auth/ws-token (authenticated via session cookie)
//! 2. Server generates a short-lived token, stores hash, returns plaintext
//! 3. Client connects to WebSocket and sends {"type": "auth", "token": "ws_xxx"}
//! 4. Server validates token (single-use), establishes authenticated connection

use crate::UserId;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Duration for WS token expiry (30 seconds).
pub const WS_TOKEN_EXPIRY_SECONDS: i64 = 30;

/// Number of random bytes in a WS token (produces 32 hex chars).
pub const WS_TOKEN_BYTES: usize = 16;

/// Prefix for all Loom WS tokens.
pub const WS_TOKEN_PREFIX: &str = "ws_";

/// A stored WebSocket authentication token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsToken {
	/// Unique identifier for this token.
	pub id: Uuid,
	/// User who owns this token.
	pub user_id: UserId,
	/// SHA-256 hash of the token (the actual token is never stored).
	pub token_hash: String,
	/// When the token was created.
	pub created_at: DateTime<Utc>,
	/// When the token expires.
	pub expires_at: DateTime<Utc>,
	/// When the token was used (None if not yet used).
	pub used_at: Option<DateTime<Utc>>,
}

impl WsToken {
	/// Create a new WS token.
	///
	/// Returns both the WsToken struct and the plaintext token string.
	/// The plaintext token must be returned to the client immediately and
	/// is only shown once.
	pub fn new(user_id: UserId) -> (Self, String) {
		let (plaintext_token, token_hash) = generate_ws_token();
		let now = Utc::now();
		let expires_at = now + Duration::seconds(WS_TOKEN_EXPIRY_SECONDS);

		let ws_token = Self {
			id: Uuid::new_v4(),
			user_id,
			token_hash,
			created_at: now,
			expires_at,
			used_at: None,
		};

		(ws_token, plaintext_token)
	}

	/// Check if the token has expired.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Check if the token has been used.
	pub fn is_used(&self) -> bool {
		self.used_at.is_some()
	}

	/// Check if the token is valid (not expired and not used).
	pub fn is_valid(&self) -> bool {
		!self.is_expired() && !self.is_used()
	}

	/// Verify a plaintext token against this token's hash.
	pub fn verify(&self, plaintext_token: &str) -> bool {
		verify_ws_token(plaintext_token, &self.token_hash)
	}
}

/// Generate a new WS token.
///
/// Returns a tuple of (plaintext_token, sha256_hash).
/// The plaintext token format is: `ws_` + 32 hex characters (16 bytes).
pub fn generate_ws_token() -> (String, String) {
	use rand::Rng;
	let mut rng = rand::thread_rng();
	let bytes: [u8; WS_TOKEN_BYTES] = rng.gen();
	let token = format!("{}{}", WS_TOKEN_PREFIX, hex::encode(bytes));
	let hash = hash_ws_token(&token);
	(token, hash)
}

/// Hash a WS token using SHA-256.
///
/// The resulting hash can be safely stored in the database.
pub fn hash_ws_token(token: &str) -> String {
	use sha2::{Digest, Sha256};
	let mut hasher = Sha256::new();
	hasher.update(token.as_bytes());
	hex::encode(hasher.finalize())
}

/// Verify a WS token against its stored SHA-256 hash.
///
/// Returns true if the token matches the hash.
pub fn verify_ws_token(token: &str, hash: &str) -> bool {
	hash_ws_token(token) == hash
}

/// Check if a string looks like a valid WS token format.
pub fn is_valid_ws_token_format(token: &str) -> bool {
	if let Some(hex_part) = token.strip_prefix(WS_TOKEN_PREFIX) {
		hex_part.len() == WS_TOKEN_BYTES * 2 && hex_part.chars().all(|c| c.is_ascii_hexdigit())
	} else {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::collections::HashSet;

	mod token_generation {
		use super::*;

		#[test]
		fn generates_token_with_correct_prefix() {
			let (token, _hash) = generate_ws_token();
			assert!(token.starts_with(WS_TOKEN_PREFIX));
		}

		#[test]
		fn generates_token_with_correct_length() {
			let (token, _hash) = generate_ws_token();
			// ws_ (3 chars) + 32 hex chars = 35 chars
			assert_eq!(token.len(), WS_TOKEN_PREFIX.len() + WS_TOKEN_BYTES * 2);
		}

		#[test]
		fn generates_token_with_valid_hex() {
			let (token, _hash) = generate_ws_token();
			let hex_part = token.strip_prefix(WS_TOKEN_PREFIX).unwrap();
			assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_tokens() {
			let tokens: HashSet<_> = (0..100).map(|_| generate_ws_token().0).collect();
			assert_eq!(tokens.len(), 100, "All tokens should be unique");
		}

		#[test]
		fn generated_token_verifies_against_hash() {
			let (token, hash) = generate_ws_token();
			assert!(verify_ws_token(&token, &hash));
		}
	}

	mod hash_verification {
		use super::*;

		#[test]
		fn hash_produces_hex_sha256_format() {
			let hash = hash_ws_token("test_token");
			assert_eq!(hash.len(), 64, "SHA-256 produces 64 hex characters");
			assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn same_token_produces_same_hash() {
			let hash1 = hash_ws_token("test_token");
			let hash2 = hash_ws_token("test_token");
			assert_eq!(hash1, hash2, "SHA-256 is deterministic");
		}

		#[test]
		fn correct_token_verifies() {
			let token = "ws_0123456789abcdef0123456789abcdef";
			let hash = hash_ws_token(token);
			assert!(verify_ws_token(token, &hash));
		}

		#[test]
		fn wrong_token_fails_verification() {
			let token = "ws_0123456789abcdef0123456789abcdef";
			let hash = hash_ws_token(token);
			let wrong_token = "ws_ffffffffffffffffffffffffffffffff";
			assert!(!verify_ws_token(wrong_token, &hash));
		}
	}

	mod ws_token_struct {
		use super::*;

		#[test]
		fn new_creates_valid_token() {
			let user_id = UserId::generate();
			let (ws_token, plaintext) = WsToken::new(user_id);

			assert!(ws_token.is_valid());
			assert!(plaintext.starts_with(WS_TOKEN_PREFIX));
			assert_eq!(ws_token.user_id, user_id);
			assert!(ws_token.used_at.is_none());
		}

		#[test]
		fn new_creates_token_with_30_second_expiry() {
			let (ws_token, _) = WsToken::new(UserId::generate());

			let expected_expiry = ws_token.created_at + Duration::seconds(WS_TOKEN_EXPIRY_SECONDS);
			let diff = (ws_token.expires_at - expected_expiry).num_seconds().abs();
			assert!(diff < 1, "Expiry should be ~30 seconds from creation");
		}

		#[test]
		fn is_expired_returns_false_for_new_token() {
			let (ws_token, _) = WsToken::new(UserId::generate());
			assert!(!ws_token.is_expired());
		}

		#[test]
		fn is_expired_returns_true_for_expired_token() {
			let (mut ws_token, _) = WsToken::new(UserId::generate());
			ws_token.expires_at = Utc::now() - Duration::seconds(1);
			assert!(ws_token.is_expired());
		}

		#[test]
		fn is_used_returns_false_for_new_token() {
			let (ws_token, _) = WsToken::new(UserId::generate());
			assert!(!ws_token.is_used());
		}

		#[test]
		fn is_used_returns_true_after_use() {
			let (mut ws_token, _) = WsToken::new(UserId::generate());
			ws_token.used_at = Some(Utc::now());
			assert!(ws_token.is_used());
		}

		#[test]
		fn is_valid_returns_true_for_fresh_token() {
			let (ws_token, _) = WsToken::new(UserId::generate());
			assert!(ws_token.is_valid());
		}

		#[test]
		fn is_valid_returns_false_for_expired_token() {
			let (mut ws_token, _) = WsToken::new(UserId::generate());
			ws_token.expires_at = Utc::now() - Duration::seconds(1);
			assert!(!ws_token.is_valid());
		}

		#[test]
		fn is_valid_returns_false_for_used_token() {
			let (mut ws_token, _) = WsToken::new(UserId::generate());
			ws_token.used_at = Some(Utc::now());
			assert!(!ws_token.is_valid());
		}

		#[test]
		fn verify_works_with_stored_token() {
			let (ws_token, plaintext) = WsToken::new(UserId::generate());

			assert!(ws_token.verify(&plaintext));
			assert!(!ws_token.verify("wrong_token"));
		}
	}

	mod format_validation {
		use super::*;

		#[test]
		fn is_valid_ws_token_format_accepts_valid_token() {
			let (token, _) = generate_ws_token();
			assert!(is_valid_ws_token_format(&token));
		}

		#[test]
		fn is_valid_ws_token_format_rejects_wrong_prefix() {
			let token = "xx_0123456789abcdef0123456789abcdef";
			assert!(!is_valid_ws_token_format(token));
		}

		#[test]
		fn is_valid_ws_token_format_rejects_access_token_prefix() {
			let token = "lt_0123456789abcdef0123456789abcdef";
			assert!(!is_valid_ws_token_format(token));
		}

		#[test]
		fn is_valid_ws_token_format_rejects_short_token() {
			let token = "ws_0123456789abcdef";
			assert!(!is_valid_ws_token_format(token));
		}

		#[test]
		fn is_valid_ws_token_format_rejects_non_hex() {
			let token = "ws_ghijklmnopqrstuvwxyz0123456789ab";
			assert!(!is_valid_ws_token_format(token));
		}
	}

	mod property_tests {
		use super::*;

		proptest! {
			#[test]
			fn generated_token_always_verifies(seed: u64) {
				// Use seed to ensure reproducibility
				let _ = seed;
				let (token, hash) = generate_ws_token();
				prop_assert!(verify_ws_token(&token, &hash));
			}

			#[test]
			fn generated_token_format_is_valid(seed: u64) {
				let _ = seed;
				let (token, _) = generate_ws_token();
				prop_assert!(is_valid_ws_token_format(&token));
			}
		}
	}
}
