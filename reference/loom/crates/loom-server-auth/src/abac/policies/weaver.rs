// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver access policies.

use crate::abac::{Action, ResourceAttrs, SubjectAttrs};

/// Evaluates weaver access policies.
///
/// Access rules:
/// - System admins have full access to all weavers
/// - Owners have full access to their own weavers
/// - Support users have read-only access to all weavers (Read action only)
pub fn evaluate(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	if subject.is_system_admin() {
		return true;
	}

	if let Some(owner_id) = &resource.owner_user_id {
		if subject.user_id == *owner_id {
			return true;
		}
	}

	if subject.is_support() && action == Action::Read {
		return true;
	}

	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::abac::ResourceAttrs;
	use crate::UserId;
	use uuid::Uuid;

	fn test_user_id() -> UserId {
		UserId::new(Uuid::new_v4())
	}

	#[test]
	fn owner_has_full_access() {
		let user_id = test_user_id();
		let subject = SubjectAttrs::new(user_id);
		let resource = ResourceAttrs::weaver(user_id);

		assert!(evaluate(&subject, Action::Read, &resource));
		assert!(evaluate(&subject, Action::Write, &resource));
		assert!(evaluate(&subject, Action::Delete, &resource));
	}

	#[test]
	fn non_owner_cannot_access() {
		let owner_id = test_user_id();
		let subject = SubjectAttrs::new(test_user_id());
		let resource = ResourceAttrs::weaver(owner_id);

		assert!(!evaluate(&subject, Action::Read, &resource));
		assert!(!evaluate(&subject, Action::Write, &resource));
		assert!(!evaluate(&subject, Action::Delete, &resource));
	}

	#[test]
	fn system_admin_has_full_access() {
		use crate::GlobalRole;

		let owner_id = test_user_id();
		let mut subject = SubjectAttrs::new(test_user_id());
		subject.global_roles.push(GlobalRole::SystemAdmin);
		let resource = ResourceAttrs::weaver(owner_id);

		assert!(evaluate(&subject, Action::Read, &resource));
		assert!(evaluate(&subject, Action::Write, &resource));
		assert!(evaluate(&subject, Action::Delete, &resource));
	}

	#[test]
	fn support_has_read_only_access() {
		use crate::GlobalRole;

		let owner_id = test_user_id();
		let mut subject = SubjectAttrs::new(test_user_id());
		subject.global_roles.push(GlobalRole::Support);
		let resource = ResourceAttrs::weaver(owner_id);

		assert!(evaluate(&subject, Action::Read, &resource));
		assert!(!evaluate(&subject, Action::Write, &resource));
		assert!(!evaluate(&subject, Action::Delete, &resource));
	}
}
