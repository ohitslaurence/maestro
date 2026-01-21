// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Key backend abstraction for cryptographic operations.
//!
//! Provides a trait-based abstraction that allows swapping between:
//! - Software keys (development/simple deployments)
//! - HSM/KMS backends (production with hardware security)

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as BASE64URL, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use loom_common_secret::SecretString;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::encryption::{self, EncryptedData, KEY_SIZE};
use crate::error::{SecretsError, SecretsResult};
use crate::svid::WeaverClaims;

/// JSON Web Key Set for public key distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWebKeySet {
	pub keys: Vec<JsonWebKey>,
}

/// A single JSON Web Key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWebKey {
	pub kty: String,
	pub kid: String,
	pub alg: String,
	#[serde(rename = "use")]
	pub key_use: String,
	/// Base64url-encoded public key (for OKP/Ed25519)
	pub x: String,
	pub crv: String,
}

/// Encrypted Data Encryption Key with metadata.
#[derive(Debug, Clone)]
pub struct EncryptedDekData {
	pub id: String,
	pub encrypted_key: Vec<u8>,
	pub nonce: [u8; 12],
	pub kek_version: u32,
}

/// Trait for key management operations.
///
/// This abstraction allows swapping between software keys and HSM/KMS
/// without changing the rest of the codebase.
#[async_trait]
pub trait KeyBackend: Send + Sync {
	/// Encrypt a DEK under the master key.
	async fn encrypt_dek(&self, dek: &[u8; KEY_SIZE]) -> SecretsResult<EncryptedDekData>;

	/// Decrypt a DEK using the master key.
	async fn decrypt_dek(
		&self,
		encrypted: &EncryptedDekData,
	) -> SecretsResult<Zeroizing<[u8; KEY_SIZE]>>;

	/// Sign a Weaver SVID JWT.
	async fn sign_weaver_svid(&self, claims: &WeaverClaims) -> SecretsResult<String>;

	/// Verify a Weaver SVID JWT and extract claims.
	async fn verify_weaver_svid(&self, token: &str) -> SecretsResult<WeaverClaims>;

	/// Get the key ID used for SVID signing.
	fn svid_signing_key_id(&self) -> &str;

	/// Get the JWKS for public key distribution.
	fn jwks(&self) -> JsonWebKeySet;

	/// Get the current KEK version.
	fn kek_version(&self) -> u32;
}

/// Clock skew tolerance in seconds for JWT validation.
const CLOCK_SKEW_SECONDS: i64 = 30;

/// Software-based key backend using in-memory keys.
///
/// Suitable for development and simple deployments.
/// For production with compliance requirements, use HSM backend.
pub struct SoftwareKeyBackend {
	/// Master key for envelope encryption (KEK).
	kek: Zeroizing<[u8; KEY_SIZE]>,
	/// KEK version for key rotation tracking.
	kek_version: u32,
	/// Ed25519 signing key for SVIDs.
	///
	/// Note: SigningKey does not implement Zeroize. The key material is held in memory
	/// for the lifetime of this struct. For production deployments with strict security
	/// requirements, consider using an HSM backend instead.
	svid_signing_key: SigningKey,
	/// Key ID for SVID signing.
	svid_key_id: String,
	/// SVID issuer name.
	svid_issuer: String,
	/// SVID audience.
	svid_audience: String,
}

impl SoftwareKeyBackend {
	/// Create a new software key backend.
	pub fn new(
		kek: Zeroizing<[u8; KEY_SIZE]>,
		svid_signing_key: Option<SigningKey>,
		svid_issuer: String,
		svid_audience: String,
	) -> Self {
		let svid_signing_key = svid_signing_key.unwrap_or_else(|| SigningKey::generate(&mut OsRng));
		let svid_key_id = format!(
			"loom-svid-{}",
			hex::encode(&svid_signing_key.verifying_key().to_bytes()[..8])
		);

		Self {
			kek,
			kek_version: 1,
			svid_signing_key,
			svid_key_id,
			svid_issuer,
			svid_audience,
		}
	}

	/// Create from base64-encoded keys.
	pub fn from_base64(
		kek_base64: &SecretString,
		svid_key_base64: Option<&SecretString>,
		svid_issuer: String,
		svid_audience: String,
	) -> SecretsResult<Self> {
		use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

		let kek_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
			BASE64
				.decode(kek_base64.expose().as_bytes())
				.map_err(|e| SecretsError::Configuration(format!("invalid KEK base64: {e}")))?,
		);

