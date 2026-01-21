// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Issue types for crash analytics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::CrashError;
use crate::project::ProjectId;
use crate::{OrgId, UserId};

/// Unique identifier for an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueId(pub Uuid);

impl IssueId {
	pub fn new() -> Self {
		Self(Uuid::now_v7())
	}
}

impl Default for IssueId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for IssueId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for IssueId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// An aggregated group of similar crash events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Issue {
	pub id: IssueId,
	pub org_id: OrgId,
	pub project_id: ProjectId,

	/// Human-readable: "PROJ-123"
	pub short_id: String,
	/// SHA256 hash for grouping
	pub fingerprint: String,

	/// Exception type + first line of message
	pub title: String,
	/// Top in-app frame function
	pub culprit: Option<String>,
	pub metadata: IssueMetadata,

	pub status: IssueStatus,
	pub level: IssueLevel,
	pub priority: IssuePriority,

	/// Total events for this issue
	pub event_count: u64,
	/// Unique person_ids affected
	pub user_count: u64,

	pub first_seen: DateTime<Utc>,
	pub last_seen: DateTime<Utc>,

	/// Resolution tracking
	pub resolved_at: Option<DateTime<Utc>>,
	pub resolved_by: Option<UserId>,
	/// "fixed in 1.2.4"
	pub resolved_in_release: Option<String>,

	/// Regression tracking
	pub times_regressed: u32,
	pub last_regressed_at: Option<DateTime<Utc>>,
	pub regressed_in_release: Option<String>,

	/// Assignment
	pub assigned_to: Option<UserId>,

	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

impl Default for Issue {
	fn default() -> Self {
		let now = Utc::now();
		Self {
			id: IssueId::new(),
			org_id: OrgId::new(),
			project_id: ProjectId::new(),
			short_id: String::new(),
			fingerprint: String::new(),
			title: String::new(),
			culprit: None,
			metadata: IssueMetadata::default(),
			status: IssueStatus::Unresolved,
			level: IssueLevel::Error,
			priority: IssuePriority::Medium,
			event_count: 0,
			user_count: 0,
			first_seen: now,
			last_seen: now,
			resolved_at: None,
			resolved_by: None,
			resolved_in_release: None,
			times_regressed: 0,
			last_regressed_at: None,
			regressed_in_release: None,
			assigned_to: None,
			created_at: now,
			updated_at: now,
		}
	}
}

/// Metadata extracted from the first event for display.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IssueMetadata {
	pub exception_type: String,
	pub exception_value: String,
	pub filename: Option<String>,
	pub function: Option<String>,
}

/// Issue status representing its lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
	Unresolved,
	Resolved,
	Ignored,
	Regressed,
}

impl fmt::Display for IssueStatus {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Unresolved => write!(f, "unresolved"),
			Self::Resolved => write!(f, "resolved"),
			Self::Ignored => write!(f, "ignored"),
			Self::Regressed => write!(f, "regressed"),
		}
	}
}

impl FromStr for IssueStatus {
	type Err = CrashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"unresolved" => Ok(Self::Unresolved),
			"resolved" => Ok(Self::Resolved),
			"ignored" => Ok(Self::Ignored),
			"regressed" => Ok(Self::Regressed),
			_ => Err(CrashError::InvalidIssueStatus(s.to_string())),
		}
	}
}

/// Issue severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssueLevel {
	Error,
	Warning,
	Info,
}

impl fmt::Display for IssueLevel {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Error => write!(f, "error"),
			Self::Warning => write!(f, "warning"),
			Self::Info => write!(f, "info"),
		}
	}
}

impl FromStr for IssueLevel {
	type Err = CrashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"error" => Ok(Self::Error),
			"warning" => Ok(Self::Warning),
			"info" => Ok(Self::Info),
			_ => Err(CrashError::InvalidIssueLevel(s.to_string())),
		}
	}
}

/// Issue priority for triage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IssuePriority {
	High,
	Medium,
	Low,
}

impl fmt::Display for IssuePriority {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::High => write!(f, "high"),
			Self::Medium => write!(f, "medium"),
			Self::Low => write!(f, "low"),
		}
	}
}

impl FromStr for IssuePriority {
	type Err = CrashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"high" => Ok(Self::High),
			"medium" => Ok(Self::Medium),
			"low" => Ok(Self::Low),
			_ => Err(CrashError::InvalidIssuePriority(s.to_string())),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn issue_id_roundtrip(uuid_bytes in any::<[u8; 16]>()) {
			let uuid = Uuid::from_bytes(uuid_bytes);
			let id = IssueId(uuid);
			let s = id.to_string();
			let parsed: IssueId = s.parse().unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn issue_status_roundtrip(status in prop_oneof![
			Just(IssueStatus::Unresolved),
			Just(IssueStatus::Resolved),
			Just(IssueStatus::Ignored),
			Just(IssueStatus::Regressed),
		]) {
			let s = status.to_string();
			let parsed: IssueStatus = s.parse().unwrap();
			prop_assert_eq!(status, parsed);
		}

		#[test]
		fn issue_level_roundtrip(level in prop_oneof![
			Just(IssueLevel::Error),
			Just(IssueLevel::Warning),
			Just(IssueLevel::Info),
		]) {
			let s = level.to_string();
			let parsed: IssueLevel = s.parse().unwrap();
			prop_assert_eq!(level, parsed);
		}

		#[test]
		fn issue_priority_roundtrip(priority in prop_oneof![
			Just(IssuePriority::High),
			Just(IssuePriority::Medium),
			Just(IssuePriority::Low),
		]) {
			let s = priority.to_string();
			let parsed: IssuePriority = s.parse().unwrap();
			prop_assert_eq!(priority, parsed);
		}
	}
}
