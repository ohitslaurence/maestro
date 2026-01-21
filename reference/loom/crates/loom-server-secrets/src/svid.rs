// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver SVID (SPIFFE Verifiable Identity Document) issuance and validation.
//!
//! This module implements SPIFFE-style identity for weavers:
//!
//! 1. Weaver presents K8s Service Account JWT
//! 2. Server validates via K8s TokenReview API
//! 3. Server fetches Pod to verify labels
//! 4. Server issues short-lived Weaver SVID (JWT)
//!
//! SVIDs use the SPIFFE ID format: `spiffe://loom.dev/weaver/{weaver-id}`

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::error::{SecretsError, SecretsResult};
use crate::key_backend::{JsonWebKeySet, KeyBackend};

/// A validated K8s service account token.
/// This type can only be constructed by successful TokenReview validation.
#[derive(Debug, Clone)]
pub struct ValidatedSaToken {
	/// The pod name from TokenReview
	pub pod_name: String,
	/// The namespace from TokenReview
	pub namespace: String,
	/// The service account name
	pub service_account: String,
}

impl ValidatedSaToken {
	/// Create a new ValidatedSaToken. This should only be called after
	/// successful K8s TokenReview validation.
	pub fn new(pod_name: String, namespace: String, service_account: String) -> Self {
		Self {
			pod_name,
			namespace,
			service_account,
		}
	}
}

/// Claims in a Weaver SVID JWT.
#[derive(Clone, Serialize, Deserialize)]
pub struct WeaverClaims {
	/// JWT ID - unique token identifier (UUID)
	pub jti: String,
	/// SPIFFE ID: spiffe://loom.dev/weaver/{weaver-id}
	pub sub: String,
	/// Weaver ID (UUID7)
	pub weaver_id: String,
	/// K8s Pod name
	pub pod_name: String,
	/// K8s namespace
	pub pod_namespace: String,
	/// K8s Pod UID
	pub pod_uid: String,
	/// Organization ID
	pub org_id: String,
	/// Repository ID (optional)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub repo_id: Option<String>,
	/// User ID who launched the weaver
	pub owner_user_id: String,
	/// Issued at (Unix timestamp)
	pub iat: i64,
	/// Not before (Unix timestamp)
	pub nbf: i64,
	/// Expiration (Unix timestamp)
	pub exp: i64,
	/// Issuer
	pub iss: String,
	/// Audience
	pub aud: Vec<String>,
}

impl std::fmt::Debug for WeaverClaims {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WeaverClaims")
			.field("jti", &self.jti)
			.field("sub", &self.sub)
			.field("weaver_id", &self.weaver_id)
			.field("pod_name", &self.pod_name)
			.field("pod_namespace", &self.pod_namespace)
			.field("pod_uid", &"[REDACTED]")
			.field("org_id", &"[REDACTED]")
			.field("repo_id", &self.repo_id.as_ref().map(|_| "[REDACTED]"))
			.field("owner_user_id", &"[REDACTED]")
			.field("iat", &self.iat)
			.field("nbf", &self.nbf)
			.field("exp", &self.exp)
			.field("iss", &self.iss)
			.field("aud", &self.aud)
			.finish()
	}
}

impl WeaverClaims {
	/// Default SVID TTL: 15 minutes
	pub const DEFAULT_TTL_SECONDS: i64 = 900;

	/// Maximum SVID TTL: 1 hour (for defense-in-depth)
	pub const MAX_TTL_SECONDS: i64 = 3600;

