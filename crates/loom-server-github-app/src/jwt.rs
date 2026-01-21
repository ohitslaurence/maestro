// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! JWT generation utilities for GitHub App authentication.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

#[cfg(test)]
pub(crate) use claims_for_testing::Claims as TestClaims;

#[cfg(test)]
mod claims_for_testing {
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Serialize, Deserialize)]
	pub struct Claims {
		pub iat: u64,
		pub exp: u64,
		pub iss: String,
	}
}
use tracing::{debug, instrument};

use crate::error::GithubAppError;

/// JWT claims for GitHub App authentication.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
	/// Issued at timestamp (seconds since epoch).
	iat: u64,
	/// Expiration timestamp (seconds since epoch).
	exp: u64,
	/// Issuer (GitHub App ID).
	iss: String,
}

/// Generate a JWT for GitHub App authentication.
///
/// The JWT is signed with the app's private RSA key and is valid for up to 10
/// minutes. We use a 9-minute expiry to provide some buffer before the GitHub
/// maximum.
///
/// # Arguments
///
/// * `app_id` - The GitHub App numeric ID
/// * `private_key_pem` - PEM-encoded RSA private key
///
/// # Returns
///
/// A signed JWT string
#[instrument(skip(private_key_pem))]
pub fn generate_app_jwt(app_id: u64, private_key_pem: &str) -> Result<String, GithubAppError> {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_err(|e| GithubAppError::Jwt(format!("System time error: {e}")))?;

	let iat = now.as_secs().saturating_sub(60);
	let exp = now.as_secs() + Duration::from_secs(9 * 60).as_secs();

	let claims = Claims {
		iat,
		exp,
		iss: app_id.to_string(),
	};

	let encoding_key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
		.map_err(|e| GithubAppError::Jwt(format!("Invalid RSA private key: {e}")))?;

	let header = Header::new(Algorithm::RS256);

	let token = encode(&header, &claims, &encoding_key)
		.map_err(|e| GithubAppError::Jwt(format!("Failed to encode JWT: {e}")))?;

	debug!(app_id = app_id, exp = exp, "Generated GitHub App JWT");

	Ok(token)
}

/// Calculate when a JWT with the given expiry should be refreshed.
///
/// Returns the duration until the token should be refreshed (30 seconds before
/// expiry).
pub fn jwt_refresh_duration(expires_at_secs: u64) -> Duration {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs();

	let refresh_at = expires_at_secs.saturating_sub(30);

	if now >= refresh_at {
		Duration::ZERO
	} else {
		Duration::from_secs(refresh_at - now)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_generate_jwt_with_invalid_key() {
		let result = generate_app_jwt(12345, "not-a-valid-key");
		assert!(result.is_err(), "Should fail with invalid key");

		let err = result.unwrap_err();
		assert!(matches!(err, GithubAppError::Jwt(_)));
	}

	#[test]
	fn test_generate_jwt_with_invalid_pem_format() {
		let result = generate_app_jwt(
			12345,
			"-----BEGIN RSA PRIVATE KEY-----\ninvalid\n-----END RSA PRIVATE KEY-----",
		);
		assert!(result.is_err(), "Should fail with malformed PEM");
	}

	#[test]
	fn test_jwt_refresh_duration_not_yet_expired() {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let expires_at = now + 300;

		let duration = jwt_refresh_duration(expires_at);
		assert!(duration > Duration::from_secs(200));
	}

	#[test]
	fn test_jwt_refresh_duration_should_refresh() {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let expires_at = now + 20;

		let duration = jwt_refresh_duration(expires_at);
		assert_eq!(duration, Duration::ZERO);
	}

	#[test]
	fn test_jwt_refresh_duration_already_expired() {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let expires_at = now.saturating_sub(60);

		let duration = jwt_refresh_duration(expires_at);
		assert_eq!(duration, Duration::ZERO);
	}

	#[test]
	fn test_jwt_claims_are_valid() {
		use super::TestClaims;
		use jsonwebtoken::{decode, DecodingKey, Validation};
		use rsa::{pkcs1::EncodeRsaPublicKey, pkcs8::EncodePrivateKey, RsaPrivateKey};

		let app_id = 12345u64;

		// Generate a real RSA key pair for testing
		let mut rng = rand::thread_rng();
		let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("Failed to generate RSA key");
		let public_key = private_key.to_public_key();

		let private_key_pem = private_key
			.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
			.expect("Failed to convert private key to PEM");
		let public_key_pem = public_key
			.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
			.expect("Failed to convert public key to PEM");

		let token = generate_app_jwt(app_id, &private_key_pem).expect("Failed to generate JWT");

		// Decode and verify claims
		let mut validation = Validation::new(Algorithm::RS256);
		validation.validate_exp = false; // We'll check manually
		validation.required_spec_claims.clear();

		let decoding_key =
			DecodingKey::from_rsa_pem(public_key_pem.as_bytes()).expect("Failed to create decoding key");

		let token_data =
			decode::<TestClaims>(&token, &decoding_key, &validation).expect("Failed to decode JWT");

		let claims = token_data.claims;

		// Verify issuer matches app_id
		assert_eq!(claims.iss, app_id.to_string());

		// Verify exp > iat
		assert!(
			claims.exp > claims.iat,
			"exp ({}) must be > iat ({})",
			claims.exp,
			claims.iat
		);

		// Verify lifetime <= 10 minutes (GitHub max)
		let lifetime = claims.exp - claims.iat;
		assert!(
			lifetime <= 10 * 60,
			"JWT lifetime ({lifetime} seconds) exceeds GitHub maximum of 10 minutes"
		);

		// Verify token is not already expired (with small grace for test execution)
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		assert!(
			claims.exp > now.saturating_sub(5),
			"Token is already expired"
		);

		// Verify iat is in the past (we subtract 60s for clock skew)
		assert!(
			claims.iat <= now + 5,
			"iat should be <= now (with small margin)"
		);
	}
}
