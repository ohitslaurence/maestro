// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

// ============================================================================
// Error Response
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AnalyticsErrorResponse {
	pub error: String,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AnalyticsSuccessResponse {
	pub message: String,
}

// ============================================================================
// Event Capture Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CaptureEventRequest {
	pub distinct_id: String,
	pub event: String,
	#[serde(default)]
	pub properties: serde_json::Value,
	pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct BatchCaptureRequest {
	pub batch: Vec<CaptureEventRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CaptureResponse {
	pub status: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub event_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub count: Option<u64>,
}

// ============================================================================
// Identify Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct IdentifyRequest {
	pub distinct_id: String,
	pub user_id: String,
	#[serde(default)]
	pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AliasRequest {
	pub distinct_id: String,
	pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SetPropertiesRequest {
	pub distinct_id: String,
	pub properties: serde_json::Value,
	#[serde(default)]
	pub set_once: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct IdentifyResponse {
	pub status: String,
	pub person_id: String,
}

// ============================================================================
// Person Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PersonResponse {
	pub id: String,
	pub org_id: String,
	pub properties: serde_json::Value,
	pub identities: Vec<PersonIdentityResponse>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PersonIdentityResponse {
	pub id: String,
	pub distinct_id: String,
	pub identity_type: String,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema, IntoParams))]
pub struct ListPersonsQuery {
	#[serde(default = "default_limit")]
	pub limit: u32,
	#[serde(default)]
	pub offset: u32,
}

fn default_limit() -> u32 {
	50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListPersonsResponse {
	pub persons: Vec<PersonResponse>,
	pub total: u64,
	pub limit: u32,
	pub offset: u32,
}

// ============================================================================
// Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EventResponse {
	pub id: String,
	pub org_id: String,
	pub person_id: Option<String>,
	pub distinct_id: String,
	pub event_name: String,
	pub properties: serde_json::Value,
	pub timestamp: DateTime<Utc>,
	pub user_agent: Option<String>,
	pub lib: Option<String>,
	pub lib_version: Option<String>,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema, IntoParams))]
pub struct ListEventsQuery {
	pub distinct_id: Option<String>,
	pub event_name: Option<String>,
	pub start_time: Option<DateTime<Utc>>,
	pub end_time: Option<DateTime<Utc>>,
	#[serde(default = "default_limit")]
	pub limit: u32,
	#[serde(default)]
	pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListEventsResponse {
	pub events: Vec<EventResponse>,
	pub total: u64,
	pub limit: u32,
	pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema, IntoParams))]
pub struct CountEventsQuery {
	pub distinct_id: Option<String>,
	pub event_name: Option<String>,
	pub start_time: Option<DateTime<Utc>>,
	pub end_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CountEventsResponse {
	pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExportEventsRequest {
	pub distinct_id: Option<String>,
	pub event_name: Option<String>,
	pub start_time: Option<DateTime<Utc>>,
	pub end_time: Option<DateTime<Utc>>,
	#[serde(default = "default_export_limit")]
	pub limit: u32,
}

fn default_export_limit() -> u32 {
	10000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExportEventsResponse {
	pub events: Vec<EventResponse>,
	pub total_exported: u64,
}

// ============================================================================
// API Key Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AnalyticsKeyTypeApi {
	Write,
	ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AnalyticsApiKeyResponse {
	pub id: String,
	pub org_id: String,
	pub name: String,
	pub key_type: AnalyticsKeyTypeApi,
	pub created_by: String,
	pub created_at: DateTime<Utc>,
	pub last_used_at: Option<DateTime<Utc>>,
	pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateAnalyticsApiKeyRequest {
	pub name: String,
	pub key_type: AnalyticsKeyTypeApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateAnalyticsApiKeyResponse {
	pub id: String,
	pub key: String,
	pub name: String,
	pub key_type: AnalyticsKeyTypeApi,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListAnalyticsApiKeysResponse {
	pub api_keys: Vec<AnalyticsApiKeyResponse>,
}