	/// Build a new WeaverClaims with standard fields.
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		weaver_id: String,
		pod_name: String,
		pod_namespace: String,
		pod_uid: String,
		org_id: String,
		repo_id: Option<String>,
		owner_user_id: String,
		issuer: &str,
		audience: &str,
		ttl_seconds: Option<i64>,
	) -> SecretsResult<Self> {
		let now = Utc::now().timestamp();
		let ttl = ttl_seconds
			.unwrap_or(Self::DEFAULT_TTL_SECONDS)
			.min(Self::MAX_TTL_SECONDS);
		let spiffe_id = Self::spiffe_id(&weaver_id)?;

		Ok(Self {
			jti: Uuid::new_v4().to_string(),
			sub: spiffe_id,
			weaver_id,
			pod_name,
			pod_namespace,
			pod_uid,
			org_id,
			repo_id,
			owner_user_id,
			iat: now,
			nbf: now,
			exp: now + ttl,
			iss: issuer.to_string(),
			aud: vec![audience.to_string()],
		})
	}

	/// Create the SPIFFE ID for a weaver.
	/// Validates that weaver_id contains only valid characters (alphanumeric, hyphens, underscores)
	/// and is between 1-128 characters.
	pub fn spiffe_id(weaver_id: &str) -> SecretsResult<String> {
		if weaver_id.is_empty() || weaver_id.len() > 128 {
			return Err(SecretsError::InvalidSpiffeId(
				"weaver_id length must be 1-128".into(),
			));
		}
		if !weaver_id
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
		{
			return Err(SecretsError::InvalidSpiffeId(
				"weaver_id contains invalid characters".into(),
			));
		}
		Ok(format!("spiffe://loom.dev/weaver/{weaver_id}"))
	}

	/// Check if the SVID is expired.
	pub fn is_expired(&self) -> bool {
		Utc::now().timestamp() >= self.exp
	}

	/// Get the weaver ID.
	pub fn weaver_id(&self) -> &str {
		&self.weaver_id
	}

	/// Seconds until expiration (negative if expired).
	pub fn seconds_until_expiry(&self) -> i64 {
		self.exp - Utc::now().timestamp()
	}
}

/// An issued Weaver SVID.
#[derive(Clone, Serialize)]
pub struct WeaverSvid {
	/// The JWT token
	pub token: String,
	/// Token type (always "Bearer")
	pub token_type: String,
	/// Expiration time
	pub expires_at: DateTime<Utc>,
	/// SPIFFE ID
	pub spiffe_id: String,
}

impl std::fmt::Debug for WeaverSvid {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WeaverSvid")
			.field("token", &"[REDACTED]")
			.field("token_type", &self.token_type)
			.field("expires_at", &self.expires_at)
			.field("spiffe_id", &self.spiffe_id)
			.finish()
	}
}

/// Request to issue a Weaver SVID.
#[derive(Debug, Clone, Deserialize)]
pub struct SvidRequest {
	/// K8s Pod name
	pub pod_name: String,
	/// K8s namespace
	pub pod_namespace: String,
}

/// Configuration for SVID issuance.
#[derive(Debug, Clone)]
pub struct SvidConfig {
	/// SVID TTL in seconds (default: 900 = 15 minutes)
	pub ttl_seconds: i64,
	/// Issuer name
	pub issuer: String,
	/// Audience
	pub audience: String,
	/// Whether to verify Pod still exists before access
	pub verify_pod_exists: bool,
	/// Expected weaver namespace
	pub weaver_namespace: String,
}

impl Default for SvidConfig {
	fn default() -> Self {
		Self {
			ttl_seconds: 900,
			issuer: "loom-secrets".to_string(),
			audience: "loom-secrets".to_string(),
			verify_pod_exists: true,
			weaver_namespace: "loom-weavers".to_string(),
		}
	}
}

/// Pod metadata extracted from K8s.
#[derive(Debug, Clone)]
pub struct PodMetadata {
	pub name: String,
	pub namespace: String,
	pub uid: String,
	pub weaver_id: Option<String>,
	pub org_id: Option<String>,
	pub repo_id: Option<String>,
	pub owner_user_id: Option<String>,
	pub is_managed: bool,
}

/// SVID issuer that validates K8s tokens and issues Weaver SVIDs.
pub struct SvidIssuer<K: KeyBackend> {
	key_backend: Arc<K>,
	config: SvidConfig,
}

impl<K: KeyBackend> SvidIssuer<K> {
	/// Create a new SVID issuer.
	pub fn new(key_backend: Arc<K>, config: SvidConfig) -> Self {
		Self {
			key_backend,
			config,
		}
	}

