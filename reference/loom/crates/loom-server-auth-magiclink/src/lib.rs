// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Magic link passwordless authentication.
//!
//! Magic links are 10-minute, single-use tokens sent via email.
//! When a new link is requested, any previous links for that email are invalidated.
//! Tokens are stored hashed with Argon2.
//!
//! # Security Properties
//!
//! - **Single-use tokens**: Each magic link can only be used once. After verification,
//!   the link is marked as used and cannot be reused.
//! - **Short-lived**: Links expire after [`MAGIC_LINK_EXPIRY_MINUTES`] (10 minutes).
//! - **Cryptographically secure**: Tokens are generated using 32 bytes of cryptographically
//!   secure random data from the OS RNG.
//! - **Secure storage**: Tokens are hashed with Argon2id before storage, protecting against
//!   database leaks.
//!
//! # Why Argon2 instead of SHA-256?
//!
//! We use Argon2id (a memory-hard password hashing function) instead of SHA-256 for token
//! hashing because:
//!
//! 1. **Brute-force resistance**: Argon2's memory-hard design makes GPU/ASIC attacks
//!    significantly more expensive than SHA-256.
//! 2. **Defense in depth**: While our 32-byte tokens have 256 bits of entropy (making
//!    brute-force infeasible), Argon2 provides additional protection if token generation
//!    is ever weakened.
//! 3. **Industry best practice**: OWASP recommends Argon2id for password and token hashing.
//! 4. **Salt included**: Each hash includes a unique salt, preventing rainbow table attacks.
//!
//! The trade-off is slightly higher CPU cost per verification, which is acceptable for
//! the low-frequency magic link authentication flow.

use argon2::password_hash::{
	rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
#[cfg(test)]
use argon2::{Algorithm, Params, Version};

/// Returns an Argon2 instance configured appropriately for the build context.
#[inline]
fn argon2_instance() -> Argon2<'static> {
	#[cfg(test)]
	{
		// Fast, insecure parameters for tests ONLY.
		let params = Params::new(1024, 1, 1, None).expect("valid Argon2 params for tests");
		Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
	}

	#[cfg(not(test))]
	{
		Argon2::default()
	}
}
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

/// Magic link expiry in minutes.
///
/// Links are intentionally short-lived (10 minutes) to minimize the window of
/// opportunity for token interception or replay attacks.
pub const MAGIC_LINK_EXPIRY_MINUTES: i64 = 10;

/// Number of random bytes for magic link token.
///
/// 32 bytes provides 256 bits of entropy, making brute-force attacks infeasible.
pub const MAGIC_LINK_TOKEN_BYTES: usize = 32;

/// A magic link for passwordless authentication.
///
/// Magic links provide a secure, user-friendly alternative to passwords. The flow is:
/// 1. User requests a magic link for their email
/// 2. Server creates a [`MagicLink`], storing the hashed token
/// 3. Server sends the plaintext token to the user's email
/// 4. User clicks the link, submitting the token
/// 5. Server verifies the token against the stored hash
/// 6. If valid and not expired/used, authentication succeeds
///
/// # Security Properties
///
/// - The plaintext token is only available at creation time
/// - The stored `token_hash` is an Argon2id hash (safe to store in database)
/// - Links are single-use (tracked via `used_at`)
/// - Links expire after [`MAGIC_LINK_EXPIRY_MINUTES`]
///
/// # Token Lifetime
///
/// ```text
/// Created ──────────────────────────────────> Expires (10 min)
///    │                                            │
///    │  Link is valid if:                         │
///    │  • Not yet used (used_at is None)          │
///    │  • Current time < expires_at               │
///    │                                            │
///    └────────[User clicks link]──────────────────┘
///                    │
///                    ▼
///              Link marked used
///              (used_at set, link invalidated)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagicLink {
	/// Unique identifier for the magic link.
	pub id: Uuid,
	/// Email address this link is for.
	pub email: String,
	/// Argon2id hash of the token.
	///
	/// The plaintext token is never stored; only this hash is persisted.
	pub token_hash: String,
	/// When the link was created.
	pub created_at: DateTime<Utc>,
	/// When the link expires.
	pub expires_at: DateTime<Utc>,
	/// When the link was used (None if unused).
	///
	/// Once set, the link cannot be used again (single-use semantics).
	pub used_at: Option<DateTime<Utc>>,
}

