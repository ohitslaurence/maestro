// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Webhook signature verification for GitHub App webhooks.

use tracing::{debug, warn};

use crate::error::GithubAppError;

/// Verify a GitHub webhook signature.
///
/// GitHub webhooks include an `X-Hub-Signature-256` header containing an
/// HMAC-SHA256 signature of the request body, using the webhook secret as the
/// key.
///
/// # Arguments
///
/// * `secret` - The webhook secret configured in GitHub App settings
/// * `signature_header` - The value of the `X-Hub-Signature-256` header
///   (format: `sha256=<hex>`)
/// * `body` - The raw request body bytes
///
/// # Returns
///
/// `Ok(())` if the signature is valid,
/// `Err(GithubAppError::InvalidWebhookSignature)` otherwise
///
/// # Example
///
/// ```rust,ignore
/// use loom_server_github_app::verify_webhook_signature;
///
/// let secret = "my-webhook-secret";
/// let signature = "sha256=abc123...";
/// let body = b"{\"action\": \"created\", ...}";
///
/// verify_webhook_signature(secret, signature, body)?;
/// ```
pub fn verify_webhook_signature(
	secret: &str,
	signature_header: &str,
	body: &[u8],
) -> Result<(), GithubAppError> {
	const PREFIX: &str = "sha256=";

	if !signature_header.starts_with(PREFIX) {
		warn!("Invalid webhook signature format: missing 'sha256=' prefix");
		return Err(GithubAppError::InvalidWebhookSignature);
	}

	let expected_hex = &signature_header[PREFIX.len()..];

	if loom_common_webhook::verify_hmac_sha256(secret.as_bytes(), body, expected_hex) {
		debug!("Webhook signature verified successfully");
		Ok(())
	} else {
		warn!("Webhook signature verification failed");
		Err(GithubAppError::InvalidWebhookSignature)
	}
}

/// Compute the HMAC-SHA256 signature for a webhook payload.
///
/// This is useful for testing webhook signature verification.
///
/// # Arguments
///
/// * `secret` - The webhook secret
/// * `body` - The request body bytes
///
/// # Returns
///
/// The signature in the format `sha256=<hex>`
pub fn compute_webhook_signature(secret: &str, body: &[u8]) -> String {
	let signature = loom_common_webhook::compute_hmac_sha256(secret.as_bytes(), body);
	format!("sha256={}", signature)
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	const TEST_SECRET: &str = "test-webhook-secret";
	const TEST_BODY: &[u8] = b"{\"action\": \"created\"}";

	#[test]
	fn test_verify_valid_signature() {
		let signature = compute_webhook_signature(TEST_SECRET, TEST_BODY);
		let result = verify_webhook_signature(TEST_SECRET, &signature, TEST_BODY);
		assert!(result.is_ok());
	}

	#[test]
	fn test_verify_invalid_signature() {
		let signature = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
		let result = verify_webhook_signature(TEST_SECRET, signature, TEST_BODY);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			GithubAppError::InvalidWebhookSignature
		));
	}

	#[test]
	fn test_verify_wrong_prefix() {
		let result = verify_webhook_signature(TEST_SECRET, "sha1=abc123", TEST_BODY);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			GithubAppError::InvalidWebhookSignature
		));
	}

	#[test]
	fn test_verify_invalid_hex() {
		let result = verify_webhook_signature(TEST_SECRET, "sha256=not-valid-hex", TEST_BODY);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			GithubAppError::InvalidWebhookSignature
		));
	}

	#[test]
	fn test_verify_tampered_body() {
		let signature = compute_webhook_signature(TEST_SECRET, TEST_BODY);
		let tampered_body = b"{\"action\": \"deleted\"}";
		let result = verify_webhook_signature(TEST_SECRET, &signature, tampered_body);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			GithubAppError::InvalidWebhookSignature
		));
	}

	#[test]
	fn test_verify_wrong_secret() {
		let signature = compute_webhook_signature(TEST_SECRET, TEST_BODY);
		let result = verify_webhook_signature("wrong-secret", &signature, TEST_BODY);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			GithubAppError::InvalidWebhookSignature
		));
	}

	#[test]
	fn test_compute_signature_format() {
		let signature = compute_webhook_signature(TEST_SECRET, TEST_BODY);
		assert!(signature.starts_with("sha256="));
		assert_eq!(signature.len(), "sha256=".len() + 64);
	}

	proptest! {
			/// **Property: Valid signatures always verify successfully**
			///
			/// Why: Ensures HMAC roundtrip is correct for any secret/body combination.
			#[test]
			fn prop_valid_signature_always_verifies(
					secret in "[a-zA-Z0-9]{8,64}",
					body in proptest::collection::vec(proptest::num::u8::ANY, 1..1000)
			) {
					let signature = compute_webhook_signature(&secret, &body);
					let result = verify_webhook_signature(&secret, &signature, &body);
					prop_assert!(result.is_ok(), "Valid signature should verify: {:?}", result);
			}

			/// **Property: Tampered payloads always fail verification**
			///
			/// Why: Security critical - ensures webhook verification detects any modification.
			#[test]
			fn prop_tampered_body_fails_verification(
					secret in "[a-zA-Z0-9]{8,64}",
					body in proptest::collection::vec(proptest::num::u8::ANY, 2..500),
					tamper_index in 0usize..500usize
			) {
					let signature = compute_webhook_signature(&secret, &body);

					// Create tampered body by flipping a byte
					let mut tampered = body.clone();
					let idx = tamper_index % tampered.len();
					tampered[idx] = tampered[idx].wrapping_add(1);

					// If the body changed, verification should fail
					if tampered != body {
							let result = verify_webhook_signature(&secret, &signature, &tampered);
							prop_assert!(result.is_err(), "Tampered body should fail verification");
					}
			}

			/// **Property: Wrong secrets always fail verification**
			///
			/// Why: Ensures different secrets produce different signatures.
			#[test]
			fn prop_wrong_secret_fails_verification(
					secret1 in "[a-zA-Z0-9]{8,64}",
					secret2 in "[a-zA-Z0-9]{8,64}",
					body in proptest::collection::vec(proptest::num::u8::ANY, 1..500)
			) {
					if secret1 != secret2 {
							let signature = compute_webhook_signature(&secret1, &body);
							let result = verify_webhook_signature(&secret2, &signature, &body);
							prop_assert!(result.is_err(), "Wrong secret should fail verification");
					}
			}

			/// **Property: Signature format is always sha256= followed by 64 hex chars**
			///
			/// Why: Ensures signature output format matches GitHub's expected format.
			#[test]
			fn prop_signature_format_is_correct(
					secret in "[a-zA-Z0-9]{1,100}",
					body in proptest::collection::vec(proptest::num::u8::ANY, 0..1000)
			) {
					let signature = compute_webhook_signature(&secret, &body);
					prop_assert!(signature.starts_with("sha256="));
					prop_assert_eq!(signature.len(), "sha256=".len() + 64);

					// Verify all characters after prefix are hex
					let hex_part = &signature["sha256=".len()..];
					prop_assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
			}
	}
}