	/// Issue a Weaver SVID after validating the K8s SA token.
	///
	/// # Arguments
	///
	/// * `validated_token` - Validated K8s Service Account token (from TokenReview)
	/// * `request` - SVID request with pod info
	/// * `pod_metadata` - Pod metadata extracted from K8s
	///
	/// # Security
	///
	/// 1. Validates that validated_token metadata matches pod_metadata
	/// 2. Verifies `loom.dev/managed=true` label
	/// 3. Extracts weaver identity from labels
	/// 4. Issues short-lived SVID with jti and nbf claims
	#[instrument(skip(self, validated_token), fields(pod_name = %request.pod_name, pod_namespace = %request.pod_namespace))]
	pub async fn issue_svid(
		&self,
		validated_token: &ValidatedSaToken,
		request: &SvidRequest,
		pod_metadata: &PodMetadata,
	) -> SecretsResult<WeaverSvid> {
		// Verify request matches validated token
		if request.pod_name != validated_token.pod_name {
			warn!(
				request_pod = %request.pod_name,
				token_pod = %validated_token.pod_name,
				"Pod name mismatch between request and validated token"
			);
			return Err(SecretsError::PodMetadataMismatch(
				"request pod_name does not match validated token".into(),
			));
		}
		if request.pod_namespace != validated_token.namespace {
			warn!(
				request_ns = %request.pod_namespace,
				token_ns = %validated_token.namespace,
				"Namespace mismatch between request and validated token"
			);
			return Err(SecretsError::PodMetadataMismatch(
				"request pod_namespace does not match validated token".into(),
			));
		}

		// Verify pod_metadata matches validated token
		if pod_metadata.name != validated_token.pod_name {
			warn!(
				metadata_pod = %pod_metadata.name,
				token_pod = %validated_token.pod_name,
				"Pod name mismatch between metadata and validated token"
			);
			return Err(SecretsError::PodMetadataMismatch(
				"pod_metadata name does not match validated token".into(),
			));
		}
		if pod_metadata.namespace != validated_token.namespace {
			warn!(
				metadata_ns = %pod_metadata.namespace,
				token_ns = %validated_token.namespace,
				"Namespace mismatch between metadata and validated token"
			);
			return Err(SecretsError::PodMetadataMismatch(
				"pod_metadata namespace does not match validated token".into(),
			));
		}

		// Verify namespace matches expected weaver namespace
		if request.pod_namespace != self.config.weaver_namespace {
			warn!(
				expected = %self.config.weaver_namespace,
				got = %request.pod_namespace,
				"SVID request from unexpected namespace"
			);
			return Err(SecretsError::SvidValidation(
				"pod not in weaver namespace".into(),
			));
		}

		// Verify pod is managed by Loom
		if !pod_metadata.is_managed {
			warn!(pod_name = %request.pod_name, "SVID request from unmanaged pod");
			return Err(SecretsError::SvidValidation(
				"pod not managed by Loom".into(),
			));
		}

		// Extract required labels
		let weaver_id = pod_metadata
			.weaver_id
			.as_ref()
			.ok_or_else(|| SecretsError::SvidValidation("missing loom.dev/weaver-id label".into()))?;

		let org_id = pod_metadata
			.org_id
			.as_ref()
			.ok_or_else(|| SecretsError::SvidValidation("missing loom.dev/org-id label".into()))?;

		let owner_user_id = pod_metadata
			.owner_user_id
			.as_ref()
			.ok_or_else(|| SecretsError::SvidValidation("missing loom.dev/owner-user-id label".into()))?;

		// Validate weaver_id format for SPIFFE ID
		let spiffe_id = WeaverClaims::spiffe_id(weaver_id)?;

		// Create claims with TTL clamped to maximum (defense in depth)
		let now = Utc::now();
		let ttl = self.config.ttl_seconds.min(WeaverClaims::MAX_TTL_SECONDS);
		let expires_at = now + Duration::seconds(ttl);

		let claims = WeaverClaims {
			jti: Uuid::new_v4().to_string(),
			sub: spiffe_id,
			weaver_id: weaver_id.clone(),
			pod_name: pod_metadata.name.clone(),
			pod_namespace: pod_metadata.namespace.clone(),
			pod_uid: pod_metadata.uid.clone(),
			org_id: org_id.clone(),
			repo_id: pod_metadata.repo_id.clone(),
			owner_user_id: owner_user_id.clone(),
			iat: now.timestamp(),
			nbf: now.timestamp(),
			exp: expires_at.timestamp(),
			iss: self.config.issuer.clone(),
			aud: vec![self.config.audience.clone()],
		};

		// Sign the SVID
		let token = self.key_backend.sign_weaver_svid(&claims).await?;

		info!(
			weaver_id = %weaver_id,
			org_id = %org_id,
			jti = %claims.jti,
			expires_at = %expires_at,
			"Issued Weaver SVID"
		);

		Ok(WeaverSvid {
			token,
			token_type: "Bearer".to_string(),
			expires_at,
			spiffe_id: claims.sub,
		})
	}

