// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Organization and team access policies.

use crate::abac::{Action, ResourceAttrs, SubjectAttrs};

/// Evaluates organization access policies.
pub fn evaluate_org(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let Some(org_id) = resource.org_id else {
		return false;
	};

	match action {
		Action::Read => subject.is_org_member(org_id),
		Action::Delete => subject.is_org_owner(org_id),
		Action::ManageOrg | Action::ManageApiKeys | Action::Write => subject.is_org_admin(org_id),
		_ => false,
	}
}

/// Evaluates API key access policies.
pub fn evaluate_api_key(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let Some(org_id) = resource.org_id else {
		return false;
	};

	match action {
		Action::Read | Action::ManageApiKeys | Action::Write | Action::Delete => {
			subject.is_org_admin(org_id)
		}
		_ => false,
	}
}

/// Evaluates team access policies.
pub fn evaluate_team(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
	let Some(org_id) = resource.org_id else {
		return false;
	};

	let team_id = resource.team_id;

	match action {
		Action::Read => {
			if subject.is_org_member(org_id) {
				return true;
			}
			if let Some(tid) = team_id {
				return subject.is_team_member(tid);
			}
			false
		}
		Action::ManageTeam | Action::Write | Action::Delete => {
			if subject.is_org_admin(org_id) {
				return true;
			}
			if let Some(tid) = team_id {
				return subject.is_team_maintainer(tid);
			}
			false
		}
		_ => false,
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

	mod org_policies {
		use super::*;

		#[test]
		fn owner_can_manage_org() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Owner,
			});
			let resource = ResourceAttrs::organization(org_id);
			assert!(evaluate_org(&subject, Action::ManageOrg, &resource));
			assert!(evaluate_org(&subject, Action::Write, &resource));
			assert!(evaluate_org(&subject, Action::Delete, &resource));
		}

		#[test]
		fn admin_can_manage_org() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::organization(org_id);
			assert!(evaluate_org(&subject, Action::ManageOrg, &resource));
			assert!(evaluate_org(&subject, Action::Write, &resource));
		}

		#[test]
		fn admin_cannot_delete_org() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::organization(org_id);
			assert!(!evaluate_org(&subject, Action::Delete, &resource));
		}

		#[test]
		fn member_cannot_manage_org() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::organization(org_id);
			assert!(!evaluate_org(&subject, Action::ManageOrg, &resource));
			assert!(!evaluate_org(&subject, Action::Write, &resource));
		}

		#[test]
		fn member_can_read_org() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::organization(org_id);
			assert!(evaluate_org(&subject, Action::Read, &resource));
		}

		#[test]
		fn non_member_cannot_read_org() {
			let org_id = test_org_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::organization(org_id);
			assert!(!evaluate_org(&subject, Action::Read, &resource));
		}

		#[test]
		fn missing_org_id_denies_access() {
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs {
				resource_type: crate::abac::ResourceType::Organization,
				owner_user_id: None,
				org_id: None,
				team_id: None,
				visibility: crate::Visibility::Private,
				is_shared_with_support: false,
			};
			assert!(!evaluate_org(&subject, Action::Read, &resource));
		}
	}

	mod api_key_policies {
		use super::*;

		#[test]
		fn owner_can_manage_api_keys() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Owner,
			});
			let resource = ResourceAttrs {
				resource_type: crate::abac::ResourceType::ApiKey,
				owner_user_id: None,
				org_id: Some(org_id),
				team_id: None,
				visibility: crate::Visibility::Private,
				is_shared_with_support: false,
			};
			assert!(evaluate_api_key(&subject, Action::ManageApiKeys, &resource));
			assert!(evaluate_api_key(&subject, Action::Read, &resource));
			assert!(evaluate_api_key(&subject, Action::Write, &resource));
			assert!(evaluate_api_key(&subject, Action::Delete, &resource));
		}

		#[test]
		fn admin_can_manage_api_keys() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs {
				resource_type: crate::abac::ResourceType::ApiKey,
				owner_user_id: None,
				org_id: Some(org_id),
				team_id: None,
				visibility: crate::Visibility::Private,
				is_shared_with_support: false,
			};
			assert!(evaluate_api_key(&subject, Action::ManageApiKeys, &resource));
		}

		#[test]
		fn member_cannot_manage_api_keys() {
			let org_id = test_org_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs {
				resource_type: crate::abac::ResourceType::ApiKey,
				owner_user_id: None,
				org_id: Some(org_id),
				team_id: None,
				visibility: crate::Visibility::Private,
				is_shared_with_support: false,
			};
			assert!(!evaluate_api_key(
				&subject,
				Action::ManageApiKeys,
				&resource
			));
			assert!(!evaluate_api_key(&subject, Action::Read, &resource));
		}
	}

	mod team_policies {
		use super::*;

		#[test]
		fn org_owner_can_manage_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Owner,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(evaluate_team(&subject, Action::ManageTeam, &resource));
			assert!(evaluate_team(&subject, Action::Write, &resource));
			assert!(evaluate_team(&subject, Action::Delete, &resource));
		}

		#[test]
		fn org_admin_can_manage_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Admin,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(evaluate_team(&subject, Action::ManageTeam, &resource));
		}

		#[test]
		fn team_maintainer_can_manage_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			subject.team_memberships.push(TeamMembershipAttr {
				team_id,
				org_id,
				role: TeamRole::Maintainer,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(evaluate_team(&subject, Action::ManageTeam, &resource));
		}

		#[test]
		fn team_member_cannot_manage_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			subject.team_memberships.push(TeamMembershipAttr {
				team_id,
				org_id,
				role: TeamRole::Member,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(!evaluate_team(&subject, Action::ManageTeam, &resource));
		}

		#[test]
		fn org_member_can_read_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.org_memberships.push(OrgMembershipAttr {
				org_id,
				role: OrgRole::Member,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(evaluate_team(&subject, Action::Read, &resource));
		}

		#[test]
		fn team_member_can_read_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let mut subject = SubjectAttrs::new(test_user_id());
			subject.team_memberships.push(TeamMembershipAttr {
				team_id,
				org_id,
				role: TeamRole::Member,
			});
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(evaluate_team(&subject, Action::Read, &resource));
		}

		#[test]
		fn non_member_cannot_read_team() {
			let org_id = test_org_id();
			let team_id = test_team_id();
			let subject = SubjectAttrs::new(test_user_id());
			let resource = ResourceAttrs::team(team_id, org_id);
			assert!(!evaluate_team(&subject, Action::Read, &resource));
		}
	}
}
