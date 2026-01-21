// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_server_db::{RepoRecord, RepoTeamAccessRecord, ScmRepository};
use uuid::Uuid;

use crate::error::{Result, ScmError};
use crate::types::{OwnerType, RepoRole, RepoTeamAccess, Repository, Visibility};

pub fn validate_repo_name(name: &str) -> Result<()> {
	if name.is_empty() || name.len() > 100 {
		return Err(ScmError::InvalidName(
			"Name must be 1-100 characters".into(),
		));
	}

	if name == "." || name == ".." {
		return Err(ScmError::InvalidName("Invalid name".into()));
	}

	if name.starts_with('.') || name.starts_with('-') {
		return Err(ScmError::InvalidName(
			"Name cannot start with '.' or '-'".into(),
		));
	}

	if name.contains("..") {
		return Err(ScmError::InvalidName("Name cannot contain '..'".into()));
	}

	if !name
		.chars()
		.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
	{
		return Err(ScmError::InvalidName(
			"Name can only contain letters, numbers, dash, underscore, dot".into(),
		));
	}

	Ok(())
}

#[async_trait]
pub trait RepoStore: Send + Sync {
	async fn create(&self, repo: &Repository) -> Result<Repository>;
	async fn get_by_id(&self, id: Uuid) -> Result<Option<Repository>>;
	async fn get_by_owner_and_name(
		&self,
		owner_type: OwnerType,
		owner_id: Uuid,
		name: &str,
	) -> Result<Option<Repository>>;
	async fn list_by_owner(&self, owner_type: OwnerType, owner_id: Uuid) -> Result<Vec<Repository>>;
	async fn update(&self, repo: &Repository) -> Result<Repository>;
	async fn soft_delete(&self, id: Uuid) -> Result<()>;
	async fn hard_delete(&self, id: Uuid) -> Result<()>;
}

pub struct SqliteRepoStore {
	db: ScmRepository,
}

impl SqliteRepoStore {
	pub fn new(db: ScmRepository) -> Self {
		Self { db }
	}

	fn record_to_repo(record: RepoRecord) -> Result<Repository> {
		Ok(Repository {
			id: record.id,
			owner_type: record.owner_type.parse::<OwnerType>().map_err(|_| {
				ScmError::Database(sqlx::Error::Decode(
					format!("invalid owner_type: {}", record.owner_type).into(),
				))
			})?,
			owner_id: record.owner_id,
			name: record.name,
			visibility: record.visibility.parse::<Visibility>().map_err(|_| {
				ScmError::Database(sqlx::Error::Decode(
					format!("invalid visibility: {}", record.visibility).into(),
				))
			})?,
			default_branch: record.default_branch,
			deleted_at: record.deleted_at,
			created_at: record.created_at,
			updated_at: record.updated_at,
		})
	}

	fn repo_to_record(repo: &Repository) -> RepoRecord {
		RepoRecord {
			id: repo.id,
			owner_type: repo.owner_type.as_str().to_string(),
			owner_id: repo.owner_id,
			name: repo.name.clone(),
			visibility: repo.visibility.as_str().to_string(),
			default_branch: repo.default_branch.clone(),
			deleted_at: repo.deleted_at,
			created_at: repo.created_at,
			updated_at: repo.updated_at,
		}
	}
}

#[async_trait]
impl RepoStore for SqliteRepoStore {
	async fn create(&self, repo: &Repository) -> Result<Repository> {
		let record = Self::repo_to_record(repo);
		self.db.create_repo(&record).await.map_err(|e| match e {
			loom_server_db::DbError::Conflict(_) => ScmError::AlreadyExists,
			loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
			_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
		})?;
		Ok(repo.clone())
	}

	async fn get_by_id(&self, id: Uuid) -> Result<Option<Repository>> {
		let record = self.db.get_repo_by_id(id).await.map_err(db_err)?;
		record.map(Self::record_to_repo).transpose()
	}

	async fn get_by_owner_and_name(
		&self,
		owner_type: OwnerType,
		owner_id: Uuid,
		name: &str,
	) -> Result<Option<Repository>> {
		let record = self
			.db
			.get_repo_by_owner_and_name(owner_type.as_str(), owner_id, name)
			.await
			.map_err(db_err)?;
		record.map(Self::record_to_repo).transpose()
	}

	async fn list_by_owner(&self, owner_type: OwnerType, owner_id: Uuid) -> Result<Vec<Repository>> {
		let records = self
			.db
			.list_repos_by_owner(owner_type.as_str(), owner_id)
			.await
			.map_err(db_err)?;
		records.into_iter().map(Self::record_to_repo).collect()
	}

