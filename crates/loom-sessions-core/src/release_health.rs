// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Release health types for tracking release quality metrics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::aggregate::SessionAggregate;

/// Computed metrics for a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReleaseHealth {
	pub project_id: String,
	pub release: String,
	pub environment: String,

	/// Total sessions for this release
	pub total_sessions: u64,
	/// Sessions with crashes
	pub crashed_sessions: u64,
	/// Sessions with handled errors
	pub errored_sessions: u64,

	/// Total unique users
	pub total_users: u64,
	/// Users who experienced crashes
	pub crashed_users: u64,

	/// Percentage of sessions without crashes: (total - crashed) / total * 100
	pub crash_free_session_rate: f64,
	/// Percentage of users without crashes: (users - crashed_users) / users * 100
	pub crash_free_user_rate: f64,

	/// This release sessions / all sessions * 100
	pub adoption_rate: f64,
	/// Current adoption stage
	pub adoption_stage: AdoptionStage,

	/// First session seen for this release
	pub first_seen: DateTime<Utc>,
	/// Last session seen for this release
	pub last_seen: DateTime<Utc>,

	/// Change in crash-free rate compared to previous period (+/- percentage points)
	pub crash_free_rate_trend: Option<f64>,
}

impl ReleaseHealth {
	/// Calculate release health from aggregates.
	///
	/// # Arguments
	/// * `project_id` - The project ID
	/// * `release` - The release version
	/// * `environment` - The environment
	/// * `aggregates` - Session aggregates for this release
	/// * `all_sessions` - Total sessions across all releases (for adoption rate)
	#[must_use]
	pub fn calculate(
		project_id: &str,
		release: &str,
		environment: &str,
		aggregates: &[SessionAggregate],
		all_sessions: u64,
	) -> Self {
		let total_sessions: u64 = aggregates.iter().map(|a| a.total_sessions).sum();
		let crashed_sessions: u64 = aggregates.iter().map(|a| a.crashed_sessions).sum();
		let errored_sessions: u64 = aggregates.iter().map(|a| a.errored_sessions).sum();
		let total_users: u64 = aggregates.iter().map(|a| a.unique_users).sum();
		let crashed_users: u64 = aggregates.iter().map(|a| a.crashed_users).sum();

		let crash_free_session_rate = if total_sessions > 0 {
			((total_sessions - crashed_sessions) as f64 / total_sessions as f64) * 100.0
		} else {
			100.0
		};

		let crash_free_user_rate = if total_users > 0 {
			((total_users - crashed_users) as f64 / total_users as f64) * 100.0
		} else {
			100.0
		};

		let adoption_rate = if all_sessions > 0 {
			(total_sessions as f64 / all_sessions as f64) * 100.0
		} else {
			0.0
		};

		let first_seen = aggregates
			.iter()
			.map(|a| a.hour)
			.min()
			.unwrap_or_else(Utc::now);
		let last_seen = aggregates
			.iter()
			.map(|a| a.hour)
			.max()
			.unwrap_or_else(Utc::now);

		Self {
			project_id: project_id.to_string(),
			release: release.to_string(),
			environment: environment.to_string(),
			total_sessions,
			crashed_sessions,
			errored_sessions,
			total_users,
			crashed_users,
			crash_free_session_rate,
			crash_free_user_rate,
			adoption_rate,
			adoption_stage: AdoptionStage::from_rate(adoption_rate),
			first_seen,
			last_seen,
			crash_free_rate_trend: None,
		}
	}

	/// Set the crash-free rate trend from previous period comparison.
	pub fn with_trend(mut self, previous_rate: f64) -> Self {
		self.crash_free_rate_trend = Some(self.crash_free_session_rate - previous_rate);
		self
	}
}

/// Release adoption stage based on percentage of traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdoptionStage {
	/// < 5% adoption
	New,
	/// 5-50% adoption
	Growing,
	/// 50-95% adoption
	Adopted,
	/// < 5% (was higher before, now being replaced)
	Replaced,
}

impl AdoptionStage {
	/// Determine adoption stage from adoption rate percentage.
	#[must_use]
	pub fn from_rate(rate: f64) -> Self {
		if rate < 5.0 {
			AdoptionStage::New
		} else if rate < 50.0 {
			AdoptionStage::Growing
		} else if rate < 95.0 {
			AdoptionStage::Adopted
		} else {
			AdoptionStage::Adopted
		}
	}
}

impl std::fmt::Display for AdoptionStage {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AdoptionStage::New => write!(f, "new"),
			AdoptionStage::Growing => write!(f, "growing"),
			AdoptionStage::Adopted => write!(f, "adopted"),
			AdoptionStage::Replaced => write!(f, "replaced"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::aggregate::SessionAggregateId;
	use chrono::TimeZone;

	fn create_test_aggregate(sessions: u64, crashed: u64, users: u64) -> SessionAggregate {
		SessionAggregate {
			id: SessionAggregateId::new(),
			org_id: "org-1".to_string(),
			project_id: "proj-1".to_string(),
			release: Some("1.0.0".to_string()),
			environment: "production".to_string(),
			hour: Utc.with_ymd_and_hms(2026, 1, 19, 12, 0, 0).unwrap(),
			total_sessions: sessions,
			exited_sessions: sessions - crashed,
			crashed_sessions: crashed,
			abnormal_sessions: 0,
			errored_sessions: 0,
			unique_users: users,
			crashed_users: if crashed > 0 { 1 } else { 0 },
			total_duration_ms: sessions * 5000,
			min_duration_ms: Some(1000),
			max_duration_ms: Some(30000),
			total_errors: 0,
			total_crashes: crashed,
			updated_at: Utc::now(),
		}
	}

	#[test]
	fn test_release_health_calculation() {
		let aggs = vec![
			create_test_aggregate(50, 2, 40),
			create_test_aggregate(50, 3, 40),
		];

		let health = ReleaseHealth::calculate("proj-1", "1.0.0", "production", &aggs, 200);

		assert_eq!(health.total_sessions, 100);
		assert_eq!(health.crashed_sessions, 5);
		assert!((health.crash_free_session_rate - 95.0).abs() < 0.01);
		assert!((health.adoption_rate - 50.0).abs() < 0.01);
	}

	#[test]
	fn test_adoption_stage_from_rate() {
		assert_eq!(AdoptionStage::from_rate(2.0), AdoptionStage::New);
		assert_eq!(AdoptionStage::from_rate(25.0), AdoptionStage::Growing);
		assert_eq!(AdoptionStage::from_rate(75.0), AdoptionStage::Adopted);
		assert_eq!(AdoptionStage::from_rate(98.0), AdoptionStage::Adopted);
	}

	#[test]
	fn test_health_with_trend() {
		let aggs = vec![create_test_aggregate(100, 5, 80)];
		let health =
			ReleaseHealth::calculate("proj-1", "1.0.0", "production", &aggs, 100).with_trend(90.0);

		assert!(health.crash_free_rate_trend.is_some());
		let trend = health.crash_free_rate_trend.unwrap();
		assert!((trend - 5.0).abs() < 0.01);
	}
}
