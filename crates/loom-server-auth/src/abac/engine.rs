// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! ABAC policy evaluation engine.
//!
//! This module contains the core [`is_allowed`] function that evaluates access
//! decisions. It implements a two-phase evaluation:
//!
//! 1. **Global role check**: SystemAdmin gets full access, Auditor gets read-only
//! 2. **Resource-specific policy**: Delegates to specialized policy modules
//!
//! All policy decisions are pure functions with no side effects, making them
//! easy to test and reason about.

use super::policies::{llm, org, thread, weaver};
use super::types::{Action, ResourceAttrs, ResourceType, SubjectAttrs};
use tracing::instrument;

/// Evaluates whether a subject is allowed to perform an action on a resource.
///
/// This is the main entry point for ABAC policy evaluation. It first checks
/// global role permissions, then delegates to resource-specific policy modules.
///
/// # Arguments
///
/// * `subject` - Attributes of the user making the request
/// * `action` - The operation being attempted
/// * `resource` - Attributes of the resource being accessed
///
/// # Returns
///
/// `true` if the action is allowed, `false` otherwise.
///
/// # Tracing
///
/// This function is instrumented with tracing. The decision and all relevant
/// attributes are logged at debug level for audit purposes.
#[instrument(
    level = "debug",
    skip(subject, resource),
    fields(
        user_id = %subject.user_id,
        action = ?action,
        resource_type = ?resource.resource_type,
    )
)]
pub fn is_allowed(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	if check_global_roles(subject, action, resource) {
		return true;
	}

	match resource.resource_type {
		ResourceType::Thread => thread::evaluate(subject, action, resource),
		ResourceType::Organization => org::evaluate_org(subject, action, resource),
		ResourceType::Team => org::evaluate_team(subject, action, resource),
		ResourceType::ApiKey => org::evaluate_api_key(subject, action, resource),
		ResourceType::Llm => llm::evaluate_llm(subject, action, resource),
		ResourceType::Tool => llm::evaluate_tool(subject, action, resource),
		ResourceType::Workspace => evaluate_workspace(subject, action, resource),
		ResourceType::User => evaluate_user(subject, action, resource),
		ResourceType::Weaver => weaver::evaluate(subject, action, resource),
	}
}

/// Checks global role permissions that apply across all resources.
fn check_global_roles(subject: &SubjectAttrs, action: Action, _resource: &ResourceAttrs) -> bool {
	if subject.is_system_admin() {
		return true;
	}

	if subject.is_auditor() {
		return matches!(action, Action::Read);
	}

	false
}

/// Evaluates workspace access policies.
fn evaluate_workspace(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let is_owner = resource
		.owner_user_id
		.map(|id| id == subject.user_id)
		.unwrap_or(false);

	if is_owner {
		return true;
	}

	match action {
		Action::Read => {
			if let Some(org_id) = resource.org_id {
				if subject.is_org_member(org_id) {
					return true;
				}
			}
			false
		}
		Action::Write | Action::Delete => {
			if let Some(org_id) = resource.org_id {
				if subject.is_org_admin(org_id) {
					return true;
				}
			}
			false
		}
		_ => false,
	}
}

