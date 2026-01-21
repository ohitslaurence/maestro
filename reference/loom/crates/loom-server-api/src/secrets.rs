// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! API types for secrets management endpoints.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Secret scope for API requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SecretScopeApi {
	Org,
	Repo,
}

impl std::fmt::Display for SecretScopeApi {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SecretScopeApi::Org => write!(f, "org"),
			SecretScopeApi::Repo => write!(f, "repo"),
		}
	}
}

/// Request to create a new secret.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateSecretRequest {
	pub name: String,
	pub value: String,
	#[serde(default)]
	pub description: Option<String>,
}

/// Request to update an existing secret (creates a new version).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateSecretRequest {
	pub value: String,
}

/// Secret metadata response (never contains the secret value).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SecretMetadataResponse {
	pub name: String,
	pub scope: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub current_version: i32,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// Response for listing secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListSecretsResponse {
	pub secrets: Vec<SecretMetadataResponse>,
}

/// Success response for secret operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SecretSuccessResponse {
	pub message: String,
}

/// Error response for secret operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SecretErrorResponse {
	pub error: String,
	pub message: String,
}