impl MagicLink {
	/// Create a new magic link for the given email.
	///
	/// Returns `(MagicLink, plaintext_token)` where:
	/// - `MagicLink` contains the hashed token (safe to store)
	/// - `plaintext_token` should be sent to the user's email (not logged, not stored)
	///
	/// # Security Notes
	///
	/// - The plaintext token is only available at creation time
	/// - The caller is responsible for securely transmitting the token (e.g., via email)
	/// - The token should never be logged or persisted in plaintext
	///
	/// # Example
	///
	/// ```
	/// use loom_server_auth_magiclink::MagicLink;
	///
	/// let (link, token) = MagicLink::new("user@example.com");
	/// // Store `link` in database
	/// // Send `token` to user via email
	/// ```
	#[instrument(
        name = "magic_link.create",
        skip_all,
        fields(
            link_id,
            email = %email.as_ref(),
        )
    )]
	pub fn new(email: impl AsRef<str> + Into<String>) -> (Self, String) {
		let (token, hash) = generate_magic_link_token();
		let now = Utc::now();
		let id = Uuid::new_v4();

		tracing::Span::current().record("link_id", id.to_string());

		let link = Self {
			id,
			email: email.into(),
			token_hash: hash,
			created_at: now,
			expires_at: now + Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES),
			used_at: None,
		};

		tracing::info!(
				expires_at = %link.expires_at,
				"created magic link"
		);

		(link, token)
	}

	/// Check if the magic link has expired.
	///
	/// Returns `true` if the current time is past `expires_at`.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Check if the magic link has been used.
	///
	/// Returns `true` if `used_at` is set (link was already consumed).
	pub fn is_used(&self) -> bool {
		self.used_at.is_some()
	}

	/// Check if the magic link is valid (not expired and not used).
	///
	/// A link must pass both checks to be considered valid for authentication.
	pub fn is_valid(&self) -> bool {
		!self.is_expired() && !self.is_used()
	}

	/// Mark the magic link as used.
	///
	/// After calling this, [`is_used`](Self::is_used) returns `true` and
	/// [`is_valid`](Self::is_valid) returns `false`.
	pub fn mark_used(&mut self) {
		self.used_at = Some(Utc::now());
	}

	/// Verify a plaintext token against this link's hash.
	///
	/// Returns `true` if the token matches the stored hash.
	///
	/// # Note
	///
	/// This only verifies the token matches; it does NOT check expiry or usage.
	/// Use [`is_valid`](Self::is_valid) to check those conditions separately.
	#[instrument(
        name = "magic_link.verify",
        skip(self, token),
        fields(
            link_id = %self.id,
            is_valid = self.is_valid(),
        )
    )]
	pub fn verify(&self, token: &str) -> bool {
		let result = verify_magic_link_token(token, &self.token_hash);
		tracing::debug!(verified = result, "token verification complete");
		result
	}
}

/// Generate a new magic link token.
///
/// Returns a tuple of `(plaintext_token, argon2_hash)` where:
/// - `plaintext_token` is a hex-encoded string (64 chars for 32 bytes)
/// - `argon2_hash` is the Argon2id hash of the token (safe to store)
///
/// # Security
///
/// Uses the OS cryptographically secure RNG for token generation.
pub fn generate_magic_link_token() -> (String, String) {
	use rand::Rng;

	let mut rng = rand::thread_rng();
	let bytes: [u8; MAGIC_LINK_TOKEN_BYTES] = rng.gen();
	let token = hex::encode(bytes);
	let hash = hash_magic_link_token(&token);
	(token, hash)
}

/// Hash a magic link token using Argon2id.
///
/// The resulting hash can be safely stored in the database. It includes:
/// - The Argon2id algorithm identifier
/// - A unique random salt
/// - The hash parameters
/// - The actual hash value
///
/// # Why Argon2id?
///
/// Argon2id is the recommended variant that provides resistance against both
/// side-channel attacks (from Argon2i) and GPU cracking attacks (from Argon2d).
#[instrument(name = "magic_link.hash", skip_all)]
pub fn hash_magic_link_token(token: &str) -> String {
	let salt = SaltString::generate(&mut OsRng);
	let argon2 = argon2_instance();
	argon2
		.hash_password(token.as_bytes(), &salt)
		.expect("Argon2 hashing should not fail")
		.to_string()
}

