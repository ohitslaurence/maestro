// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Thread access policies.

use crate::abac::{Action, ResourceAttrs, SubjectAttrs};
use crate::Visibility;

/// Evaluates thread access policies.
pub fn evaluate(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let is_owner = resource
		.owner_user_id
		.map(|id| id == subject.user_id)
		.unwrap_or(false);

	if is_owner {
		return true;
	}

	match action {
		Action::Read => can_read(subject, resource),
		Action::Write | Action::Delete => can_write(subject, resource),
		Action::Share => false,
		_ => false,
	}
}

fn can_read(subject: &SubjectAttrs, resource: &ResourceAttrs) -> bool {
	if resource.is_shared_with_support && subject.is_support() {
		return true;
	}

	match resource.visibility {
		Visibility::Public => true,
		Visibility::Organization => {
			if let Some(org_id) = resource.org_id {
				subject.is_org_member(org_id)
			} else {
				false
			}
		}
		Visibility::Team => {
			if let Some(team_id) = resource.team_id {
				subject.is_team_member(team_id)
			} else {
				false
			}
		}
		Visibility::Private => false,
	}
}

fn can_write(subject: &SubjectAttrs, resource: &ResourceAttrs) -> bool {
	if let Some(org_id) = resource.org_id {
		subject.is_org_admin(org_id)
	} else {
		false
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::abac::{OrgMembershipAttr, TeamMembershipAttr};
	use crate::{OrgId, OrgRole, TeamId, TeamRole, UserId};
	use uuid::Uuid;

	fn test_user_id() -> UserId {
		UserId::new(Uuid::new_v4())
	}

	fn test_org_id() -> OrgId {
		OrgId::new(Uuid::new_v4())
	}

	fn test_team_id() -> TeamId {
		TeamId::new(Uuid::new_v4())
	}

	mod read_access {
		use super::*;
		use crate::GlobalRole;

		#[test]
		fn owner_can_read() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs::thread(user_id);
			assert!(evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn public_thread_readable_by_anyone() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(test_user_id()).with_visibility(Visibility::Public);
			assert!(evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn private_thread_not_readable_by_others() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(test_user_id()).with_visibility(Visibility::Private);
			assert!(!evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn org_thread_readable_by_org_members() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn org_thread_not_readable_by_non_members() {
			let org_id = test_org_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(!evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn team_thread_readable_by_team_members() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.team_memberships.push(TeamMembershipAttr {
				team_id,
				org_id,
				role: TeamRole::Member,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_team(team_id)
				.with_visibility(Visibility::Team);
			assert!(evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn team_thread_not_readable_by_non_team_members() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_team(team_id)
				.with_visibility(Visibility::Team);
			assert!(!evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn support_can_read_shared_threads() {
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.global_roles.push(GlobalRole::Support);
			let resource = ResourceAttrs::thread(test_user_id()).with_support_access(true);
			assert!(evaluate(&subject, Action::Read, &resource));
		}

		#[test]
		fn support_cannot_read_unshared_threads() {
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.global_roles.push(GlobalRole::Support);
			let resource = ResourceAttrs::thread(test_user_id());
			assert!(!evaluate(&subject, Action::Read, &resource));
		}
	}

	mod write_access {
		use super::*;

		#[test]
		fn owner_can_write() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs::thread(user_id);
			assert!(evaluate(&subject, Action::Write, &resource));
		}

		#[test]
		fn non_owner_cannot_write_private_thread() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(test_user_id());
			assert!(!evaluate(&subject, Action::Write, &resource));
		}

		#[test]
		fn org_admin_can_write_org_threads() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(evaluate(&subject, Action::Write, &resource));
		}

		#[test]
		fn org_member_cannot_write_org_threads() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(!evaluate(&subject, Action::Write, &resource));
		}
	}

	mod delete_access {
		use super::*;

		#[test]
		fn owner_can_delete() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs::thread(user_id);
			assert!(evaluate(&subject, Action::Delete, &resource));
		}

		#[test]
		fn org_admin_can_delete_org_threads() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(evaluate(&subject, Action::Delete, &resource));
		}
	}

	mod share_access {
		use super::*;

		#[test]
		fn owner_can_share() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs::thread(user_id);
			assert!(evaluate(&subject, Action::Share, &resource));
		}

		#[test]
		fn non_owner_cannot_share() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(test_user_id());
			assert!(!evaluate(&subject, Action::Share, &resource));
		}

		#[test]
		fn org_admin_cannot_share_others_threads() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::thread(test_user_id())
				.with_org(org_id)
				.with_visibility(Visibility::Organization);
			assert!(!evaluate(&subject, Action::Share, &resource));
		}
	}
}
