// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Share link management for external read-only thread access.
//!
//! Share links allow thread owners to share read-only access to threads
//! with external users (not logged in or from different organizations).
//! Links are stored hashed for security.

use crate::argon2_config::argon2_instance;
use crate::UserId;
use argon2::password_hash::{
	rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Number of random bytes in a share token (produces 48 hex chars).
pub const SHARE_TOKEN_BYTES: usize = 24;

/// A stored share link (token is hashed, never stored in plaintext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareLink {
	/// Unique identifier for the share link.
	pub id: Uuid,
	/// Thread this link provides access to.
	pub thread_id: String,
	/// Argon2 hash of the token (the actual token is never stored).
	pub token_hash: String,
	/// User who created the link.
	pub created_by: UserId,
	/// When the link was created.
	pub created_at: DateTime<Utc>,
	/// When the link expires (None = never expires).
	pub expires_at: Option<DateTime<Utc>>,
	/// When the link was revoked (None if active).
	pub revoked_at: Option<DateTime<Utc>>,
}

impl ShareLink {
	/// Create a new share link for a thread.
	///
	/// Returns both the ShareLink struct and the plaintext token.
	/// The plaintext token is only available at creation time.
	pub fn new(
		thread_id: impl Into<String>,
		created_by: UserId,
		expires_at: Option<DateTime<Utc>>,
	) -> (Self, String) {
		let (plaintext_token, token_hash) = generate_share_token();
		let now = Utc::now();

		let share_link = Self {
			id: Uuid::new_v4(),
			thread_id: thread_id.into(),
			token_hash,
			created_by,
			created_at: now,
			expires_at,
			revoked_at: None,
		};

		(share_link, plaintext_token)
	}

	/// Check if the link is currently valid (not expired, not revoked).
	pub fn is_valid(&self) -> bool {
		if self.revoked_at.is_some() {
			return false;
		}
		if let Some(expires_at) = self.expires_at {
			if Utc::now() >= expires_at {
				return false;
			}
		}
		true
	}

	/// Revoke the share link.
	pub fn revoke(&mut self) {
		self.revoked_at = Some(Utc::now());
	}

	/// Verify a plaintext token against this link's hash.
	pub fn verify(&self, plaintext_token: &str) -> bool {
		verify_share_token(plaintext_token, &self.token_hash)
	}
}

/// Generate a new share token.
///
/// Returns a tuple of (plaintext_token, argon2_hash).
/// The plaintext token is 48 hex characters (24 bytes).
pub fn generate_share_token() -> (String, String) {
	use rand::Rng;
	let mut rng = rand::thread_rng();
	let bytes: [u8; SHARE_TOKEN_BYTES] = rng.gen();
	let token = hex::encode(bytes);
	let hash = hash_share_token(&token);
	(token, hash)
}

/// Hash a share token using Argon2.
///
/// The resulting hash can be safely stored in the database.
/// Uses production-strength parameters in release builds,
/// and fast test parameters in test builds.
pub fn hash_share_token(token: &str) -> String {
	let salt = SaltString::generate(&mut OsRng);
	let argon2 = argon2_instance();
	argon2
		.hash_password(token.as_bytes(), &salt)
		.expect("Argon2 hashing should not fail")
		.to_string()
}

/// Verify a share token against its stored Argon2 hash.
///
/// Returns true if the token matches the hash.
pub fn verify_share_token(token: &str, hash: &str) -> bool {
	let parsed_hash = match PasswordHash::new(hash) {
		Ok(h) => h,
		Err(_) => return false,
	};
	argon2_instance()
		.verify_password(token.as_bytes(), &parsed_hash)
		.is_ok()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	mod token_generation {
		use super::*;

		#[test]
		fn generates_token_with_correct_length() {
			let (token, _hash) = generate_share_token();
			// 24 bytes = 48 hex chars
			assert_eq!(token.len(), SHARE_TOKEN_BYTES * 2);
		}

		#[test]
		fn generates_token_with_valid_hex() {
			let (token, _hash) = generate_share_token();
			assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_tokens() {
			let tokens: HashSet<_> = (0..100).map(|_| generate_share_token().0).collect();
			assert_eq!(tokens.len(), 100, "All tokens should be unique");
		}

		#[test]
		fn generated_token_verifies_against_hash() {
			let (token, hash) = generate_share_token();
			assert!(verify_share_token(&token, &hash));
		}
	}

	mod hash_verification {
		use super::*;

		#[test]
		fn hash_produces_argon2_format() {
			let hash = hash_share_token("test_token");
			assert!(hash.starts_with("$argon2"));
		}

		#[test]
		fn same_token_produces_different_hashes() {
			let hash1 = hash_share_token("test_token");
			let hash2 = hash_share_token("test_token");
			assert_ne!(
				hash1, hash2,
				"Different salts should produce different hashes"
			);
		}

		#[test]
		fn correct_token_verifies() {
			let token = "0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_share_token(token);
			assert!(verify_share_token(token, &hash));
		}

		#[test]
		fn wrong_token_fails_verification() {
			let token = "0123456789abcdef0123456789abcdef0123456789abcdef";
			let hash = hash_share_token(token);
			let wrong_token = "ffffffffffffffffffffffffffffffffffffffffffff";
			assert!(!verify_share_token(wrong_token, &hash));
		}

		#[test]
		fn invalid_hash_fails_verification() {
			assert!(!verify_share_token("any_token", "invalid_hash"));
		}
	}

	mod share_link_struct {
		use super::*;
		use chrono::Duration;

		#[test]
		fn new_creates_valid_link() {
			let user_id = UserId::generate();
			let (link, plaintext) = ShareLink::new("T-test-thread", user_id, None);

			assert!(link.is_valid());
			assert_eq!(link.thread_id, "T-test-thread");
			assert_eq!(link.created_by, user_id);
			assert!(link.expires_at.is_none());
			assert!(link.revoked_at.is_none());
			assert_eq!(plaintext.len(), SHARE_TOKEN_BYTES * 2);
		}

		#[test]
		fn new_with_expiry_sets_expires_at() {
			let user_id = UserId::generate();
			let expires = Utc::now() + Duration::days(7);
			let (link, _) = ShareLink::new("T-test-thread", user_id, Some(expires));

			assert_eq!(link.expires_at, Some(expires));
		}

		#[test]
		fn revoke_marks_link_invalid() {
			let user_id = UserId::generate();
			let (mut link, _) = ShareLink::new("T-test-thread", user_id, None);

			assert!(link.is_valid());
			link.revoke();
			assert!(!link.is_valid());
			assert!(link.revoked_at.is_some());
		}

		#[test]
		fn expired_link_is_invalid() {
			let user_id = UserId::generate();
			let expired = Utc::now() - Duration::hours(1);
			let (link, _) = ShareLink::new("T-test-thread", user_id, Some(expired));

			assert!(!link.is_valid());
		}

		#[test]
		fn future_expiry_link_is_valid() {
			let user_id = UserId::generate();
			let future = Utc::now() + Duration::days(7);
			let (link, _) = ShareLink::new("T-test-thread", user_id, Some(future));

			assert!(link.is_valid());
		}

		#[test]
		fn verify_works_with_stored_link() {
			let user_id = UserId::generate();
			let (link, plaintext) = ShareLink::new("T-test-thread", user_id, None);

			assert!(link.verify(&plaintext));
			assert!(!link.verify("wrong_token"));
		}
	}
}