		if kek_bytes.len() != KEY_SIZE {
			return Err(SecretsError::Configuration(format!(
				"KEK must be {} bytes, got {}",
				KEY_SIZE,
				kek_bytes.len()
			)));
		}

		let mut kek = Zeroizing::new([0u8; KEY_SIZE]);
		kek.copy_from_slice(&kek_bytes);

		let svid_signing_key = if let Some(key_b64) = svid_key_base64 {
			let key_bytes: Zeroizing<Vec<u8>> = Zeroizing::new(
				BASE64
					.decode(key_b64.expose().as_bytes())
					.map_err(|e| SecretsError::Configuration(format!("invalid SVID key base64: {e}")))?,
			);

			if key_bytes.len() != 32 {
				return Err(SecretsError::Configuration(format!(
					"SVID signing key must be 32 bytes, got {}",
					key_bytes.len()
				)));
			}

			let mut key_array: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
			key_array.copy_from_slice(&key_bytes);
			Some(SigningKey::from_bytes(&key_array))
		} else {
			None
		};

		Ok(Self::new(kek, svid_signing_key, svid_issuer, svid_audience))
	}

	/// Get the verifying (public) key for SVID verification.
	pub fn svid_verifying_key(&self) -> VerifyingKey {
		self.svid_signing_key.verifying_key()
	}

	/// Encode JWT header and claims into signing input.
	fn encode_jwt_payload(&self, claims: &WeaverClaims) -> SecretsResult<String> {
		#[derive(Serialize)]
		struct JwtHeader<'a> {
			alg: &'a str,
			typ: &'a str,
			kid: &'a str,
		}

		let header = JwtHeader {
			alg: "EdDSA",
			typ: "JWT",
			kid: &self.svid_key_id,
		};

		let header_json = serde_json::to_vec(&header)
			.map_err(|e| SecretsError::SvidSigning(format!("header encoding failed: {e}")))?;
		let claims_json = serde_json::to_vec(claims)
			.map_err(|e| SecretsError::SvidSigning(format!("claims encoding failed: {e}")))?;

		let header_b64 = BASE64URL.encode(&header_json);
		let claims_b64 = BASE64URL.encode(&claims_json);

		Ok(format!("{}.{}", header_b64, claims_b64))
	}

	/// Parse JWT token into parts and validate structure.
	fn parse_jwt_parts(token: &str) -> SecretsResult<(&str, &str, &str)> {
		let parts: Vec<&str> = token.split('.').collect();
		if parts.len() != 3 {
			return Err(SecretsError::SvidValidation("invalid JWT structure".into()));
		}
		Ok((parts[0], parts[1], parts[2]))
	}
}

#[async_trait]
impl KeyBackend for SoftwareKeyBackend {
	async fn encrypt_dek(&self, dek: &[u8; KEY_SIZE]) -> SecretsResult<EncryptedDekData> {
		let encrypted = encryption::encrypt_dek(&self.kek, dek)?;

		Ok(EncryptedDekData {
			id: uuid::Uuid::new_v4().to_string(),
			encrypted_key: encrypted.ciphertext,
			nonce: encrypted.nonce,
			kek_version: self.kek_version,
		})
	}

	async fn decrypt_dek(
		&self,
		encrypted: &EncryptedDekData,
	) -> SecretsResult<Zeroizing<[u8; KEY_SIZE]>> {
		if encrypted.kek_version != self.kek_version {
			return Err(SecretsError::KeyVersionMismatch {
				expected: self.kek_version,
				actual: encrypted.kek_version,
			});
		}

		let enc_data = EncryptedData {
			ciphertext: encrypted.encrypted_key.clone(),
			nonce: encrypted.nonce,
		};

		encryption::decrypt_dek(&self.kek, &enc_data)
	}

	async fn sign_weaver_svid(&self, claims: &WeaverClaims) -> SecretsResult<String> {
		let signing_input = self.encode_jwt_payload(claims)?;
		let signature = self.svid_signing_key.sign(signing_input.as_bytes());
		let signature_b64 = BASE64URL.encode(signature.to_bytes());

		Ok(format!("{}.{}", signing_input, signature_b64))
	}

