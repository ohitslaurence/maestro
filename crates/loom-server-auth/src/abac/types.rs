// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Type definitions for ABAC policy evaluation.
//!
//! This module defines the core data structures for Attribute-Based Access Control:
//!
//! - [`SubjectAttrs`]: Describes the user making a request (their memberships and roles)
//! - [`ResourceAttrs`]: Describes the resource being accessed (type, owner, visibility)
//! - [`Action`]: The operation being performed (read, write, delete, etc.)
//!
//! # Design Principles
//!
//! 1. **Immutable evaluation**: All attributes are computed before policy evaluation
//! 2. **No database access**: Policy functions are pure; all data is pre-loaded
//! 3. **Explicit attributes**: Every relevant fact is an explicit field, not derived
//! 4. **Serializable**: All types can be logged/audited as JSON

use crate::{GlobalRole, OrgId, OrgRole, TeamId, TeamRole, UserId, Visibility};
use serde::{Deserialize, Serialize};

/// Attributes describing the subject (user) requesting access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectAttrs {
	pub user_id: UserId,
	pub org_memberships: Vec<OrgMembershipAttr>,
	pub team_memberships: Vec<TeamMembershipAttr>,
	pub global_roles: Vec<GlobalRole>,
}

impl SubjectAttrs {
	/// Creates a new subject with no memberships or roles.
	pub fn new(user_id: UserId) -> Self {
		Self {
			user_id,
			org_memberships: Vec::new(),
			team_memberships: Vec::new(),
			global_roles: Vec::new(),
		}
	}

	/// Returns true if the subject has the given global role.
	pub fn has_global_role(&self, role: GlobalRole) -> bool {
		self.global_roles.contains(&role)
	}

	/// Returns true if the subject is a system administrator.
	pub fn is_system_admin(&self) -> bool {
		self.has_global_role(GlobalRole::SystemAdmin)
	}

	/// Returns true if the subject has the support role.
	pub fn is_support(&self) -> bool {
		self.has_global_role(GlobalRole::Support)
	}

	/// Returns true if the subject has the auditor role.
	pub fn is_auditor(&self) -> bool {
		self.has_global_role(GlobalRole::Auditor)
	}

	/// Returns the role for the given organization, if any.
	pub fn org_role(&self, org_id: OrgId) -> Option<OrgRole> {
		self
			.org_memberships
			.iter()
			.find(|m| m.org_id == org_id)
			.map(|m| m.role)
	}

	/// Returns true if the subject is a member of the given organization.
	pub fn is_org_member(&self, org_id: OrgId) -> bool {
		self.org_role(org_id).is_some()
	}

	/// Returns true if the subject is an owner or admin of the given organization.
	pub fn is_org_admin(&self, org_id: OrgId) -> bool {
		matches!(
			self.org_role(org_id),
			Some(OrgRole::Owner) | Some(OrgRole::Admin)
		)
	}

	/// Returns true if the subject is the owner of the given organization.
	pub fn is_org_owner(&self, org_id: OrgId) -> bool {
		matches!(self.org_role(org_id), Some(OrgRole::Owner))
	}

	/// Returns the role for the given team, if any.
	pub fn team_role(&self, team_id: TeamId) -> Option<TeamRole> {
		self
			.team_memberships
			.iter()
			.find(|m| m.team_id == team_id)
			.map(|m| m.role)
	}

	/// Returns true if the subject is a member of the given team.
	pub fn is_team_member(&self, team_id: TeamId) -> bool {
		self.team_role(team_id).is_some()
	}

	/// Returns true if the subject is a maintainer of the given team.
	pub fn is_team_maintainer(&self, team_id: TeamId) -> bool {
		matches!(self.team_role(team_id), Some(TeamRole::Maintainer))
	}

	/// Returns the organization ID for a team membership, if the subject is a member.
	pub fn team_org_id(&self, team_id: TeamId) -> Option<OrgId> {
		self
			.team_memberships
			.iter()
			.find(|m| m.team_id == team_id)
			.map(|m| m.org_id)
	}
}

/// Organization membership attribute for ABAC evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgMembershipAttr {
	pub org_id: OrgId,
	pub role: OrgRole,
}

/// Team membership attribute for ABAC evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMembershipAttr {
	pub team_id: TeamId,
	pub org_id: OrgId,
	pub role: TeamRole,
}

/// Attributes describing the resource being accessed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAttrs {
	pub resource_type: ResourceType,
	pub owner_user_id: Option<UserId>,
	pub org_id: Option<OrgId>,
	pub team_id: Option<TeamId>,
	pub visibility: Visibility,
	pub is_shared_with_support: bool,
}

