// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerType {
	User,
	Org,
}

impl OwnerType {
	pub fn as_str(&self) -> &'static str {
		match self {
			OwnerType::User => "user",
			OwnerType::Org => "org",
		}
	}
}

impl std::str::FromStr for OwnerType {
	type Err = ();
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"user" => Ok(OwnerType::User),
			"org" => Ok(OwnerType::Org),
			_ => Err(()),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
	Private,
	Public,
}

impl Visibility {
	pub fn as_str(&self) -> &'static str {
		match self {
			Visibility::Private => "private",
			Visibility::Public => "public",
		}
	}
}

impl std::str::FromStr for Visibility {
	type Err = ();
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"private" => Ok(Visibility::Private),
			"public" => Ok(Visibility::Public),
			_ => Err(()),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoRole {
	Read,
	Write,
	Admin,
}

impl RepoRole {
	pub fn as_str(&self) -> &'static str {
		match self {
			RepoRole::Read => "read",
			RepoRole::Write => "write",
			RepoRole::Admin => "admin",
		}
	}

	pub fn has_permission_of(&self, other: &RepoRole) -> bool {
		matches!(
			(self, other),
			(RepoRole::Admin, _)
				| (RepoRole::Write, RepoRole::Read | RepoRole::Write)
				| (RepoRole::Read, RepoRole::Read)
		)
	}
}

impl std::str::FromStr for RepoRole {
	type Err = ();
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"read" => Ok(RepoRole::Read),
			"write" => Ok(RepoRole::Write),
			"admin" => Ok(RepoRole::Admin),
			_ => Err(()),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
	pub id: Uuid,
	pub owner_type: OwnerType,
	pub owner_id: Uuid,
	pub name: String,
	pub visibility: Visibility,
	pub default_branch: String,
	pub deleted_at: Option<DateTime<Utc>>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

impl Repository {
	pub fn new(owner_type: OwnerType, owner_id: Uuid, name: String, visibility: Visibility) -> Self {
		let now = Utc::now();
		Self {
			id: Uuid::new_v4(),
			owner_type,
			owner_id,
			name,
			visibility,
			default_branch: "cannon".to_string(),
			deleted_at: None,
			created_at: now,
			updated_at: now,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchProtectionRule {
	pub id: Uuid,
	pub repo_id: Uuid,
	pub pattern: String,
	pub block_direct_push: bool,
	pub block_force_push: bool,
	pub block_deletion: bool,
	pub created_at: DateTime<Utc>,
}

impl BranchProtectionRule {
	pub fn new(repo_id: Uuid, pattern: String) -> Self {
		Self {
			id: Uuid::new_v4(),
			repo_id,
			pattern,
			block_direct_push: true,
			block_force_push: true,
			block_deletion: true,
			created_at: Utc::now(),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoTeamAccess {
	pub repo_id: Uuid,
	pub team_id: Uuid,
	pub role: RepoRole,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_role_hierarchy() {
		assert!(RepoRole::Admin.has_permission_of(&RepoRole::Read));
		assert!(RepoRole::Admin.has_permission_of(&RepoRole::Write));
		assert!(RepoRole::Admin.has_permission_of(&RepoRole::Admin));
		assert!(RepoRole::Write.has_permission_of(&RepoRole::Read));
		assert!(RepoRole::Write.has_permission_of(&RepoRole::Write));
		assert!(!RepoRole::Write.has_permission_of(&RepoRole::Admin));
		assert!(RepoRole::Read.has_permission_of(&RepoRole::Read));
		assert!(!RepoRole::Read.has_permission_of(&RepoRole::Write));
		assert!(!RepoRole::Read.has_permission_of(&RepoRole::Admin));
	}
}
