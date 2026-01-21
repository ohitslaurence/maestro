// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Shared HMAC-SHA256 webhook signature utilities.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute an HMAC-SHA256 signature for a payload.
///
/// Returns the hex-encoded signature without any prefix.
pub fn compute_hmac_sha256(secret: &[u8], payload: &[u8]) -> String {
	let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
	mac.update(payload);
	let result = mac.finalize();
	hex::encode(result.into_bytes())
}

/// Verify an HMAC-SHA256 signature for a payload.
///
/// The `signature` should be the raw hex-encoded signature (no prefix).
pub fn verify_hmac_sha256(secret: &[u8], payload: &[u8], signature: &str) -> bool {
	let expected_bytes = match hex::decode(signature) {
		Ok(bytes) => bytes,
		Err(_) => return false,
	};

	let mut mac = match HmacSha256::new_from_slice(secret) {
		Ok(m) => m,
		Err(_) => return false,
	};

	mac.update(payload);
	mac.verify_slice(&expected_bytes).is_ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_compute_hmac_sha256() {
		let secret = b"test-secret";
		let payload = b"test payload";
		let sig = compute_hmac_sha256(secret, payload);
		assert!(!sig.is_empty());
		assert_eq!(sig.len(), 64);
	}

	#[test]
	fn test_verify_hmac_sha256_valid() {
		let secret = b"test-secret";
		let payload = b"test payload";
		let sig = compute_hmac_sha256(secret, payload);
		assert!(verify_hmac_sha256(secret, payload, &sig));
	}

	#[test]
	fn test_verify_hmac_sha256_invalid_signature() {
		let secret = b"test-secret";
		let payload = b"test payload";
		let invalid_sig = "0".repeat(64);
		assert!(!verify_hmac_sha256(secret, payload, &invalid_sig));
	}

	#[test]
	fn test_verify_hmac_sha256_invalid_hex() {
		let secret = b"test-secret";
		let payload = b"test payload";
		assert!(!verify_hmac_sha256(secret, payload, "not-valid-hex"));
	}

	#[test]
	fn test_verify_hmac_sha256_wrong_secret() {
		let secret = b"test-secret";
		let payload = b"test payload";
		let sig = compute_hmac_sha256(secret, payload);
		assert!(!verify_hmac_sha256(b"wrong-secret", payload, &sig));
	}

	#[test]
	fn test_verify_hmac_sha256_tampered_payload() {
		let secret = b"test-secret";
		let payload = b"test payload";
		let sig = compute_hmac_sha256(secret, payload);
		assert!(!verify_hmac_sha256(secret, b"tampered payload", &sig));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn prop_roundtrip(
			secret in proptest::collection::vec(proptest::num::u8::ANY, 1..100),
			payload in proptest::collection::vec(proptest::num::u8::ANY, 0..1000)
		) {
			let sig = compute_hmac_sha256(&secret, &payload);
			prop_assert!(verify_hmac_sha256(&secret, &payload, &sig));
		}

		#[test]
		fn prop_signature_is_64_hex_chars(
			secret in proptest::collection::vec(proptest::num::u8::ANY, 1..100),
			payload in proptest::collection::vec(proptest::num::u8::ANY, 0..1000)
		) {
			let sig = compute_hmac_sha256(&secret, &payload);
			prop_assert_eq!(sig.len(), 64);
			prop_assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn prop_wrong_secret_fails(
			secret1 in proptest::collection::vec(proptest::num::u8::ANY, 1..100),
			secret2 in proptest::collection::vec(proptest::num::u8::ANY, 1..100),
			payload in proptest::collection::vec(proptest::num::u8::ANY, 1..500)
		) {
			if secret1 != secret2 {
				let sig = compute_hmac_sha256(&secret1, &payload);
				prop_assert!(!verify_hmac_sha256(&secret2, &payload, &sig));
			}
		}
	}
}
