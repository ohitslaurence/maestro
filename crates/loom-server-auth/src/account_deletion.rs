// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Account deletion with soft-delete and grace period support.
//!
//! Implements:
//! - 90-day grace period before permanent deletion
//! - Self-service restore during grace period
//! - Per-user tombstone for org threads

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::UserId;

/// Number of days before a soft-deleted account is permanently purged.
pub const ACCOUNT_DELETION_GRACE_DAYS: i64 = 90;

/// A request to delete a user account (soft-delete).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionRequest {
	pub id: Uuid,
	pub user_id: UserId,
	pub requested_at: DateTime<Utc>,
	pub hard_delete_at: DateTime<Utc>,
	pub restored_at: Option<DateTime<Utc>>,
}

impl DeletionRequest {
	/// Create a new deletion request for the given user.
	pub fn new(user_id: UserId) -> Self {
		let now = Utc::now();
		Self {
			id: Uuid::new_v4(),
			user_id,
			requested_at: now,
			hard_delete_at: now + Duration::days(ACCOUNT_DELETION_GRACE_DAYS),
			restored_at: None,
		}
	}

	/// Restore the account, canceling the deletion request.
	pub fn restore(&mut self) {
		self.restored_at = Some(Utc::now());
	}

	/// Returns true if the account has been restored.
	pub fn is_restored(&self) -> bool {
		self.restored_at.is_some()
	}

	/// Returns true if the account should be permanently purged.
	pub fn should_purge(&self) -> bool {
		!self.is_restored() && is_past_grace_period(self)
	}

	/// Returns the number of days until the account is permanently purged.
	/// Returns None if already purged or restored.
	pub fn days_until_purge(&self) -> Option<i64> {
		if self.is_restored() {
			return None;
		}

		let now = Utc::now();
		if now >= self.hard_delete_at {
			return None;
		}

		Some((self.hard_delete_at - now).num_days())
	}
}

/// A tombstone representing a deleted user in org threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneUser {
	pub original_user_id: UserId,
	pub tombstone_id: String,
	pub deleted_at: DateTime<Utc>,
}

impl TombstoneUser {
	/// Create a new tombstone for the given user.
	pub fn new(original_user_id: UserId) -> Self {
		Self {
			original_user_id,
			tombstone_id: create_tombstone_id(&original_user_id),
			deleted_at: Utc::now(),
		}
	}
}

/// Returns true if the deletion request is still within the grace period.
pub fn can_restore(request: &DeletionRequest) -> bool {
	!request.is_restored() && !is_past_grace_period(request)
}

/// Returns true if the deletion request is past the grace period.
pub fn is_past_grace_period(request: &DeletionRequest) -> bool {
	Utc::now() >= request.hard_delete_at
}

/// Create a tombstone ID for a deleted user.
pub fn create_tombstone_id(user_id: &UserId) -> String {
	format!("deleted-user-{}", user_id.as_uuid())
}

#[cfg(test)]
mod tests {
	use super::*;

	mod deletion_request {
		use super::*;

		#[test]
		fn new_sets_hard_delete_90_days_later() {
			let user_id = UserId::generate();
			let request = DeletionRequest::new(user_id);

			let expected_hard_delete = request.requested_at + Duration::days(90);
			assert_eq!(request.hard_delete_at, expected_hard_delete);
			assert!(request.restored_at.is_none());
		}

		#[test]
		fn restore_sets_restored_at() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);

			assert!(!request.is_restored());
			request.restore();
			assert!(request.is_restored());
			assert!(request.restored_at.is_some());
		}

		#[test]
		fn should_purge_returns_false_when_restored() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.hard_delete_at = Utc::now() - Duration::days(1);
			request.restore();

			assert!(!request.should_purge());
		}

		#[test]
		fn should_purge_returns_false_within_grace_period() {
			let user_id = UserId::generate();
			let request = DeletionRequest::new(user_id);

			assert!(!request.should_purge());
		}

		#[test]
		fn should_purge_returns_true_when_past_grace_period() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.hard_delete_at = Utc::now() - Duration::days(1);

			assert!(request.should_purge());
		}

		#[test]
		fn days_until_purge_returns_remaining_days() {
			let user_id = UserId::generate();
			let request = DeletionRequest::new(user_id);

			let days = request.days_until_purge().unwrap();
			assert!((89..=90).contains(&days));
		}

		#[test]
		fn days_until_purge_returns_none_when_restored() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.restore();

			assert!(request.days_until_purge().is_none());
		}

		#[test]
		fn days_until_purge_returns_none_when_past_grace() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.hard_delete_at = Utc::now() - Duration::days(1);

			assert!(request.days_until_purge().is_none());
		}
	}

	mod tombstone_user {
		use super::*;

		#[test]
		fn new_creates_tombstone_with_correct_id() {
			let user_id = UserId::generate();
			let tombstone = TombstoneUser::new(user_id);

			assert_eq!(tombstone.original_user_id, user_id);
			assert_eq!(
				tombstone.tombstone_id,
				format!("deleted-user-{}", user_id.as_uuid())
			);
		}
	}

	mod helper_functions {
		use super::*;

		#[test]
		fn can_restore_returns_true_within_grace_period() {
			let user_id = UserId::generate();
			let request = DeletionRequest::new(user_id);

			assert!(can_restore(&request));
		}

		#[test]
		fn can_restore_returns_false_when_past_grace() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.hard_delete_at = Utc::now() - Duration::days(1);

			assert!(!can_restore(&request));
		}

		#[test]
		fn can_restore_returns_false_when_already_restored() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.restore();

			assert!(!can_restore(&request));
		}

		#[test]
		fn is_past_grace_period_returns_false_within_grace() {
			let user_id = UserId::generate();
			let request = DeletionRequest::new(user_id);

			assert!(!is_past_grace_period(&request));
		}

		#[test]
		fn is_past_grace_period_returns_true_after_grace() {
			let user_id = UserId::generate();
			let mut request = DeletionRequest::new(user_id);
			request.hard_delete_at = Utc::now() - Duration::days(1);

			assert!(is_past_grace_period(&request));
		}

		#[test]
		fn create_tombstone_id_formats_correctly() {
			let uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
			let user_id = UserId::new(uuid);

			assert_eq!(
				create_tombstone_id(&user_id),
				"deleted-user-550e8400-e29b-41d4-a716-446655440000"
			);
		}
	}
}