	async fn update(&self, repo: &Repository) -> Result<Repository> {
		let record = Self::repo_to_record(repo);
		self.db.update_repo(&record).await.map_err(|e| match e {
			loom_server_db::DbError::NotFound(_) => ScmError::NotFound,
			loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
			_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
		})?;
		self.get_by_id(repo.id).await?.ok_or(ScmError::NotFound)
	}

	async fn soft_delete(&self, id: Uuid) -> Result<()> {
		self.db.soft_delete_repo(id).await.map_err(|e| match e {
			loom_server_db::DbError::NotFound(_) => ScmError::NotFound,
			loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
			_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
		})
	}

	async fn hard_delete(&self, id: Uuid) -> Result<()> {
		self.db.hard_delete_repo(id).await.map_err(|e| match e {
			loom_server_db::DbError::NotFound(_) => ScmError::NotFound,
			loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
			_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
		})
	}
}

#[async_trait]
pub trait RepoTeamAccessStore: Send + Sync {
	async fn grant_team_access(&self, repo_id: Uuid, team_id: Uuid, role: RepoRole) -> Result<()>;
	async fn revoke_team_access(&self, repo_id: Uuid, team_id: Uuid) -> Result<()>;
	async fn list_repo_team_access(&self, repo_id: Uuid) -> Result<Vec<RepoTeamAccess>>;
	async fn get_user_role_via_teams(&self, user_id: Uuid, repo_id: Uuid)
		-> Result<Option<RepoRole>>;
}

pub struct SqliteRepoTeamAccessStore {
	db: ScmRepository,
}

impl SqliteRepoTeamAccessStore {
	pub fn new(db: ScmRepository) -> Self {
		Self { db }
	}

	fn record_to_team_access(record: RepoTeamAccessRecord) -> Result<RepoTeamAccess> {
		Ok(RepoTeamAccess {
			repo_id: record.repo_id,
			team_id: record.team_id,
			role: record.role.parse::<RepoRole>().map_err(|_| {
				ScmError::Database(sqlx::Error::Decode(
					format!("invalid role: {}", record.role).into(),
				))
			})?,
		})
	}
}

#[async_trait]
impl RepoTeamAccessStore for SqliteRepoTeamAccessStore {
	async fn grant_team_access(&self, repo_id: Uuid, team_id: Uuid, role: RepoRole) -> Result<()> {
		self
			.db
			.grant_team_access(repo_id, team_id, role.as_str())
			.await
			.map_err(db_err)
	}

	async fn revoke_team_access(&self, repo_id: Uuid, team_id: Uuid) -> Result<()> {
		self
			.db
			.revoke_team_access(repo_id, team_id)
			.await
			.map_err(|e| match e {
				loom_server_db::DbError::NotFound(_) => ScmError::NotFound,
				loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
				_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
			})
	}

	async fn list_repo_team_access(&self, repo_id: Uuid) -> Result<Vec<RepoTeamAccess>> {
		let records = self
			.db
			.list_repo_team_access(repo_id)
			.await
			.map_err(db_err)?;
		records
			.into_iter()
			.map(Self::record_to_team_access)
			.collect()
	}

	async fn get_user_role_via_teams(
		&self,
		user_id: Uuid,
		repo_id: Uuid,
	) -> Result<Option<RepoRole>> {
		let roles = self
			.db
			.get_user_roles_via_teams(user_id, repo_id)
			.await
			.map_err(db_err)?;

		let mut highest_role: Option<RepoRole> = None;
		for role_str in roles {
			let role = role_str.parse::<RepoRole>().map_err(|_| {
				ScmError::Database(sqlx::Error::Decode(
					format!("invalid role: {}", role_str).into(),
				))
			})?;

			highest_role = Some(match highest_role {
				None => role,
				Some(current) => {
					if role.has_permission_of(&current) {
						role
					} else {
						current
					}
				}
			});
		}

		Ok(highest_role)
	}
}

