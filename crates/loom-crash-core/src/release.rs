// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Release types for crash tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::project::ProjectId;
use crate::OrgId;

/// Unique identifier for a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseId(pub Uuid);

impl ReleaseId {
	pub fn new() -> Self {
		Self(Uuid::now_v7())
	}
}

impl Default for ReleaseId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for ReleaseId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for ReleaseId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Release tracking for crash correlation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Release {
	pub id: ReleaseId,
	pub org_id: OrgId,
	pub project_id: ProjectId,

	/// Semantic version or commit SHA
	pub version: String,
	/// Display version
	pub short_version: Option<String>,
	/// Link to release notes/commit
	pub url: Option<String>,

	/// Stats
	pub crash_count: u64,
	pub new_issue_count: u64,
	pub regression_count: u64,
	pub user_count: u64,

	/// Timestamps
	pub date_released: Option<DateTime<Utc>>,
	pub first_event: Option<DateTime<Utc>>,
	pub last_event: Option<DateTime<Utc>>,

	pub created_at: DateTime<Utc>,
}

impl Default for Release {
	fn default() -> Self {
		let now = Utc::now();
		Self {
			id: ReleaseId::new(),
			org_id: OrgId::new(),
			project_id: ProjectId::new(),
			version: String::new(),
			short_version: None,
			url: None,
			crash_count: 0,
			new_issue_count: 0,
			regression_count: 0,
			user_count: 0,
			date_released: None,
			first_event: None,
			last_event: None,
			created_at: now,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn release_id_roundtrip(uuid_bytes in any::<[u8; 16]>()) {
			let uuid = Uuid::from_bytes(uuid_bytes);
			let id = ReleaseId(uuid);
			let s = id.to_string();
			let parsed: ReleaseId = s.parse().unwrap();
			prop_assert_eq!(id, parsed);
		}
	}
}