impl ResourceAttrs {
	/// Creates resource attributes for a thread.
	pub fn thread(owner_user_id: UserId) -> Self {
		Self {
			resource_type: ResourceType::Thread,
			owner_user_id: Some(owner_user_id),
			org_id: None,
			team_id: None,
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Creates resource attributes for an organization.
	pub fn organization(org_id: OrgId) -> Self {
		Self {
			resource_type: ResourceType::Organization,
			owner_user_id: None,
			org_id: Some(org_id),
			team_id: None,
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Creates resource attributes for a team.
	pub fn team(team_id: TeamId, org_id: OrgId) -> Self {
		Self {
			resource_type: ResourceType::Team,
			owner_user_id: None,
			org_id: Some(org_id),
			team_id: Some(team_id),
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Creates resource attributes for LLM access scoped to an organization.
	pub fn llm(org_id: OrgId) -> Self {
		Self {
			resource_type: ResourceType::Llm,
			owner_user_id: None,
			org_id: Some(org_id),
			team_id: None,
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Creates resource attributes for a tool scoped to an organization.
	pub fn tool(org_id: OrgId) -> Self {
		Self {
			resource_type: ResourceType::Tool,
			owner_user_id: None,
			org_id: Some(org_id),
			team_id: None,
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Creates resource attributes for a weaver.
	pub fn weaver(owner_user_id: UserId) -> Self {
		Self {
			resource_type: ResourceType::Weaver,
			owner_user_id: Some(owner_user_id),
			org_id: None,
			team_id: None,
			visibility: Visibility::Private,
			is_shared_with_support: false,
		}
	}

	/// Builder: set org_id.
	pub fn with_org(mut self, org_id: OrgId) -> Self {
		self.org_id = Some(org_id);
		self
	}

	/// Builder: set team_id.
	pub fn with_team(mut self, team_id: TeamId) -> Self {
		self.team_id = Some(team_id);
		self
	}

	/// Builder: set visibility.
	pub fn with_visibility(mut self, visibility: Visibility) -> Self {
		self.visibility = visibility;
		self
	}

	/// Builder: set is_shared_with_support.
	pub fn with_support_access(mut self, shared: bool) -> Self {
		self.is_shared_with_support = shared;
		self
	}
}

/// Types of resources that can be protected by ABAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
	Thread,
	Workspace,
	Tool,
	Organization,
	Team,
	User,
	ApiKey,
	Llm,
	Weaver,
}

/// Actions that can be performed on resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
	Read,
	Write,
	Delete,
	Share,
	UseTool,
	UseLlm,
	ManageOrg,
	ManageApiKeys,
	ManageTeam,
	Impersonate,
}

#[cfg(test)]
mod tests {
	use super::*;
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

	#[test]
	fn subject_attrs_new_has_no_roles() {
		let subject = SubjectAttrs::new(test_user_id());
		assert!(subject.org_memberships.is_empty());
		assert!(subject.team_memberships.is_empty());
		assert!(subject.global_roles.is_empty());
	}

	#[test]
	fn subject_attrs_has_global_role() {
		let mut subject = SubjectAttrs::new(test_user_id());
		subject.global_roles.push(GlobalRole::SystemAdmin);
		assert!(subject.has_global_role(GlobalRole::SystemAdmin));
		assert!(subject.is_system_admin());
		assert!(!subject.is_support());
		assert!(!subject.is_auditor());
	}

	#[test]
	fn subject_attrs_org_membership() {
		let mut subject = SubjectAttrs::new(test_user_id());
		let org_id = test_org_id();
		subject.org_memberships.push(OrgMembershipAttr {
			org_id,
			role: OrgRole::Admin,
		});
		assert!(subject.is_org_member(org_id));
		assert!(subject.is_org_admin(org_id));
		assert!(!subject.is_org_owner(org_id));
		assert_eq!(subject.org_role(org_id), Some(OrgRole::Admin));
	}

	#[test]
	fn subject_attrs_team_membership() {
		let mut subject = SubjectAttrs::new(test_user_id());
		let org_id = test_org_id();
		let team_id = test_team_id();
		subject.team_memberships.push(TeamMembershipAttr {
			team_id,
			org_id,
			role: TeamRole::Maintainer,
		});
		assert!(subject.is_team_member(team_id));
		assert!(subject.is_team_maintainer(team_id));
		assert_eq!(subject.team_role(team_id), Some(TeamRole::Maintainer));
		assert_eq!(subject.team_org_id(team_id), Some(org_id));
	}

	#[test]
	fn resource_attrs_thread_builder() {
		let user_id = test_user_id();
		let org_id = test_org_id();
		let resource = ResourceAttrs::thread(user_id)
			.with_org(org_id)
			.with_visibility(Visibility::Organization)
			.with_support_access(true);
		assert_eq!(resource.resource_type, ResourceType::Thread);
		assert_eq!(resource.owner_user_id, Some(user_id));
		assert_eq!(resource.org_id, Some(org_id));
		assert_eq!(resource.visibility, Visibility::Organization);
		assert!(resource.is_shared_with_support);
	}

	#[test]
	fn resource_attrs_organization() {
		let org_id = test_org_id();
		let resource = ResourceAttrs::organization(org_id);
		assert_eq!(resource.resource_type, ResourceType::Organization);
		assert_eq!(resource.org_id, Some(org_id));
	}

	#[test]
	fn resource_attrs_team() {
		let org_id = test_org_id();
		let team_id = test_team_id();
		let resource = ResourceAttrs::team(team_id, org_id);
		assert_eq!(resource.resource_type, ResourceType::Team);
		assert_eq!(resource.org_id, Some(org_id));
		assert_eq!(resource.team_id, Some(team_id));
	}
}
