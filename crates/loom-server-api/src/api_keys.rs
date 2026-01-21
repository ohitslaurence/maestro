// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_auth::ApiKeyScope;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// API key scope for request/response types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyScopeApi {
	/// Read thread data.
	ThreadsRead,
	/// Create and update threads.
	ThreadsWrite,
	/// Delete threads.
	ThreadsDelete,
	/// Use LLM services.
	LlmUse,
	/// Execute tools.
	ToolsUse,
}

/// An API key in API responses (without the secret key).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeyResponse {
	/// Unique identifier for the API key.
	pub id: String,
	/// Human-readable name for the key.
	pub name: String,
	/// Scopes granted to this key.
	pub scopes: Vec<ApiKeyScopeApi>,
	/// User ID who created the key.
	pub created_by: String,
	/// When the key was created.
	pub created_at: DateTime<Utc>,
	/// When the key was last used.
	pub last_used_at: Option<DateTime<Utc>>,
	/// When the key was revoked (None if active).
	pub revoked_at: Option<DateTime<Utc>>,
}

/// Response for creating a new API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateApiKeyResponse {
	/// Unique identifier for the API key.
	pub id: String,
	/// The actual API key value (only shown once!).
	pub key: String,
	/// Human-readable name for the key.
	pub name: String,
	/// Scopes granted to this key.
	pub scopes: Vec<ApiKeyScopeApi>,
	/// When the key was created.
	pub created_at: DateTime<Utc>,
}

/// Response for listing API keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListApiKeysResponse {
	pub api_keys: Vec<ApiKeyResponse>,
}

/// Request to create an API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateApiKeyRequest {
	/// Human-readable name for the key.
	pub name: String,
	/// Scopes to grant to this key.
	pub scopes: Vec<ApiKeyScopeApi>,
}

/// API key usage log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeyUsageResponse {
	/// Unique identifier for this usage record.
	pub id: String,
	/// When the request was made.
	pub timestamp: DateTime<Utc>,
	/// Client IP address (if available).
	pub ip_address: Option<String>,
	/// API endpoint accessed.
	pub endpoint: String,
	/// HTTP method used.
	pub method: String,
}

/// Response for API key usage logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeyUsageListResponse {
	pub usage: Vec<ApiKeyUsageResponse>,
	pub total: i64,
}

/// Success response for API key operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeySuccessResponse {
	pub message: String,
}

/// Error response for API key operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeyErrorResponse {
	pub error: String,
	pub message: String,
}

impl From<ApiKeyScopeApi> for ApiKeyScope {
	fn from(scope: ApiKeyScopeApi) -> Self {
		match scope {
			ApiKeyScopeApi::ThreadsRead => ApiKeyScope::ThreadsRead,
			ApiKeyScopeApi::ThreadsWrite => ApiKeyScope::ThreadsWrite,
			ApiKeyScopeApi::ThreadsDelete => ApiKeyScope::ThreadsDelete,
			ApiKeyScopeApi::LlmUse => ApiKeyScope::LlmUse,
			ApiKeyScopeApi::ToolsUse => ApiKeyScope::ToolsUse,
		}
	}
}

impl From<ApiKeyScope> for ApiKeyScopeApi {
	fn from(scope: ApiKeyScope) -> Self {
		match scope {
			ApiKeyScope::ThreadsRead => ApiKeyScopeApi::ThreadsRead,
			ApiKeyScope::ThreadsWrite => ApiKeyScopeApi::ThreadsWrite,
			ApiKeyScope::ThreadsDelete => ApiKeyScopeApi::ThreadsDelete,
			ApiKeyScope::LlmUse => ApiKeyScopeApi::LlmUse,
			ApiKeyScope::ToolsUse => ApiKeyScopeApi::ToolsUse,
		}
	}
}
