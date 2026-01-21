// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Thread-related API types.

use loom_common_thread::{ThreadSummary, ThreadVisibility};
use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

fn default_limit() -> u32 {
	50
}

/// Query parameters for listing threads.
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct ListParams {
	/// Filter by workspace root.
	pub workspace: Option<String>,
	/// Maximum number of results (default: 50).
	#[serde(default = "default_limit")]
	pub limit: u32,
	/// Pagination offset (default: 0).
	#[serde(default)]
	pub offset: u32,
}

/// Query parameters for search endpoint
#[derive(Debug, Clone, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct SearchParams {
	/// Search query
	pub q: String,
	/// Optional workspace filter
	pub workspace: Option<String>,
	/// Maximum results (default: 50)
	#[serde(default = "default_limit")]
	pub limit: u32,
	/// Pagination offset (default: 0)
	#[serde(default)]
	pub offset: u32,
}

/// Response for list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListResponse {
	pub threads: Vec<ThreadSummary>,
	pub total: u64,
	pub limit: u32,
	pub offset: u32,
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SearchResponse {
	pub hits: Vec<SearchResponseHit>,
	pub limit: u32,
	pub offset: u32,
}

/// Single search hit in the response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SearchResponseHit {
	#[serde(flatten)]
	pub summary: ThreadSummary,
	pub score: f64,
}

/// Request body for updating thread visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateVisibilityRequest {
	pub visibility: ThreadVisibility,
}
