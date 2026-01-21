// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! LLM and tool access policies.
//!
//! Per the spec: "All or nothing - if user can access LLM, they can use all tools.
//! All org members have LLM access."

use crate::abac::{Action, ResourceAttrs, SubjectAttrs};

/// Evaluates LLM access policies.
pub fn evaluate_llm(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let Some(org_id) = resource.org_id else {
		return false;
	};

	match action {
		Action::UseLlm | Action::Read => subject.is_org_member(org_id),
		_ => false,
	}
}

/// Evaluates tool access policies.
pub fn evaluate_tool(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let Some(org_id) = resource.org_id else {
		return false;
	};

	match action {
		Action::UseTool | Action::Read => subject.is_org_member(org_id),
		_ => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::abac::OrgMembershipAttr;
	use crate::{OrgId, OrgRole, UserId};
	use uuid::Uuid;

	fn test_user_id() -> UserId {
		UserId::new(Uuid::new_v4())
	}

	fn test_org_id() -> OrgId {
		OrgId::new(Uuid::new_v4())
	}

	mod llm_policies {
		use super::*;

		#[test]
		fn org_owner_can_use_llm() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Owner,
			});
			let resource = ResourceAttrs::llm(org_id);
			assert!(evaluate_llm(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn org_admin_can_use_llm() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::llm(org_id);
			assert!(evaluate_llm(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn org_member_can_use_llm() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::llm(org_id);
			assert!(evaluate_llm(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn non_member_cannot_use_llm() {
			let org_id = test_org_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::llm(org_id);
			assert!(!evaluate_llm(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn member_of_different_org_cannot_use_llm() {
			let org_id = test_org_id();
			let other_org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id: other_org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::llm(org_id);
			assert!(!evaluate_llm(&subject, Action::UseLlm, &resource));
		}

		#[test]
		fn missing_org_id_denies_access() {
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id: test_org_id(),
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs {
				resource_type: crate::abac::ResourceType::Llm,
				owner_user_id: None,
				org_id: None,
				team_id: None,
				visibility: crate::Visibility::Private,
				is_shared_with_support: false,
			};
			assert!(!evaluate_llm(&subject, Action::UseLlm, &resource));
		}
	}

	mod tool_policies {
		use super::*;

		#[test]
		fn org_member_can_use_tools() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::tool(org_id);
			assert!(evaluate_tool(&subject, Action::UseTool, &resource));
		}

		#[test]
		fn non_member_cannot_use_tools() {
			let org_id = test_org_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::tool(org_id);
			assert!(!evaluate_tool(&subject, Action::UseTool, &resource));
		}

		#[test]
		fn llm_and_tool_access_are_coupled() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});

			let llm_resource = ResourceAttrs::llm(org_id);
			let tool_resource = ResourceAttrs::tool(org_id);

			let can_use_llm = evaluate_llm(&subject, Action::UseLlm, &llm_resource);
			let can_use_tool = evaluate_tool(&subject, Action::UseTool, &tool_resource);

			assert_eq!(can_use_llm, can_use_tool);
		}
	}
}
