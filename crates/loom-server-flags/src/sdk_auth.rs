// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use argon2::{
	password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
	Argon2,
};

use crate::error::{FlagsServerError, Result};

/// Hashes an SDK key using Argon2.
pub fn hash_sdk_key(key: &str) -> Result<String> {
	let salt = SaltString::generate(&mut OsRng);
	let argon2 = Argon2::default();

	argon2
		.hash_password(key.as_bytes(), &salt)
		.map(|hash| hash.to_string())
		.map_err(|_| FlagsServerError::Internal("Failed to hash SDK key".to_string()))
}

/// Verifies an SDK key against a stored hash.
pub fn verify_sdk_key(key: &str, hash: &str) -> Result<bool> {
	let parsed_hash = PasswordHash::new(hash)
		.map_err(|_| FlagsServerError::Internal("Invalid SDK key hash format".to_string()))?;

	Ok(
		Argon2::default()
			.verify_password(key.as_bytes(), &parsed_hash)
			.is_ok(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_hash_and_verify() {
		let key = "loom_sdk_server_prod_abc123def456";

		let hash = hash_sdk_key(key).unwrap();
		assert!(hash.starts_with("$argon2"));

		assert!(verify_sdk_key(key, &hash).unwrap());
		assert!(!verify_sdk_key("wrong_key", &hash).unwrap());
	}

	#[test]
	fn test_different_hashes_for_same_key() {
		let key = "loom_sdk_server_prod_abc123def456";

		let hash1 = hash_sdk_key(key).unwrap();
		let hash2 = hash_sdk_key(key).unwrap();

		// Hashes should be different due to random salt
		assert_ne!(hash1, hash2);

		// But both should verify
		assert!(verify_sdk_key(key, &hash1).unwrap());
		assert!(verify_sdk_key(key, &hash2).unwrap());
	}
}