/// Evaluates user resource access policies.
fn evaluate_user(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let is_self = resource
		.owner_user_id
		.map(|id| id == subject.user_id)
		.unwrap_or(false);

	match action {
		Action::Read => true,
		Action::Write | Action::Delete => is_self,
		Action::Impersonate => subject.is_system_admin(),
		_ => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{GlobalRole, OrgId, OrgRole, TeamId, TeamRole, UserId, Visibility};
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

	fn subject_with_global_role(role: GlobalRole) -> SubjectAttrs {
		let mut subject = SubjectAttrs::new(test_user_id());
		subject.global_roles.push(role);
		subject
	}

	fn subject_with_org_role(org_id: OrgId, role: OrgRole) -> SubjectAttrs {
		let mut subject = SubjectAttrs::new(test_user_id());
		subject
			.org_memberships
			.push(crate::abac::OrgMembershipAttr { org_id, role });
		subject
	}

	fn subject_with_team_role(team_id: TeamId, org_id: OrgId, role: TeamRole) -> SubjectAttrs {
		let mut subject = SubjectAttrs::new(test_user_id());
		subject
			.org_memberships
			.push(crate::abac::OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
		subject
			.team_memberships
			.push(crate::abac::TeamMembershipAttr {
				team_id,
				org_id,
				role,
			});
		subject
	}

	mod global_roles {
		use super::*;

		#[test]
		fn system_admin_has_full_access() {
			let subject = subject_with_global_role(GlobalRole::SystemAdmin);
			let resource = ResourceAttrs::thread(test_user_id());

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(is_allowed(&subject, Action::Write, &resource));
			assert!(is_allowed(&subject, Action::Delete, &resource));
			assert!(is_allowed(&subject, Action::Share, &resource));
		}

		#[test]
		fn auditor_has_read_only_access() {
			let subject = subject_with_global_role(GlobalRole::Auditor);
			let resource = ResourceAttrs::thread(test_user_id());

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
			assert!(!is_allowed(&subject, Action::Delete, &resource));
			assert!(!is_allowed(&subject, Action::Share, &resource));
		}

		#[test]
		fn support_role_alone_does_not_grant_access() {
			let subject = subject_with_global_role(GlobalRole::Support);
			let resource = ResourceAttrs::thread(test_user_id());

			assert!(!is_allowed(&subject, Action::Read, &resource));
		}

		#[test]
		fn support_role_with_shared_thread_grants_read() {
			let subject = subject_with_global_role(GlobalRole::Support);
			let resource = ResourceAttrs::thread(test_user_id()).with_support_access(true);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}
	}

	mod thread_access {
		use super::*;

		#[test]
		fn owner_has_full_access() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs::thread(user_id);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(is_allowed(&subject, Action::Write, &resource));
			assert!(is_allowed(&subject, Action::Delete, &resource));
			assert!(is_allowed(&subject, Action::Share, &resource));
		}

		#[test]
		fn non_owner_cannot_access_private_thread() {
			let owner_id = test_user_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(owner_id);

			assert!(!is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}

		#[test]
		fn public_thread_is_readable_by_anyone() {
			let owner_id = test_user_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::thread(owner_id).with_visibility(Visibility::Public);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}

		#[test]
		fn org_thread_readable_by_org_members() {
			let owner_id = test_user_id();
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Member);
			let resource = ResourceAttrs::thread(owner_id)
				.with_org(org_id)
				.with_visibility(Visibility::Organization);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}

		#[test]
		fn team_thread_readable_by_team_members() {
			let owner_id = test_user_id();
			let org_id = test_org_id();
			let team_id = test_team_id();
			let subject = subject_with_team_role(team_id, org_id, TeamRole::Member);
			let resource = ResourceAttrs::thread(owner_id)
				.with_org(org_id)
				.with_team(team_id)
				.with_visibility(Visibility::Team);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}

		#[test]
		fn org_admin_can_manage_org_threads() {
			let owner_id = test_user_id();
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Admin);
			let resource = ResourceAttrs::thread(owner_id)
				.with_org(org_id)
				.with_visibility(Visibility::Organization);

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(is_allowed(&subject, Action::Write, &resource));
			assert!(is_allowed(&subject, Action::Delete, &resource));
		}
	}

	mod org_access {
		use super::*;

		#[test]
		fn org_owner_can_manage() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Owner);
			let resource = ResourceAttrs::organization(org_id);

			assert!(is_allowed(&subject, Action::ManageOrg, &resource));
			assert!(is_allowed(&subject, Action::ManageApiKeys, &resource));
		}

		#[test]
		fn org_admin_can_manage() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Admin);
			let resource = ResourceAttrs::organization(org_id);

			assert!(is_allowed(&subject, Action::ManageOrg, &resource));
			assert!(is_allowed(&subject, Action::ManageApiKeys, &resource));
		}

		#[test]
		fn org_member_cannot_manage() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Member);
			let resource = ResourceAttrs::organization(org_id);

			assert!(!is_allowed(&subject, Action::ManageOrg, &resource));
			assert!(!is_allowed(&subject, Action::ManageApiKeys, &resource));
		}
	}

	mod team_access {
		use super::*;

		#[test]
		fn team_maintainer_can_manage() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let subject = subject_with_team_role(team_id, org_id, TeamRole::Maintainer);
			let resource = ResourceAttrs::team(team_id, org_id);

			assert!(is_allowed(&subject, Action::ManageTeam, &resource));
		}

		#[test]
		fn team_member_cannot_manage() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let subject = subject_with_team_role(team_id, org_id, TeamRole::Member);
			let resource = ResourceAttrs::team(team_id, org_id);

			assert!(!is_allowed(&subject, Action::ManageTeam, &resource));
		}

		#[test]
		fn org_admin_can_manage_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let subject = subject_with_org_role(org_id, OrgRole::Admin);
			let resource = ResourceAttrs::team(team_id, org_id);

			assert!(is_allowed(&subject, Action::ManageTeam, &resource));
		}
	}

	mod llm_access {
		use super::*;

		#[test]
		fn org_member_can_use_llm() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Member);
			let resource = ResourceAttrs::llm(org_id);

			assert!(is_allowed(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn org_member_can_use_tools() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Member);
			let resource = ResourceAttrs::tool(org_id);

			assert!(is_allowed(&subject, Action::UseTool, &resource));
		}

		#[test]
		fn non_member_cannot_use_llm() {
			let org_id = test_org_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::llm(org_id);

			assert!(!is_allowed(&subject, Action::UseLlm, &resource));
		}
	}

	mod workspace_access {
		use super::*;

		#[test]
		fn owner_has_full_access() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs {
				resource_type: ResourceType::Workspace,
				owner_user_id: Some(user_id),
				org_id: None,
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(is_allowed(&subject, Action::Write, &resource));
			assert!(is_allowed(&subject, Action::Delete, &resource));
		}

		#[test]
		fn org_member_can_read() {
			let org_id = test_org_id();
			let subject = subject_with_org_role(org_id, OrgRole::Member);
			let resource = ResourceAttrs {
				resource_type: ResourceType::Workspace,
				owner_user_id: Some(test_user_id()),
				org_id: Some(org_id),
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(is_allowed(&subject, Action::Read, &resource));
			assert!(!is_allowed(&subject, Action::Write, &resource));
		}
	}

	mod user_access {
		use super::*;

		#[test]
		fn anyone_can_read_user_profiles() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs {
				resource_type: ResourceType::User,
				owner_user_id: Some(test_user_id()),
				org_id: None,
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(is_allowed(&subject, Action::Read, &resource));
		}

		#[test]
		fn user_can_modify_self() {
			let user_id = test_user_id();
			let subject = SubjectAttrs::new(user_id);
			let resource = ResourceAttrs {
				resource_type: ResourceType::User,
				owner_user_id: Some(user_id),
				org_id: None,
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(is_allowed(&subject, Action::Write, &resource));
			assert!(is_allowed(&subject, Action::Delete, &resource));
		}

		#[test]
		fn cannot_modify_other_users() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs {
				resource_type: ResourceType::User,
				owner_user_id: Some(test_user_id()),
				org_id: None,
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(!is_allowed(&subject, Action::Write, &resource));
			assert!(!is_allowed(&subject, Action::Delete, &resource));
		}

		#[test]
		fn only_system_admin_can_impersonate() {
			let admin = subject_with_global_role(GlobalRole::SystemAdmin);
			let regular = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs {
				resource_type: ResourceType::User,
				owner_user_id: Some(test_user_id()),
				org_id: None,
				team_id: None,
				visibility: Visibility::Private,
				is_shared_with_support: false,
			};

			assert!(is_allowed(&admin, Action::Impersonate, &resource));
			assert!(!is_allowed(&regular, Action::Impersonate, &resource));
		}
	}

	mod property_tests {
		use super::*;
		use proptest::prelude::*;

		fn arb_visibility() -> impl Strategy<Value = Visibility> {
			prop_oneof![
				Just(Visibility::Private),
				Just(Visibility::Team),
				Just(Visibility::Organization),
				Just(Visibility::Public),
			]
		}

		fn arb_org_role() -> impl Strategy<Value = OrgRole> {
			prop_oneof![
				Just(OrgRole::Owner),
				Just(OrgRole::Admin),
				Just(OrgRole::Member),
			]
		}

		proptest! {
				#[test]
				fn system_admin_can_do_anything(
						subject_uuid in any::<u128>(),
						resource_uuid in any::<u128>(),
						visibility in arb_visibility(),
				) {
						let mut subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						subject.global_roles.push(GlobalRole::SystemAdmin);

						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(resource_uuid)))
								.with_visibility(visibility);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
						prop_assert!(is_allowed(&subject, Action::Write, &resource));
						prop_assert!(is_allowed(&subject, Action::Delete, &resource));
						prop_assert!(is_allowed(&subject, Action::Share, &resource));
				}

				#[test]
				fn owner_always_has_full_access_to_owned_thread(
						user_uuid in any::<u128>(),
						visibility in arb_visibility(),
				) {
						let user_id = UserId::new(Uuid::from_u128(user_uuid));
						let subject = SubjectAttrs::new(user_id);
						let resource = ResourceAttrs::thread(user_id).with_visibility(visibility);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
						prop_assert!(is_allowed(&subject, Action::Write, &resource));
						prop_assert!(is_allowed(&subject, Action::Delete, &resource));
						prop_assert!(is_allowed(&subject, Action::Share, &resource));
				}

				#[test]
				fn auditor_can_only_read(
						subject_uuid in any::<u128>(),
						resource_uuid in any::<u128>(),
						visibility in arb_visibility(),
				) {
						let mut subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						subject.global_roles.push(GlobalRole::Auditor);

						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(resource_uuid)))
								.with_visibility(visibility);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
						prop_assert!(!is_allowed(&subject, Action::Write, &resource));
						prop_assert!(!is_allowed(&subject, Action::Delete, &resource));
				}

				#[test]
				fn public_resources_are_readable_by_anyone(
						subject_uuid in any::<u128>(),
						owner_uuid in any::<u128>(),
				) {
						let subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(owner_uuid)))
								.with_visibility(Visibility::Public);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
				}

				#[test]
				fn private_resources_not_readable_by_non_owners(
						subject_uuid in any::<u128>(),
						owner_uuid in any::<u128>(),
				) {
						prop_assume!(subject_uuid != owner_uuid);

						let subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(owner_uuid)))
								.with_visibility(Visibility::Private);

						prop_assert!(!is_allowed(&subject, Action::Read, &resource));
				}

				#[test]
				fn org_members_can_read_org_visible_threads(
						subject_uuid in any::<u128>(),
						owner_uuid in any::<u128>(),
						org_uuid in any::<u128>(),
						role in arb_org_role(),
				) {
						let org_id = OrgId::new(Uuid::from_u128(org_uuid));
						let mut subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						subject.org_memberships.push(crate::abac::OrgMembershipAttr { org_id, role });

						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(owner_uuid)))
								.with_org(org_id)
								.with_visibility(Visibility::Organization);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
				}

				#[test]
				fn org_admins_can_manage_org_threads(
						subject_uuid in any::<u128>(),
						owner_uuid in any::<u128>(),
						org_uuid in any::<u128>(),
				) {
						let org_id = OrgId::new(Uuid::from_u128(org_uuid));
						let mut subject = SubjectAttrs::new(UserId::new(Uuid::from_u128(subject_uuid)));
						subject.org_memberships.push(crate::abac::OrgMembershipAttr {
								org_id,
								role: OrgRole::Admin,
						});

						let resource = ResourceAttrs::thread(UserId::new(Uuid::from_u128(owner_uuid)))
								.with_org(org_id)
								.with_visibility(Visibility::Organization);

						prop_assert!(is_allowed(&subject, Action::Read, &resource));
						prop_assert!(is_allowed(&subject, Action::Write, &resource));
						prop_assert!(is_allowed(&subject, Action::Delete, &resource));
				}
		}
	}
}