	async fn verify_weaver_svid(&self, token: &str) -> SecretsResult<WeaverClaims> {
		let (header_b64, claims_b64, sig_b64) = Self::parse_jwt_parts(token)?;

		// Decode and validate header (alg and kid)
		let header_json = BASE64URL
			.decode(header_b64)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid header encoding: {e}")))?;

		#[derive(Deserialize)]
		struct JwtHeader {
			alg: String,
			kid: Option<String>,
		}

		let header: JwtHeader = serde_json::from_slice(&header_json)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid header JSON: {e}")))?;

		// Validate algorithm - only accept EdDSA
		if header.alg != "EdDSA" {
			return Err(SecretsError::SvidValidation(format!(
				"unsupported algorithm: expected EdDSA, got {}",
				header.alg
			)));
		}

		// Validate typ if present - must be JWT
		#[derive(Deserialize)]
		struct JwtHeaderTyp {
			typ: Option<String>,
		}
		if let Ok(typ_header) = serde_json::from_slice::<JwtHeaderTyp>(&header_json) {
			if let Some(ref typ) = typ_header.typ {
				if typ != "JWT" {
					return Err(SecretsError::SvidValidation(format!(
						"unexpected typ: expected JWT, got {}",
						typ
					)));
				}
			}
		}

		// Validate kid matches our expected key ID
		match &header.kid {
			Some(kid) if kid == &self.svid_key_id => {}
			Some(_) => {
				return Err(SecretsError::SvidValidation(
					"token signed by unknown key".into(),
				));
			}
			None => {
				return Err(SecretsError::SvidValidation(
					"token missing kid header".into(),
				));
			}
		}

		// Verify signature
		let signing_input = format!("{}.{}", header_b64, claims_b64);
		let sig_bytes = BASE64URL
			.decode(sig_b64)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid signature encoding: {e}")))?;

		let signature = Signature::from_slice(&sig_bytes)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid signature format: {e}")))?;

		self
			.svid_signing_key
			.verifying_key()
			.verify(signing_input.as_bytes(), &signature)
			.map_err(|_| SecretsError::SvidInvalidSignature)?;

		// Decode claims
		let claims_json = BASE64URL
			.decode(claims_b64)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid claims encoding: {e}")))?;

		let claims: WeaverClaims = serde_json::from_slice(&claims_json)
			.map_err(|e| SecretsError::SvidValidation(format!("invalid claims JSON: {e}")))?;

		let now = chrono::Utc::now().timestamp();

		// Validate expiration
		if claims.exp <= now {
			return Err(SecretsError::SvidExpired);
		}

		// Validate nbf (not-before)
		if claims.nbf > now + CLOCK_SKEW_SECONDS {
			return Err(SecretsError::SvidNotYetValid);
		}

		// Validate issuer
		if claims.iss != self.svid_issuer {
			return Err(SecretsError::SvidInvalidIssuer);
		}

		// Validate audience
		if !claims.aud.iter().any(|a| a == &self.svid_audience) {
			return Err(SecretsError::SvidInvalidAudience);
		}

		// Validate iat is not in the future (with clock skew tolerance)
		if claims.iat > now + CLOCK_SKEW_SECONDS {
			return Err(SecretsError::SvidValidation("iat is in the future".into()));
		}

		Ok(claims)
	}

	fn svid_signing_key_id(&self) -> &str {
		&self.svid_key_id
	}

	fn jwks(&self) -> JsonWebKeySet {
		let public_key = self.svid_signing_key.verifying_key();
		let x = BASE64URL.encode(public_key.to_bytes());

		JsonWebKeySet {
			keys: vec![JsonWebKey {
				kty: "OKP".to_string(),
				kid: self.svid_key_id.clone(),
				alg: "EdDSA".to_string(),
				key_use: "sig".to_string(),
				x,
				crv: "Ed25519".to_string(),
			}],
		}
	}

	fn kek_version(&self) -> u32 {
		self.kek_version
	}
}

impl std::fmt::Debug for SoftwareKeyBackend {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SoftwareKeyBackend")
			.field("kek", &"[REDACTED]")
			.field("kek_version", &self.kek_version)
			.field("svid_key_id", &self.svid_key_id)
			.field("svid_issuer", &self.svid_issuer)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::encryption::generate_key;

	fn create_test_backend() -> SoftwareKeyBackend {
		let kek = generate_key();
		SoftwareKeyBackend::new(
			kek,
			None,
			"test-issuer".to_string(),
			"test-audience".to_string(),
		)
	}

	#[tokio::test]
	async fn dek_encryption_roundtrip() {
		let backend = create_test_backend();
		let dek = generate_key();

		let encrypted = backend.encrypt_dek(&dek).await.unwrap();
		let decrypted = backend.decrypt_dek(&encrypted).await.unwrap();

		assert_eq!(decrypted.as_slice(), dek.as_slice());
	}

