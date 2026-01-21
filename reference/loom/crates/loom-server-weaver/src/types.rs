// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver provisioning types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a weaver, using UUID7 (time-ordered).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaverId(uuid7::Uuid);

impl WeaverId {
	/// Create a new weaver ID with UUID7.
	pub fn new() -> Self {
		Self(uuid7::uuid7())
	}

	/// Get the Kubernetes-compatible name for this weaver.
	pub fn as_k8s_name(&self) -> String {
		format!("weaver-{}", self.0)
	}
}

impl Default for WeaverId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for WeaverId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for WeaverId {
	type Err = uuid7::ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let uuid = s.parse::<uuid7::Uuid>()?;
		Ok(Self(uuid))
	}
}

/// Status of a weaver, mapped from Kubernetes Pod phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaverStatus {
	/// Pod created, containers starting
	Pending,
	/// Containers running
	Running,
	/// Completed successfully (exit 0)
	Succeeded,
	/// Container failed (non-zero exit)
	Failed,
	/// Pod is being deleted (has deletionTimestamp)
	Terminating,
}

/// A weaver instance with its current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weaver {
	pub id: WeaverId,
	pub pod_name: String,
	pub status: WeaverStatus,
	pub image: String,
	pub tags: HashMap<String, String>,
	pub created_at: DateTime<Utc>,
	pub lifetime_hours: u32,
	pub age_hours: f64,
	pub owner_user_id: String,
}

/// Resource limits for a weaver.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceSpec {
	/// Memory limit (e.g., "8Gi")
	pub memory_limit: Option<String>,
	/// CPU limit (e.g., "4")
	pub cpu_limit: Option<String>,
}

/// Request to create a new weaver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWeaverRequest {
	pub image: String,
	#[serde(default)]
	pub env: HashMap<String, String>,
	#[serde(default)]
	pub resources: ResourceSpec,
	#[serde(default)]
	pub tags: HashMap<String, String>,
	pub lifetime_hours: Option<u32>,
	pub command: Option<Vec<String>>,
	pub args: Option<Vec<String>>,
	pub workdir: Option<String>,
	pub repo: Option<String>,
	pub branch: Option<String>,
	#[serde(default)]
	pub owner_user_id: Option<String>,
	/// Organization ID that owns this weaver (required for billing/isolation).
	pub org_id: String,
	/// Repository ID (optional, for repo-scoped secrets).
	pub repo_id: Option<String>,
}

/// Options for streaming weaver logs.
#[derive(Debug, Clone)]
pub struct LogStreamOptions {
	/// Number of lines to tail from the end of the log (default: 256)
	pub tail: u32,
	/// Whether to include timestamps in log output (default: true)
	pub timestamps: bool,
}

impl Default for LogStreamOptions {
	fn default() -> Self {
		Self {
			tail: 256,
			timestamps: true,
		}
	}
}

/// Result of a cleanup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
	/// IDs of weavers that were deleted
	pub deleted: Vec<WeaverId>,
	/// Number of weavers deleted
	pub count: u32,
}
