// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use loom_server_weaver::{Weaver, WeaverStatus};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

/// Request to create a new weaver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateWeaverApiRequest {
	/// Container image to run
	pub image: String,
	/// Organization ID that owns this weaver
	pub org_id: String,
	/// Repository ID (optional, for repo-scoped secrets)
	pub repo_id: Option<String>,
	/// Environment variables
	#[serde(default)]
	pub env: HashMap<String, String>,
	/// Resource limits
	#[serde(default)]
	pub resources: ResourceSpecApi,
	/// User-defined metadata tags
	#[serde(default)]
	pub tags: HashMap<String, String>,
	/// TTL override in hours (max: 48)
	pub lifetime_hours: Option<u32>,
	/// Override container ENTRYPOINT
	pub command: Option<Vec<String>>,
	/// Override container CMD
	pub args: Option<Vec<String>>,
	/// Override container WORKDIR
	pub workdir: Option<String>,
}

/// Resource limits for a weaver.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ResourceSpecApi {
	/// Memory limit (e.g., "8Gi")
	pub memory_limit: Option<String>,
	/// CPU limit (e.g., "4")
	pub cpu_limit: Option<String>,
}

/// Response for a single weaver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct WeaverApiResponse {
	/// Unique weaver identifier
	pub id: String,
	/// Kubernetes Pod name
	pub pod_name: String,
	/// Current weaver status
	pub status: WeaverStatusApi,
	/// When the weaver was created
	pub created_at: DateTime<Utc>,
	/// Container image
	#[serde(skip_serializing_if = "Option::is_none")]
	pub image: Option<String>,
	/// User-defined metadata tags
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tags: Option<HashMap<String, String>>,
	/// Configured lifetime in hours
	#[serde(skip_serializing_if = "Option::is_none")]
	pub lifetime_hours: Option<u32>,
	/// Current age in hours
	#[serde(skip_serializing_if = "Option::is_none")]
	pub age_hours: Option<f64>,
	/// Owner user ID
	#[serde(skip_serializing_if = "Option::is_none")]
	pub owner_user_id: Option<String>,
}

/// Weaver status for API responses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WeaverStatusApi {
	Pending,
	Running,
	Succeeded,
	Failed,
	Terminating,
}

/// Response for listing weavers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListWeaversApiResponse {
	/// List of weavers
	pub weavers: Vec<WeaverApiResponse>,
	/// Total count of weavers returned
	pub count: u32,
}

/// Query parameters for listing weavers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct ListWeaversParams {
	/// Filter by tag (format: key:value). Multiple allowed.
	#[serde(default)]
	#[cfg_attr(feature = "openapi", param(value_type = Option<Vec<String>>))]
	pub tag: Option<Vec<String>>,
}

/// Query parameters for log streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct LogStreamParams {
	/// Number of lines to tail from the end (default: 256)
	#[serde(default = "default_tail")]
	pub tail: u32,
	/// Whether to include timestamps (default: true)
	#[serde(default = "default_timestamps")]
	pub timestamps: bool,
}

fn default_tail() -> u32 {
	256
}

fn default_timestamps() -> bool {
	true
}

/// Query parameters for cleanup endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct CleanupParams {
	/// If true, only list weavers that would be deleted without actually deleting them
	#[serde(default)]
	pub dry_run: bool,
}

/// Response for cleanup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CleanupApiResponse {
	/// Whether this was a dry run
	pub dry_run: bool,
	/// Weaver IDs that were deleted (or would be deleted in dry run)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub deleted: Option<Vec<String>>,
	/// Weaver IDs that would be deleted (dry run only)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub would_delete: Option<Vec<String>>,
	/// Number of weavers affected
	pub count: u32,
}

impl From<WeaverStatus> for WeaverStatusApi {
	fn from(status: WeaverStatus) -> Self {
		match status {
			WeaverStatus::Pending => WeaverStatusApi::Pending,
			WeaverStatus::Running => WeaverStatusApi::Running,
			WeaverStatus::Succeeded => WeaverStatusApi::Succeeded,
			WeaverStatus::Failed => WeaverStatusApi::Failed,
			WeaverStatus::Terminating => WeaverStatusApi::Terminating,
		}
	}
}

impl From<Weaver> for WeaverApiResponse {
	fn from(weaver: Weaver) -> Self {
		let owner_user_id = if weaver.owner_user_id.is_empty() {
			None
		} else {
			Some(weaver.owner_user_id)
		};
		Self {
			id: weaver.id.to_string(),
			pod_name: weaver.pod_name,
			status: weaver.status.into(),
			created_at: weaver.created_at,
			image: Some(weaver.image),
			tags: Some(weaver.tags),
			lifetime_hours: Some(weaver.lifetime_hours),
			age_hours: Some(weaver.age_hours),
			owner_user_id,
		}
	}
}