	/// Verify a Weaver SVID and extract claims.
	#[instrument(skip(self, token))]
	pub async fn verify_svid(&self, token: &str) -> SecretsResult<WeaverClaims> {
		self.key_backend.verify_weaver_svid(token).await
	}

	/// Get the JWKS for this issuer.
	pub fn jwks(&self) -> JsonWebKeySet {
		self.key_backend.jwks()
	}

	/// Get the SVID configuration.
	pub fn config(&self) -> &SvidConfig {
		&self.config
	}

	/// Get a reference to the key backend.
	pub fn key_backend(&self) -> &K {
		&self.key_backend
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::encryption::generate_key;
	use crate::key_backend::SoftwareKeyBackend;

	fn create_test_issuer() -> SvidIssuer<SoftwareKeyBackend> {
		let kek = generate_key();
		let backend = Arc::new(SoftwareKeyBackend::new(
			kek,
			None,
			"loom-secrets".to_string(),
			"loom-secrets".to_string(),
		));
		let config = SvidConfig::default();
		SvidIssuer::new(backend, config)
	}

	fn create_test_pod_metadata() -> PodMetadata {
		PodMetadata {
			name: "weaver-test-123".to_string(),
			namespace: "loom-weavers".to_string(),
			uid: "pod-uid-123".to_string(),
			weaver_id: Some("test-123".to_string()),
			org_id: Some("org-123".to_string()),
			repo_id: Some("repo-123".to_string()),
			owner_user_id: Some("user-123".to_string()),
			is_managed: true,
		}
	}

	fn create_test_validated_token() -> ValidatedSaToken {
		ValidatedSaToken::new(
			"weaver-test-123".to_string(),
			"loom-weavers".to_string(),
			"weaver-sa".to_string(),
		)
	}

	#[tokio::test]
	async fn issue_and_verify_svid() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let pod_metadata = create_test_pod_metadata();
		let validated_token = create_test_validated_token();

		let svid = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await
			.unwrap();

		assert_eq!(svid.token_type, "Bearer");
		assert!(svid.expires_at > Utc::now());
		assert!(svid.spiffe_id.contains("test-123"));

		// Verify the token
		let claims = issuer.verify_svid(&svid.token).await.unwrap();
		assert_eq!(claims.weaver_id, "test-123");
		assert_eq!(claims.org_id, "org-123");
		assert!(!claims.jti.is_empty());
		assert_eq!(claims.nbf, claims.iat);
	}