fn db_err(e: loom_server_db::DbError) -> ScmError {
	match e {
		loom_server_db::DbError::Sqlx(e) => ScmError::Database(e),
		_ => ScmError::Database(sqlx::Error::Protocol(e.to_string())),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_valid_names() {
		assert!(validate_repo_name("my-repo").is_ok());
		assert!(validate_repo_name("repo_name").is_ok());
		assert!(validate_repo_name("repo.v2").is_ok());
		assert!(validate_repo_name("MyRepo123").is_ok());
		assert!(validate_repo_name("a").is_ok());
		assert!(validate_repo_name("A123_test-name.v1").is_ok());
	}

	#[test]
	fn test_empty_name() {
		assert!(validate_repo_name("").is_err());
	}

	#[test]
	fn test_name_too_long() {
		let long_name = "a".repeat(101);
		assert!(validate_repo_name(&long_name).is_err());
		let max_name = "a".repeat(100);
		assert!(validate_repo_name(&max_name).is_ok());
	}

	#[test]
	fn test_dot_names() {
		assert!(validate_repo_name(".").is_err());
		assert!(validate_repo_name("..").is_err());
	}

	#[test]
	fn test_starts_with_dot_or_dash() {
		assert!(validate_repo_name(".hidden").is_err());
		assert!(validate_repo_name("-dash").is_err());
	}

	#[test]
	fn test_path_traversal() {
		assert!(validate_repo_name("../etc").is_err());
		assert!(validate_repo_name("foo/../bar").is_err());
		assert!(validate_repo_name("..passwd").is_err());
	}

	#[test]
	fn test_slashes() {
		assert!(validate_repo_name("repo/name").is_err());
		assert!(validate_repo_name("repo\\name").is_err());
	}

	#[test]
	fn test_shell_metacharacters() {
		assert!(validate_repo_name("repo;rm -rf").is_err());
		assert!(validate_repo_name("repo&cmd").is_err());
		assert!(validate_repo_name("repo|cat").is_err());
		assert!(validate_repo_name("repo`cmd`").is_err());
		assert!(validate_repo_name("repo$VAR").is_err());
		assert!(validate_repo_name("repo$(cmd)").is_err());
		assert!(validate_repo_name("repo{a,b}").is_err());
		assert!(validate_repo_name("repo<file").is_err());
		assert!(validate_repo_name("repo>file").is_err());
		assert!(validate_repo_name("repo!cmd").is_err());
	}

	#[test]
	fn test_spaces_and_special() {
		assert!(validate_repo_name("my repo").is_err());
		assert!(validate_repo_name("repo@name").is_err());
		assert!(validate_repo_name("repo#1").is_err());
	}

	proptest! {
		#[test]
		fn valid_names_pass(name in "[a-zA-Z]([a-zA-Z0-9_-]|[.][a-zA-Z0-9_-]){0,49}") {
			prop_assert!(validate_repo_name(&name).is_ok());
		}

		#[test]
		fn path_traversal_rejected(prefix in r"\.\./.*") {
			prop_assert!(validate_repo_name(&prefix).is_err());
		}

		#[test]
		fn shell_metacharacters_rejected(name in r"[a-zA-Z0-9]*[;&|`$(){}\[\]<>!][a-zA-Z0-9]*") {
			prop_assert!(validate_repo_name(&name).is_err());
		}

		#[test]
		fn slashes_rejected(name in r"[a-zA-Z0-9]*[/\\][a-zA-Z0-9]*") {
			prop_assert!(validate_repo_name(&name).is_err());
		}
	}

	mod team_access_tests {
		use super::*;

		#[test]
		fn test_admin_has_highest_permission() {
			assert!(RepoRole::Admin.has_permission_of(&RepoRole::Admin));
			assert!(RepoRole::Admin.has_permission_of(&RepoRole::Write));
			assert!(RepoRole::Admin.has_permission_of(&RepoRole::Read));
		}

		#[test]
		fn test_write_has_mid_permission() {
			assert!(!RepoRole::Write.has_permission_of(&RepoRole::Admin));
			assert!(RepoRole::Write.has_permission_of(&RepoRole::Write));
			assert!(RepoRole::Write.has_permission_of(&RepoRole::Read));
		}

		#[test]
		fn test_read_has_lowest_permission() {
			assert!(!RepoRole::Read.has_permission_of(&RepoRole::Admin));
			assert!(!RepoRole::Read.has_permission_of(&RepoRole::Write));
			assert!(RepoRole::Read.has_permission_of(&RepoRole::Read));
		}

		#[test]
		fn test_repo_team_access_struct() {
			let access = RepoTeamAccess {
				repo_id: Uuid::new_v4(),
				team_id: Uuid::new_v4(),
				role: RepoRole::Write,
			};
			assert_eq!(access.role, RepoRole::Write);
		}
	}
}
