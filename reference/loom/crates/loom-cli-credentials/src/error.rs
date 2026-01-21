// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Credential error types.

/// Errors that can occur during credential operations.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
	#[error("IO error: {0}")]
	Io(String),

	#[error("Serialization error: {0}")]
	Serde(String),

	#[error("Permission error: {0}")]
	Permission(String),

	#[error("Credential not found for provider: {0}")]
	NotFound(String),

	#[error("Token refresh failed: {0}")]
	RefreshFailed(String),

	#[error("Invalid credential format: {0}")]
	InvalidFormat(String),

	#[error("{0}")]
	Other(String),

	#[error("Backend error: {0}")]
	Backend(String),

	#[error("Parse error: {0}")]
	Parse(String),
}

impl From<std::io::Error> for CredentialError {
	fn from(err: std::io::Error) -> Self {
		CredentialError::Io(err.to_string())
	}
}

impl From<serde_json::Error> for CredentialError {
	fn from(err: serde_json::Error) -> Self {
		CredentialError::Serde(err.to_string())
	}
}