	#[tokio::test]
	async fn reject_unmanaged_pod() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let mut pod_metadata = create_test_pod_metadata();
		pod_metadata.is_managed = false;
		let validated_token = create_test_validated_token();

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::SvidValidation(_))));
	}

	#[tokio::test]
	async fn reject_wrong_namespace() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "default".to_string(), // Wrong namespace
		};
		let pod_metadata = create_test_pod_metadata();
		let validated_token = ValidatedSaToken::new(
			"weaver-test-123".to_string(),
			"default".to_string(),
			"weaver-sa".to_string(),
		);

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::PodMetadataMismatch(_))));
	}

	#[tokio::test]
	async fn reject_missing_weaver_id() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let mut pod_metadata = create_test_pod_metadata();
		pod_metadata.weaver_id = None;
		let validated_token = create_test_validated_token();

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::SvidValidation(_))));
	}

	#[tokio::test]
	async fn reject_missing_org_id() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let mut pod_metadata = create_test_pod_metadata();
		pod_metadata.org_id = None;
		let validated_token = create_test_validated_token();

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::SvidValidation(_))));
	}

	#[tokio::test]
	async fn reject_missing_owner_user_id() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let mut pod_metadata = create_test_pod_metadata();
		pod_metadata.owner_user_id = None;
		let validated_token = create_test_validated_token();

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::SvidValidation(_))));
	}

	#[tokio::test]
	async fn reject_pod_name_mismatch() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let pod_metadata = create_test_pod_metadata();
		let validated_token = ValidatedSaToken::new(
			"different-pod".to_string(),
			"loom-weavers".to_string(),
			"weaver-sa".to_string(),
		);

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::PodMetadataMismatch(_))));
	}

	#[tokio::test]
	async fn reject_namespace_mismatch() {
		let issuer = create_test_issuer();
		let request = SvidRequest {
			pod_name: "weaver-test-123".to_string(),
			pod_namespace: "loom-weavers".to_string(),
		};
		let pod_metadata = create_test_pod_metadata();
		let validated_token = ValidatedSaToken::new(
			"weaver-test-123".to_string(),
			"different-namespace".to_string(),
			"weaver-sa".to_string(),
		);

		let result = issuer
			.issue_svid(&validated_token, &request, &pod_metadata)
			.await;
		assert!(matches!(result, Err(SecretsError::PodMetadataMismatch(_))));
	}

	#[test]
	fn spiffe_id_format() {
		let spiffe_id = WeaverClaims::spiffe_id("abc-123").unwrap();
		assert_eq!(spiffe_id, "spiffe://loom.dev/weaver/abc-123");
	}

	#[test]
	fn spiffe_id_rejects_empty() {
		let result = WeaverClaims::spiffe_id("");
		assert!(matches!(result, Err(SecretsError::InvalidSpiffeId(_))));
	}

	#[test]
	fn spiffe_id_rejects_too_long() {
		let long_id = "a".repeat(129);
		let result = WeaverClaims::spiffe_id(&long_id);
		assert!(matches!(result, Err(SecretsError::InvalidSpiffeId(_))));
	}

	#[test]
	fn spiffe_id_rejects_invalid_chars() {
		let result = WeaverClaims::spiffe_id("abc/def");
		assert!(matches!(result, Err(SecretsError::InvalidSpiffeId(_))));

		let result = WeaverClaims::spiffe_id("abc..def");
		assert!(matches!(result, Err(SecretsError::InvalidSpiffeId(_))));

		let result = WeaverClaims::spiffe_id("abc def");
		assert!(matches!(result, Err(SecretsError::InvalidSpiffeId(_))));
	}

	#[test]
	fn spiffe_id_accepts_valid_chars() {
		assert!(WeaverClaims::spiffe_id("abc-123").is_ok());
		assert!(WeaverClaims::spiffe_id("abc_123").is_ok());
		assert!(WeaverClaims::spiffe_id("ABC123").is_ok());
		assert!(WeaverClaims::spiffe_id("a-b_c-1_2_3").is_ok());
	}

	#[test]
	fn new_claims_has_correct_spiffe_id() {
		let claims = WeaverClaims::new(
			"weaver-123".into(),
			"pod-name".into(),
			"loom-weavers".into(),
			"pod-uid".into(),
			"org-123".into(),
			Some("repo-456".into()),
			"user-789".into(),
			"loom-secrets",
			"loom-secrets",
			None,
		)
		.unwrap();

		assert_eq!(claims.sub, "spiffe://loom.dev/weaver/weaver-123");
		assert_eq!(claims.weaver_id, "weaver-123");
		assert!(!claims.jti.is_empty());
		assert_eq!(claims.nbf, claims.iat);
	}

	#[test]
	fn default_ttl_is_15_minutes() {
		let claims = WeaverClaims::new(
			"weaver-123".into(),
			"pod-name".into(),
			"loom-weavers".into(),
			"pod-uid".into(),
			"org-123".into(),
			None,
			"user-789".into(),
			"loom-secrets",
			"loom-secrets",
			None,
		)
		.unwrap();

		let expected_ttl = WeaverClaims::DEFAULT_TTL_SECONDS;
		let actual_ttl = claims.exp - claims.iat;
		assert_eq!(actual_ttl, expected_ttl);
	}

	#[test]
	fn custom_ttl_works() {
		let claims = WeaverClaims::new(
			"weaver-123".into(),
			"pod-name".into(),
			"loom-weavers".into(),
			"pod-uid".into(),
			"org-123".into(),
			None,
			"user-789".into(),
			"loom-secrets",
			"loom-secrets",
			Some(300),
		)
		.unwrap();

		let actual_ttl = claims.exp - claims.iat;
		assert_eq!(actual_ttl, 300);
	}

	#[test]
	fn is_expired_works() {
		let mut claims = WeaverClaims::new(
			"weaver-123".into(),
			"pod-name".into(),
			"loom-weavers".into(),
			"pod-uid".into(),
			"org-123".into(),
			None,
			"user-789".into(),
			"loom-secrets",
			"loom-secrets",
			None,
		)
		.unwrap();

		assert!(!claims.is_expired());

		claims.exp = Utc::now().timestamp() - 100;
		assert!(claims.is_expired());
	}

	#[test]
	fn default_config_values() {
		let config = SvidConfig::default();
		assert_eq!(config.ttl_seconds, 900);
		assert_eq!(config.issuer, "loom-secrets");
		assert_eq!(config.audience, "loom-secrets");
		assert!(config.verify_pod_exists);
		assert_eq!(config.weaver_namespace, "loom-weavers");
	}

	#[test]
	fn weaver_svid_debug_redacts_token() {
		let svid = WeaverSvid {
			token: "super-secret-token-12345".to_string(),
			token_type: "Bearer".to_string(),
			expires_at: Utc::now(),
			spiffe_id: "spiffe://loom.dev/weaver/test".to_string(),
		};
		let debug_output = format!("{:?}", svid);
		assert!(!debug_output.contains("super-secret-token"));
		assert!(debug_output.contains("[REDACTED]"));
		assert!(debug_output.contains("Bearer"));
		assert!(debug_output.contains("spiffe://loom.dev/weaver/test"));
	}

	#[test]
	fn weaver_claims_debug_redacts_sensitive_fields() {
		let claims = WeaverClaims {
			jti: "test-jti".to_string(),
			sub: "spiffe://loom.dev/weaver/test".to_string(),
			weaver_id: "test-weaver".to_string(),
			pod_name: "test-pod".to_string(),
			pod_namespace: "loom-weavers".to_string(),
			pod_uid: "secret-pod-uid-123".to_string(),
			org_id: "secret-org-id-456".to_string(),
			repo_id: Some("secret-repo-id-789".to_string()),
			owner_user_id: "secret-user-id-000".to_string(),
			iat: 0,
			nbf: 0,
			exp: 0,
			iss: "test-issuer".to_string(),
			aud: vec!["test-audience".to_string()],
		};
		let debug_output = format!("{:?}", claims);
		assert!(!debug_output.contains("secret-pod-uid"));
		assert!(!debug_output.contains("secret-org-id"));
		assert!(!debug_output.contains("secret-repo-id"));
		assert!(!debug_output.contains("secret-user-id"));
		assert!(debug_output.contains("[REDACTED]"));
		assert!(debug_output.contains("test-weaver"));
		assert!(debug_output.contains("test-pod"));
	}

	#[test]
	fn max_ttl_is_enforced() {
		let claims = WeaverClaims::new(
			"weaver-123".into(),
			"pod-name".into(),
			"loom-weavers".into(),
			"pod-uid".into(),
			"org-123".into(),
			None,
			"user-789".into(),
			"loom-secrets",
			"loom-secrets",
			Some(7200), // Request 2 hours, should be clamped to 1 hour
		)
		.unwrap();

		let actual_ttl = claims.exp - claims.iat;
		assert_eq!(actual_ttl, WeaverClaims::MAX_TTL_SECONDS);
	}
}
