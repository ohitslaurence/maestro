// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Team management types and operations.
//!
//! This module provides:
//! - [`Team`] - sub-groups within an organization
//! - [`TeamMembership`] - links users to teams with roles

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{OrgId, TeamId, TeamRole, UserId};

/// A team within an organization.
///
/// Teams are sub-groups that allow finer-grained access control
/// and organization of members within an org.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
	/// Unique identifier for this team.
	pub id: TeamId,

	/// The organization this team belongs to.
	pub org_id: OrgId,

	/// Display name of the team.
	pub name: String,

	/// URL-friendly identifier for the team.
	pub slug: String,

	/// When the team was created.
	pub created_at: DateTime<Utc>,

	/// When the team was last updated.
	pub updated_at: DateTime<Utc>,
}

impl Team {
	/// Creates a new team with the given organization, name, and slug.
	///
	/// Generates a new team ID and sets timestamps to now.
	pub fn new(org_id: OrgId, name: impl Into<String>, slug: impl Into<String>) -> Self {
		let now = Utc::now();
		Self {
			id: TeamId::generate(),
			org_id,
			name: name.into(),
			slug: slug.into(),
			created_at: now,
			updated_at: now,
		}
	}
}

/// A user's membership in a team.
///
/// Defines the relationship between a user and a team,
/// including their role within that team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMembership {
	/// Unique identifier for this membership record.
	pub id: uuid::Uuid,

	/// The team this membership is for.
	pub team_id: TeamId,

	/// The user who is a member.
	pub user_id: UserId,

	/// The user's role within the team.
	pub role: TeamRole,

	/// When this membership was created.
	pub created_at: DateTime<Utc>,
}

impl TeamMembership {
	/// Creates a new team membership.
	///
	/// Generates a new membership ID and sets created_at to now.
	pub fn new(team_id: TeamId, user_id: UserId, role: TeamRole) -> Self {
		Self {
			id: uuid::Uuid::new_v4(),
			team_id,
			user_id,
			role,
			created_at: Utc::now(),
		}
	}

	/// Returns true if this member is a maintainer.
	pub fn is_maintainer(&self) -> bool {
		self.role == TeamRole::Maintainer
	}

	/// Returns true if this member has at least the given role's permissions.
	pub fn has_permission_of(&self, role: &TeamRole) -> bool {
		self.role.has_permission_of(role)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	mod team {
		use super::*;

		#[test]
		fn new_creates_team_with_generated_id() {
			let org_id = OrgId::generate();
			let team = Team::new(org_id, "Engineering", "engineering");

			assert_eq!(team.org_id, org_id);
			assert_eq!(team.name, "Engineering");
			assert_eq!(team.slug, "engineering");
		}

		#[test]
		fn new_sets_timestamps() {
			let org_id = OrgId::generate();
			let before = Utc::now();
			let team = Team::new(org_id, "Design", "design");
			let after = Utc::now();

			assert!(team.created_at >= before && team.created_at <= after);
			assert!(team.updated_at >= before && team.updated_at <= after);
			assert_eq!(team.created_at, team.updated_at);
		}

		#[test]
		fn new_generates_unique_ids() {
			let org_id = OrgId::generate();
			let team1 = Team::new(org_id, "Team 1", "team-1");
			let team2 = Team::new(org_id, "Team 2", "team-2");

			assert_ne!(team1.id, team2.id);
		}

		#[test]
		fn serializes_correctly() {
			let org_id = OrgId::generate();
			let team = Team::new(org_id, "Backend", "backend");

			let json = serde_json::to_string(&team).unwrap();
			assert!(json.contains("\"name\":\"Backend\""));
			assert!(json.contains("\"slug\":\"backend\""));
		}

		#[test]
		fn deserializes_correctly() {
			let org_id = OrgId::generate();
			let team = Team::new(org_id, "Frontend", "frontend");
			let json = serde_json::to_string(&team).unwrap();

			let deserialized: Team = serde_json::from_str(&json).unwrap();
			assert_eq!(deserialized.id, team.id);
			assert_eq!(deserialized.org_id, team.org_id);
			assert_eq!(deserialized.name, team.name);
			assert_eq!(deserialized.slug, team.slug);
		}
	}

	mod team_membership {
		use super::*;

		#[test]
		fn new_creates_membership() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Member);

			assert_eq!(membership.team_id, team_id);
			assert_eq!(membership.user_id, user_id);
			assert_eq!(membership.role, TeamRole::Member);
		}

		#[test]
		fn new_sets_created_at() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let before = Utc::now();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Maintainer);
			let after = Utc::now();

			assert!(membership.created_at >= before && membership.created_at <= after);
		}

		#[test]
		fn new_generates_unique_ids() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let m1 = TeamMembership::new(team_id, user_id, TeamRole::Member);
			let m2 = TeamMembership::new(team_id, user_id, TeamRole::Member);

			assert_ne!(m1.id, m2.id);
		}

		#[test]
		fn is_maintainer_returns_true_for_maintainer() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Maintainer);

			assert!(membership.is_maintainer());
		}

		#[test]
		fn is_maintainer_returns_false_for_member() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Member);

			assert!(!membership.is_maintainer());
		}

		#[test]
		fn has_permission_of_maintainer() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Maintainer);

			assert!(membership.has_permission_of(&TeamRole::Maintainer));
			assert!(membership.has_permission_of(&TeamRole::Member));
		}

		#[test]
		fn has_permission_of_member() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Member);

			assert!(!membership.has_permission_of(&TeamRole::Maintainer));
			assert!(membership.has_permission_of(&TeamRole::Member));
		}

		#[test]
		fn serializes_correctly() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Member);

			let json = serde_json::to_string(&membership).unwrap();
			assert!(json.contains("\"role\":\"member\""));
		}

		#[test]
		fn deserializes_correctly() {
			let team_id = TeamId::generate();
			let user_id = UserId::generate();
			let membership = TeamMembership::new(team_id, user_id, TeamRole::Maintainer);
			let json = serde_json::to_string(&membership).unwrap();

			let deserialized: TeamMembership = serde_json::from_str(&json).unwrap();
			assert_eq!(deserialized.id, membership.id);
			assert_eq!(deserialized.team_id, membership.team_id);
			assert_eq!(deserialized.user_id, membership.user_id);
			assert_eq!(deserialized.role, membership.role);
		}
	}
}