/// Verify a magic link token against its stored Argon2 hash.
///
/// Returns `true` if the token matches the hash, `false` otherwise.
///
/// # Security
///
/// - Uses constant-time comparison internally (provided by Argon2)
/// - Returns `false` for malformed hashes (does not panic)
#[instrument(name = "magic_link.verify_hash", skip_all)]
pub fn verify_magic_link_token(token: &str, hash: &str) -> bool {
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
			let (token, _hash) = generate_magic_link_token();
			// 32 bytes -> 64 hex chars
			assert_eq!(token.len(), MAGIC_LINK_TOKEN_BYTES * 2);
		}

		#[test]
		fn generates_hex_token() {
			let (token, _hash) = generate_magic_link_token();
			assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_tokens() {
			let tokens: HashSet<_> = (0..100).map(|_| generate_magic_link_token().0).collect();
			assert_eq!(tokens.len(), 100, "All tokens should be unique");
		}

		#[test]
		fn generated_token_verifies_against_hash() {
			let (token, hash) = generate_magic_link_token();
			assert!(verify_magic_link_token(&token, &hash));
		}
	}

	mod hash_verification {
		use super::*;

		#[test]
		fn hash_produces_argon2_format() {
			let hash = hash_magic_link_token("test_token");
			assert!(hash.starts_with("$argon2"));
		}

		#[test]
		fn same_token_produces_different_hashes() {
			let hash1 = hash_magic_link_token("test_token");
			let hash2 = hash_magic_link_token("test_token");
			assert_ne!(
				hash1, hash2,
				"Different salts should produce different hashes"
			);
		}

		#[test]
		fn correct_token_verifies() {
			let token = "test_magic_link_token_12345";
			let hash = hash_magic_link_token(token);
			assert!(verify_magic_link_token(token, &hash));
		}

		#[test]
		fn wrong_token_fails_verification() {
			let token = "correct_token";
			let hash = hash_magic_link_token(token);
			assert!(!verify_magic_link_token("wrong_token", &hash));
		}

		#[test]
		fn invalid_hash_fails_verification() {
			assert!(!verify_magic_link_token("any_token", "invalid_hash"));
		}
	}

	mod magic_link_struct {
		use super::*;

		#[test]
		fn new_creates_valid_link() {
			let (link, token) = MagicLink::new("test@example.com");

			assert_eq!(link.email, "test@example.com");
			assert!(link.is_valid());
			assert!(!link.is_expired());
			assert!(!link.is_used());
			assert!(!token.is_empty());
		}

		#[test]
		fn new_sets_correct_expiry() {
			let (link, _) = MagicLink::new("test@example.com");
			let expected_expiry = link.created_at + Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES);
			assert_eq!(link.expires_at, expected_expiry);
		}

		#[test]
		fn verify_works_with_correct_token() {
			let (link, token) = MagicLink::new("test@example.com");
			assert!(link.verify(&token));
		}

		#[test]
		fn verify_fails_with_wrong_token() {
			let (link, _) = MagicLink::new("test@example.com");
			assert!(!link.verify("wrong_token"));
		}

		#[test]
		fn mark_used_sets_timestamp() {
			let (mut link, _) = MagicLink::new("test@example.com");
			assert!(link.used_at.is_none());

			link.mark_used();

			assert!(link.used_at.is_some());
			assert!(link.is_used());
		}

		#[test]
		fn is_valid_returns_false_after_used() {
			let (mut link, _) = MagicLink::new("test@example.com");
			assert!(link.is_valid());

			link.mark_used();

			assert!(!link.is_valid());
		}
	}

	mod expiry_checks {
		use super::*;

		#[test]
		fn is_expired_returns_false_for_fresh_link() {
			let (link, _) = MagicLink::new("test@example.com");
			assert!(!link.is_expired());
		}

		#[test]
		fn is_expired_returns_true_for_past_expiry() {
			let (token, hash) = generate_magic_link_token();
			let past = Utc::now() - Duration::hours(1);
			let link = MagicLink {
				id: Uuid::new_v4(),
				email: "test@example.com".to_string(),
				token_hash: hash,
				created_at: past - Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES),
				expires_at: past,
				used_at: None,
			};

			assert!(link.is_expired());
			assert!(!link.is_valid());
			// Token still verifies even if expired
			assert!(link.verify(&token));
		}

		#[test]
		fn is_valid_returns_false_when_expired() {
			let (_, hash) = generate_magic_link_token();
			let past = Utc::now() - Duration::hours(1);
			let link = MagicLink {
				id: Uuid::new_v4(),
				email: "test@example.com".to_string(),
				token_hash: hash,
				created_at: past - Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES),
				expires_at: past,
				used_at: None,
			};

			assert!(!link.is_valid());
		}

		#[test]
		fn is_valid_returns_false_when_used_even_if_not_expired() {
			let (_, hash) = generate_magic_link_token();
			let now = Utc::now();
			let link = MagicLink {
				id: Uuid::new_v4(),
				email: "test@example.com".to_string(),
				token_hash: hash,
				created_at: now,
				expires_at: now + Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES),
				used_at: Some(now),
			};

			assert!(!link.is_valid());
			assert!(link.is_used());
			assert!(!link.is_expired());
		}
	}

	mod proptest_tests {
		use super::*;
		use proptest::prelude::*;

		proptest! {
				/// Property: Token generation always produces unique tokens.
				///
				/// While we can't prove uniqueness across all possible runs, we can verify
				/// that any two generated tokens are different with overwhelming probability.
				#[test]
				fn token_generation_produces_unique_tokens(_ in 0..100u32) {
						let (token1, _) = generate_magic_link_token();
						let (token2, _) = generate_magic_link_token();
						prop_assert_ne!(token1, token2, "Two generated tokens should never be equal");
				}

				/// Property: Token hashing is deterministic for verification purposes.
				///
				/// While the hash output differs each time (due to random salt), verifying
				/// the same token against its hash always succeeds.
				#[test]
				fn token_verification_is_deterministic(token in "[a-f0-9]{64}") {
						let hash = hash_magic_link_token(&token);
						// Verification should always succeed for the correct token
						prop_assert!(verify_magic_link_token(&token, &hash));
						// And should be repeatable
						prop_assert!(verify_magic_link_token(&token, &hash));
				}

				/// Property: Wrong tokens always fail verification.
				#[test]
				fn wrong_token_always_fails(
						token in "[a-f0-9]{64}",
						wrong_token in "[a-f0-9]{64}"
				) {
						prop_assume!(token != wrong_token);
						let hash = hash_magic_link_token(&token);
						prop_assert!(!verify_magic_link_token(&wrong_token, &hash));
				}

				/// Property: Generated tokens have correct format (hex, correct length).
				#[test]
				fn generated_tokens_have_correct_format(_ in 0..50u32) {
						let (token, _hash) = generate_magic_link_token();
						prop_assert_eq!(token.len(), MAGIC_LINK_TOKEN_BYTES * 2);
						prop_assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
				}

				/// Property: Fresh magic links are always valid.
				#[test]
				fn fresh_magic_links_are_valid(email in "[a-z]{5,10}@example\\.com") {
						let (link, _token) = MagicLink::new(&email);
						prop_assert!(link.is_valid());
						prop_assert!(!link.is_expired());
						prop_assert!(!link.is_used());
				}

				/// Property: Expiry is always MAGIC_LINK_EXPIRY_MINUTES after creation.
				#[test]
				fn expiry_is_correct(email in "[a-z]{5,10}@example\\.com") {
						let (link, _) = MagicLink::new(&email);
						let expected = link.created_at + Duration::minutes(MAGIC_LINK_EXPIRY_MINUTES);
						prop_assert_eq!(link.expires_at, expected);
				}

				/// Property: Used links are never valid.
				#[test]
				fn used_links_are_never_valid(email in "[a-z]{5,10}@example\\.com") {
						let (mut link, _) = MagicLink::new(&email);
						link.mark_used();
						prop_assert!(!link.is_valid());
						prop_assert!(link.is_used());
				}

				/// Property: Tokens generated with MagicLink::new always verify.
				#[test]
				fn magic_link_token_always_verifies(email in "[a-z]{5,10}@example\\.com") {
						let (link, token) = MagicLink::new(&email);
						prop_assert!(link.verify(&token));
				}
		}
	}
}
