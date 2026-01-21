// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the secrets client.

use thiserror::Error;

/// Errors that can occur when accessing secrets.
#[derive(Debug, Error)]
pub enum SecretsClientError {
	/// Failed to read K8s service account token.
	#[error("failed to read service account token: {0}")]
	ServiceAccountToken(String),

	/// Failed to obtain SVID.
	#[error("failed to obtain SVID: {0}")]
	SvidIssuance(String),

	/// SVID has expired.
	#[error("SVID has expired, please refresh")]
	SvidExpired,

	/// Failed to fetch secret.
	#[error("failed to fetch secret: {0}")]
	SecretFetch(String),

	/// Secret not found.
	#[error("secret not found: {0}")]
	SecretNotFound(String),

	/// Access denied to secret.
	#[error("access denied: {0}")]
	AccessDenied(String),

	/// HTTP error.
	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),

	/// IO error.
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	/// Configuration error.
	#[error("configuration error: {0}")]
	Configuration(String),

	/// Invalid response.
	#[error("invalid response: {0}")]
	InvalidResponse(String),
}

/// Result type for secrets client operations.
pub type SecretsClientResult<T> = Result<T, SecretsClientError>;