	#[tokio::test]
	async fn svid_sign_verify_roundtrip() {
		let backend = create_test_backend();

		let now = chrono::Utc::now().timestamp();
		let claims = WeaverClaims {
			jti: "test-jti-123".to_string(),
			sub: "spiffe://loom.dev/weaver/test-123".to_string(),
			weaver_id: "test-123".to_string(),
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
			pod_uid: "pod-uid-123".to_string(),
			org_id: "org-123".to_string(),
			repo_id: Some("repo-123".to_string()),
			owner_user_id: "user-123".to_string(),
			iat: now,
			nbf: now,
			exp: now + 900,
			iss: "test-issuer".to_string(),
			aud: vec!["test-audience".to_string()],
		};

		let token = backend.sign_weaver_svid(&claims).await.unwrap();
		let verified = backend.verify_weaver_svid(&token).await.unwrap();

		assert_eq!(verified.weaver_id, claims.weaver_id);
		assert_eq!(verified.org_id, claims.org_id);
	}

	#[tokio::test]
	async fn expired_svid_rejected() {
		let backend = create_test_backend();
		let now = chrono::Utc::now().timestamp();

		let claims = WeaverClaims {
			jti: "test-jti-456".to_string(),
			sub: "spiffe://loom.dev/weaver/test-123".to_string(),
			weaver_id: "test-123".to_string(),
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
			pod_uid: "pod-uid-123".to_string(),
			org_id: "org-123".to_string(),
			repo_id: None,
			owner_user_id: "user-123".to_string(),
			iat: now - 1000,
			nbf: now - 1000,
			exp: now - 100,
			iss: "test-issuer".to_string(),
			aud: vec!["test-audience".to_string()],
		};

		let token = backend.sign_weaver_svid(&claims).await.unwrap();
		let result = backend.verify_weaver_svid(&token).await;

		assert!(matches!(result, Err(SecretsError::SvidExpired)));
	}

	#[tokio::test]
	async fn nbf_in_future_rejected() {
		let backend = create_test_backend();
		let now = chrono::Utc::now().timestamp();

		let claims = WeaverClaims {
			jti: "test-jti-nbf".to_string(),
			sub: "spiffe://loom.dev/weaver/test-123".to_string(),
			weaver_id: "test-123".to_string(),
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
			pod_uid: "pod-uid-123".to_string(),
			org_id: "org-123".to_string(),
			repo_id: None,
			owner_user_id: "user-123".to_string(),
			iat: now,
			nbf: now + 3600,
			exp: now + 7200,
			iss: "test-issuer".to_string(),
			aud: vec!["test-audience".to_string()],
		};

		let token = backend.sign_weaver_svid(&claims).await.unwrap();
		let result = backend.verify_weaver_svid(&token).await;

		assert!(matches!(result, Err(SecretsError::SvidNotYetValid)));
	}

	#[tokio::test]
	async fn nbf_with_clock_skew_accepted() {
		let backend = create_test_backend();
		let now = chrono::Utc::now().timestamp();

		let claims = WeaverClaims {
			jti: "test-jti-skew".to_string(),
			sub: "spiffe://loom.dev/weaver/test-123".to_string(),
			weaver_id: "test-123".to_string(),
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
			pod_uid: "pod-uid-123".to_string(),
			org_id: "org-123".to_string(),
			repo_id: None,
			owner_user_id: "user-123".to_string(),
			iat: now,
			nbf: now + 20,
			exp: now + 900,
			iss: "test-issuer".to_string(),
			aud: vec!["test-audience".to_string()],
		};

		let token = backend.sign_weaver_svid(&claims).await.unwrap();
		let result = backend.verify_weaver_svid(&token).await;

		assert!(result.is_ok());
	}

	#[test]
	fn jwks_has_correct_structure() {
		let backend = create_test_backend();
		let jwks = backend.jwks();

		assert_eq!(jwks.keys.len(), 1);
		let key = &jwks.keys[0];
		assert_eq!(key.kty, "OKP");
		assert_eq!(key.alg, "EdDSA");
		assert_eq!(key.crv, "Ed25519");
		assert_eq!(key.key_use, "sig");
	}

	#[test]
	fn debug_does_not_leak_kek() {
		let backend = create_test_backend();
		let debug_output = format!("{:?}", backend);

		assert!(debug_output.contains("[REDACTED]"));
	}
}
