// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the secrets management system.

use thiserror::Error;

/// Result type alias for secrets operations.
pub type SecretsResult<T> = Result<T, SecretsError>;

/// Errors that can occur during secrets operations.
#[derive(Debug, Error)]
pub enum SecretsError {
	// =========================================================================
	// Configuration Errors
	// =========================================================================
	#[error("configuration error: {0}")]
	Configuration(String),

	#[error("master key not configured")]
	MasterKeyNotConfigured,

	#[error("SVID signing key not configured")]
	SvidSigningKeyNotConfigured,

	// =========================================================================
	// Encryption Errors
	// =========================================================================
	#[error("encryption failed: {0}")]
	Encryption(String),

	#[error("decryption failed: {0}")]
	Decryption(String),

	#[error("invalid key size: expected {expected}, got {actual}")]
	InvalidKeySize { expected: usize, actual: usize },

	#[error("key version mismatch: expected {expected}, got {actual}")]
	KeyVersionMismatch { expected: u32, actual: u32 },

	// =========================================================================
	// SVID Errors
	// =========================================================================
	#[error("SVID signing failed: {0}")]
	SvidSigning(String),

	#[error("SVID validation failed: {0}")]
	SvidValidation(String),

	#[error("SVID has expired")]
	SvidExpired,

	#[error("SVID is not yet valid")]
	SvidNotYetValid,

	#[error("SVID has invalid signature")]
	SvidInvalidSignature,

	#[error("SVID has invalid audience")]
	SvidInvalidAudience,

	#[error("SVID has invalid issuer")]
	SvidInvalidIssuer,

	// =========================================================================
	// Secret Access Errors
	// =========================================================================
	#[error("secret not found: {0}")]
	SecretNotFound(String),

	#[error("secret not found by id: {0}")]
	SecretNotFoundById(crate::types::SecretId),

	#[error("secret version not found: {secret_id} v{version}")]
	SecretVersionNotFound { secret_id: String, version: u32 },

	#[error("access denied: {0}")]
	AccessDenied(String),

	#[error("secret already exists: {0}")]
	SecretAlreadyExists(String),

	#[error("secret is disabled: {0}")]
	SecretDisabled(String),

	#[error("invalid secret name: {0}")]
	InvalidSecretName(String),

	#[error("DEK not found: {0}")]
	DekNotFound(String),

	#[error("invalid claim: {0}")]
	InvalidClaim(String),

	#[error("invalid nonce: {0}")]
	InvalidNonce(String),

	#[error("corrupted data: {0}")]
	CorruptedData(String),

	#[error("invalid SPIFFE ID: {0}")]
	InvalidSpiffeId(String),

	#[error("pod metadata mismatch: {0}")]
	PodMetadataMismatch(String),

	// =========================================================================
	// Infrastructure Errors
	// =========================================================================
	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("internal error: {0}")]
	Internal(String),
}

impl SecretsError {
	/// Returns true if this error should be logged at error level.
	pub fn is_internal(&self) -> bool {
		matches!(
			self,
			SecretsError::Database(_)
				| SecretsError::Internal(_)
				| SecretsError::Configuration(_)
				| SecretsError::MasterKeyNotConfigured
				| SecretsError::SvidSigningKeyNotConfigured
		)
	}

	/// Returns the HTTP status code for this error.
	pub fn status_code(&self) -> u16 {
		match self {
			// 400 Bad Request
			SecretsError::Configuration(_) | SecretsError::InvalidKeySize { .. } => 400,

			// 500 - Server misconfiguration
			SecretsError::MasterKeyNotConfigured | SecretsError::SvidSigningKeyNotConfigured => 500,

			// 401 Unauthorized
			SecretsError::SvidExpired
			| SecretsError::SvidNotYetValid
			| SecretsError::SvidInvalidSignature
			| SecretsError::SvidInvalidAudience
			| SecretsError::SvidInvalidIssuer
			| SecretsError::SvidValidation(_) => 401,

			// 403 Forbidden
			SecretsError::AccessDenied(_)
			| SecretsError::KeyVersionMismatch { .. }
			| SecretsError::SecretDisabled(_) => 403,

			// 404 Not Found
			SecretsError::SecretNotFound(_)
			| SecretsError::SecretNotFoundById(_)
			| SecretsError::SecretVersionNotFound { .. }
			| SecretsError::DekNotFound(_) => 404,

			// 400 Bad Request
			SecretsError::InvalidSecretName(_)
			| SecretsError::InvalidClaim(_)
			| SecretsError::InvalidNonce(_)
			| SecretsError::CorruptedData(_)
			| SecretsError::InvalidSpiffeId(_)
			| SecretsError::PodMetadataMismatch(_) => 400,

			// 409 Conflict
			SecretsError::SecretAlreadyExists(_) => 409,

			// 500 Internal Server Error
			SecretsError::Encryption(_)
			| SecretsError::Decryption(_)
			| SecretsError::SvidSigning(_)
			| SecretsError::Database(_)
			| SecretsError::Internal(_) => 500,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn svid_expired_is_401() {
		assert_eq!(SecretsError::SvidExpired.status_code(), 401);
	}

	#[test]
	fn access_denied_is_403() {
		assert_eq!(SecretsError::AccessDenied("test".into()).status_code(), 403);
	}

	#[test]
	fn secret_not_found_is_404() {
		assert_eq!(
			SecretsError::SecretNotFound("test".into()).status_code(),
			404
		);
	}

	#[test]
	fn internal_errors_are_flagged() {
		assert!(SecretsError::Internal("test".into()).is_internal());
		assert!(!SecretsError::SvidExpired.is_internal());
	}
}
