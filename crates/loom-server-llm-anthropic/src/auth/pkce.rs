// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! PKCE (Proof Key for Code Exchange) implementation for OAuth 2.0.
//!
//! Implements RFC 7636 with S256 challenge method.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

/// PKCE (Proof Key for Code Exchange) values for OAuth 2.0.
#[derive(Debug, Clone)]
pub struct Pkce {
	/// The code verifier (random string, 43-128 characters).
	pub verifier: String,
	/// The code challenge (SHA256 hash of verifier, base64url encoded).
	pub challenge: String,
}

impl Pkce {
	/// Generate a new PKCE pair with cryptographically secure random bytes.
	pub fn generate() -> Self {
		let mut verifier_bytes = [0u8; 32];
		getrandom::getrandom(&mut verifier_bytes).expect("Failed to generate random bytes");
		let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

		let mut hasher = Sha256::new();
		hasher.update(verifier.as_bytes());
		let hash = hasher.finalize();
		let challenge = URL_SAFE_NO_PAD.encode(hash);

		Self {
			verifier,
			challenge,
		}
	}

	/// Create a PKCE pair from an existing verifier (for testing or reconstruction).
	pub fn from_verifier(verifier: impl Into<String>) -> Self {
		let verifier = verifier.into();
		let mut hasher = Sha256::new();
		hasher.update(verifier.as_bytes());
		let hash = hasher.finalize();
		let challenge = URL_SAFE_NO_PAD.encode(hash);

		Self {
			verifier,
			challenge,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_pkce_generation() {
		let pkce = Pkce::generate();

		assert!(!pkce.verifier.is_empty());
		assert!(!pkce.challenge.is_empty());
		assert_ne!(pkce.verifier, pkce.challenge);
	}

	#[test]
	fn test_pkce_verifier_length() {
		let pkce = Pkce::generate();
		assert_eq!(pkce.verifier.len(), 43);
	}

	#[test]
	fn test_pkce_challenge_is_deterministic() {
		let verifier = "test_verifier_string_12345";
		let pkce1 = Pkce::from_verifier(verifier);
		let pkce2 = Pkce::from_verifier(verifier);

		assert_eq!(pkce1.challenge, pkce2.challenge);
	}

	#[test]
	fn test_pkce_uniqueness() {
		let pkce1 = Pkce::generate();
		let pkce2 = Pkce::generate();

		assert_ne!(pkce1.verifier, pkce2.verifier);
		assert_ne!(pkce1.challenge, pkce2.challenge);
	}

	#[test]
	fn test_pkce_challenge_is_base64url() {
		let pkce = Pkce::generate();
		assert!(!pkce.challenge.contains('+'));
		assert!(!pkce.challenge.contains('/'));
		assert!(!pkce.challenge.contains('='));
	}
}
