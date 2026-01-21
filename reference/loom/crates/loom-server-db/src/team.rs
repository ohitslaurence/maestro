// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Team repository for database operations.
//!
//! This module provides database access for team management within organizations.
//! Teams group users for access control and collaboration.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loom_server_auth::{
	team::{Team, TeamMembership},
	types::{OrgId, TeamId, TeamRole, UserId},
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

use crate::error::DbError;

#[async_trait]
pub trait TeamStore: Send + Sync {
	async fn create_team(&self, team: &Team) -> Result<(), DbError>;
	async fn get_team_by_id(&self, id: &TeamId) -> Result<Option<Team>, DbError>;
	async fn get_team_by_slug(&self, org_id: &OrgId, slug: &str) -> Result<Option<Team>, DbError>;
	async fn update_team(&self, team: &Team) -> Result<(), DbError>;
	async fn delete_team(&self, id: &TeamId) -> Result<bool, DbError>;
	async fn list_teams_for_org(&self, org_id: &OrgId) -> Result<Vec<Team>, DbError>;
	async fn create_scim_team(
		&self,
		org_id: &OrgId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<TeamId, DbError>;
	async fn update_scim_team(
		&self,
		team_id: &TeamId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<(), DbError>;
	async fn delete_scim_team(&self, team_id: &TeamId, org_id: &OrgId) -> Result<bool, DbError>;
	async fn set_team_members(&self, team_id: &TeamId, user_ids: &[UserId]) -> Result<(), DbError>;
	async fn list_scim_teams(
		&self,
		org_id: &OrgId,
		limit: i64,
		offset: i64,
	) -> Result<Vec<ScimTeam>, DbError>;
	async fn count_teams_in_org(&self, org_id: &OrgId) -> Result<i64, DbError>;
	async fn get_team_with_scim_fields(
		&self,
		team_id: &TeamId,
		org_id: &OrgId,
	) -> Result<Option<ScimTeam>, DbError>;
	async fn list_scim_group_members(
		&self,
		team_id: &TeamId,
	) -> Result<Vec<(UserId, Option<String>)>, DbError>;
	async fn add_member(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError>;
	async fn get_membership(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
	) -> Result<Option<TeamMembership>, DbError>;
	async fn update_member_role(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError>;
	async fn remove_member(&self, team_id: &TeamId, user_id: &UserId) -> Result<bool, DbError>;
	async fn list_members(&self, team_id: &TeamId) -> Result<Vec<TeamMembership>, DbError>;
	async fn get_teams_for_user(&self, user_id: &UserId) -> Result<Vec<(Team, TeamRole)>, DbError>;
}

/// A team with SCIM-specific fields for provisioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimTeam {
	pub id: TeamId,
	pub org_id: OrgId,
	pub name: String,
	pub slug: String,
	pub scim_external_id: Option<String>,
	pub scim_managed: bool,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// Repository for team database operations.
///
/// Manages teams within organizations and their memberships.
/// Teams are scoped to a single organization.
#[derive(Clone)]
pub struct TeamRepository {
	pool: SqlitePool,
}

impl TeamRepository {
	/// Create a new repository with the given pool.
	///
	/// # Arguments
	/// * `pool` - SQLite connection pool
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	// =========================================================================
	// Team CRUD
	// =========================================================================

	/// Create a new team.
	///
	/// # Arguments
	/// * `team` - The team to create
	///
	/// # Errors
	/// Returns `DbError::Sqlx` if insert fails (e.g., duplicate slug within org).
	///
	/// # Database Constraints
	/// - `id` must be unique
	/// - (`org_id`, `slug`) must be unique
	/// - `org_id` must reference an existing organization
	#[tracing::instrument(skip(self, team), fields(team_id = %team.id, org_id = %team.org_id, slug = %team.slug))]
	pub async fn create_team(&self, team: &Team) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO teams (id, org_id, name, slug, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(team.id.to_string())
		.bind(team.org_id.to_string())
		.bind(&team.name)
		.bind(&team.slug)
		.bind(team.created_at.to_rfc3339())
		.bind(team.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team.id, org_id = %team.org_id, "team created");
		Ok(())
	}

	/// Get a team by ID.
	///
	/// # Arguments
	/// * `id` - The team's UUID
	///
	/// # Returns
	/// `None` if no team exists with this ID.
	#[tracing::instrument(skip(self), fields(team_id = %id))]
	pub async fn get_team_by_id(&self, id: &TeamId) -> Result<Option<Team>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, name, slug, created_at, updated_at
			FROM teams
			WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| self.row_to_team(&r)).transpose()
	}

	/// Get a team by slug within an organization.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	/// * `slug` - The team's URL-safe slug
	///
	/// # Returns
	/// `None` if no team exists with this slug in the organization.
	#[tracing::instrument(skip(self), fields(org_id = %org_id, slug = %slug))]
	pub async fn get_team_by_slug(
		&self,
		org_id: &OrgId,
		slug: &str,
	) -> Result<Option<Team>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, name, slug, created_at, updated_at
			FROM teams
			WHERE org_id = ? AND slug = ?
			"#,
		)
		.bind(org_id.to_string())
		.bind(slug)
		.fetch_optional(&self.pool)
		.await?;

		let result = row.map(|r| self.row_to_team(&r)).transpose()?;
		if let Some(ref team) = result {
			tracing::debug!(team_id = %team.id, "team found by slug");
		}
		Ok(result)
	}

	/// Update a team.
	///
	/// # Arguments
	/// * `team` - The team with updated fields
	///
	/// # Errors
	/// Returns `DbError::Sqlx` if update fails (e.g., duplicate slug).
	#[tracing::instrument(skip(self, team), fields(team_id = %team.id))]
	pub async fn update_team(&self, team: &Team) -> Result<(), DbError> {
		let now = Utc::now().to_rfc3339();
		sqlx::query(
			r#"
			UPDATE teams
			SET name = ?, slug = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&team.name)
		.bind(&team.slug)
		.bind(now)
		.bind(team.id.to_string())
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team.id, "team updated");
		Ok(())
	}

	/// Delete a team.
	///
	/// # Arguments
	/// * `id` - The team's UUID
	///
	/// # Returns
	/// `true` if a team was deleted, `false` if not found.
	///
	/// # Note
	/// This is a hard delete. Team memberships will be cascade deleted.
	#[tracing::instrument(skip(self), fields(team_id = %id))]
	pub async fn delete_team(&self, id: &TeamId) -> Result<bool, DbError> {
		let result = sqlx::query(
			r#"
			DELETE FROM teams
			WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		let deleted = result.rows_affected() > 0;
		if deleted {
			tracing::debug!(team_id = %id, "team deleted");
		}
		Ok(deleted)
	}

	/// List all teams for an organization.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	///
	/// # Returns
	/// List of teams ordered by name.
	#[tracing::instrument(skip(self), fields(org_id = %org_id))]
	pub async fn list_teams_for_org(&self, org_id: &OrgId) -> Result<Vec<Team>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, org_id, name, slug, created_at, updated_at
			FROM teams
			WHERE org_id = ?
			ORDER BY name ASC
			"#,
		)
		.bind(org_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		let teams: Result<Vec<_>, _> = rows.iter().map(|r| self.row_to_team(r)).collect();
		let teams = teams?;
		tracing::debug!(org_id = %org_id, count = teams.len(), "listed teams for organization");
		Ok(teams)
	}

	// =========================================================================
	// SCIM Operations
	// =========================================================================

	/// Create a SCIM-managed team.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	/// * `name` - Display name of the team
	/// * `scim_external_id` - Optional external ID from SCIM provider
	///
	/// # Returns
	/// The ID of the created team.
	#[tracing::instrument(skip(self), fields(org_id = %org_id, name = %name))]
	pub async fn create_scim_team(
		&self,
		org_id: &OrgId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<TeamId, DbError> {
		let team_id = TeamId::generate();
		let now = Utc::now().to_rfc3339();

		sqlx::query(
			r#"
			INSERT INTO teams (id, org_id, name, slug, scim_external_id, scim_managed, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, 1, ?, ?)
			"#,
		)
		.bind(team_id.to_string())
		.bind(org_id.to_string())
		.bind(name)
		.bind(slug_from_name(name))
		.bind(scim_external_id)
		.bind(&now)
		.bind(&now)
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team_id, org_id = %org_id, "SCIM team created");
		Ok(team_id)
	}

	/// Update a SCIM-managed team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `name` - New display name
	/// * `scim_external_id` - New external ID from SCIM provider
	#[tracing::instrument(skip(self), fields(team_id = %team_id))]
	pub async fn update_scim_team(
		&self,
		team_id: &TeamId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<(), DbError> {
		let now = Utc::now().to_rfc3339();

		sqlx::query(
			r#"
			UPDATE teams SET name = ?, slug = ?, scim_external_id = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(name)
		.bind(slug_from_name(name))
		.bind(scim_external_id)
		.bind(&now)
		.bind(team_id.to_string())
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team_id, "SCIM team updated");
		Ok(())
	}

	/// Delete a SCIM-managed team.
	///
	/// Only deletes teams that were created via SCIM (scim_managed = 1).
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `org_id` - The organization's UUID (for authorization)
	///
	/// # Returns
	/// `true` if a team was deleted, `false` if not found or not SCIM-managed.
	#[tracing::instrument(skip(self), fields(team_id = %team_id, org_id = %org_id))]
	pub async fn delete_scim_team(&self, team_id: &TeamId, org_id: &OrgId) -> Result<bool, DbError> {
		sqlx::query("DELETE FROM team_memberships WHERE team_id = ?")
			.bind(team_id.to_string())
			.execute(&self.pool)
			.await?;

		let result = sqlx::query("DELETE FROM teams WHERE id = ? AND org_id = ? AND scim_managed = 1")
			.bind(team_id.to_string())
			.bind(org_id.to_string())
			.execute(&self.pool)
			.await?;

		let deleted = result.rows_affected() > 0;
		if deleted {
			tracing::debug!(team_id = %team_id, "SCIM team deleted");
		}
		Ok(deleted)
	}

	/// Replace all members of a team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `user_ids` - New list of user IDs to be members
	#[tracing::instrument(skip(self, user_ids), fields(team_id = %team_id, member_count = user_ids.len()))]
	pub async fn set_team_members(
		&self,
		team_id: &TeamId,
		user_ids: &[UserId],
	) -> Result<(), DbError> {
		sqlx::query("DELETE FROM team_memberships WHERE team_id = ?")
			.bind(team_id.to_string())
			.execute(&self.pool)
			.await?;

		let now = Utc::now().to_rfc3339();
		for user_id in user_ids {
			let membership_id = Uuid::new_v4().to_string();
			sqlx::query(
				r#"
				INSERT INTO team_memberships (id, team_id, user_id, role, created_at)
				VALUES (?, ?, ?, 'member', ?)
				"#,
			)
			.bind(&membership_id)
			.bind(team_id.to_string())
			.bind(user_id.to_string())
			.bind(&now)
			.execute(&self.pool)
			.await?;
		}

		tracing::debug!(team_id = %team_id, count = user_ids.len(), "team members replaced");
		Ok(())
	}

	/// List SCIM teams in an organization with pagination.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	/// * `limit` - Maximum number of teams to return
	/// * `offset` - Number of teams to skip
	///
	/// # Returns
	/// List of teams with SCIM fields, ordered by ID.
	#[tracing::instrument(skip(self), fields(org_id = %org_id, limit = limit, offset = offset))]
	pub async fn list_scim_teams(
		&self,
		org_id: &OrgId,
		limit: i64,
		offset: i64,
	) -> Result<Vec<ScimTeam>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, org_id, name, slug, scim_external_id, scim_managed, created_at, updated_at
			FROM teams
			WHERE org_id = ?
			ORDER BY id ASC
			LIMIT ? OFFSET ?
			"#,
		)
		.bind(org_id.to_string())
		.bind(limit)
		.bind(offset)
		.fetch_all(&self.pool)
		.await?;

		let teams: Result<Vec<_>, _> = rows.iter().map(|r| self.row_to_scim_team(r)).collect();
		let teams = teams?;
		tracing::debug!(org_id = %org_id, count = teams.len(), "listed SCIM teams");
		Ok(teams)
	}

	/// Count total teams in an organization.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	#[tracing::instrument(skip(self), fields(org_id = %org_id))]
	pub async fn count_teams_in_org(&self, org_id: &OrgId) -> Result<i64, DbError> {
		let row = sqlx::query(r#"SELECT COUNT(*) as count FROM teams WHERE org_id = ?"#)
			.bind(org_id.to_string())
			.fetch_one(&self.pool)
			.await?;

		let count: i64 = row.get("count");
		Ok(count)
	}

	/// Get a team by ID including SCIM fields.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `org_id` - The organization's UUID (for authorization)
	///
	/// # Returns
	/// `None` if no team exists with this ID in the organization.
	#[tracing::instrument(skip(self), fields(team_id = %team_id, org_id = %org_id))]
	pub async fn get_team_with_scim_fields(
		&self,
		team_id: &TeamId,
		org_id: &OrgId,
	) -> Result<Option<ScimTeam>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, name, slug, scim_external_id, scim_managed, created_at, updated_at
			FROM teams
			WHERE id = ? AND org_id = ?
			"#,
		)
		.bind(team_id.to_string())
		.bind(org_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| self.row_to_scim_team(&r)).transpose()
	}

	/// List group members with display names for SCIM response.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	///
	/// # Returns
	/// List of (user_id, display_name) tuples.
	#[tracing::instrument(skip(self), fields(team_id = %team_id))]
	pub async fn list_scim_group_members(
		&self,
		team_id: &TeamId,
	) -> Result<Vec<(UserId, Option<String>)>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT u.id, u.display_name
			FROM users u
			JOIN team_memberships tm ON u.id = tm.user_id
			WHERE tm.team_id = ?
			"#,
		)
		.bind(team_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		let mut members = Vec::with_capacity(rows.len());
		for row in rows {
			let id_str: String = row.get("id");
			let display_name: Option<String> = row.get("display_name");
			let user_id =
				Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(format!("Invalid user ID: {e}")))?;
			members.push((UserId::new(user_id), display_name));
		}
		Ok(members)
	}

	// =========================================================================
	// Memberships
	// =========================================================================

	/// Add a member to a team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `user_id` - The user's UUID
	/// * `role` - The member's role (maintainer or member)
	///
	/// # Database Constraints
	/// - (`team_id`, `user_id`) must be unique
	/// - `team_id` must reference an existing team
	/// - `user_id` must reference an existing user
	#[tracing::instrument(skip(self), fields(team_id = %team_id, user_id = %user_id, role = %role))]
	pub async fn add_member(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError> {
		let id = Uuid::new_v4().to_string();
		let now = Utc::now().to_rfc3339();
		sqlx::query(
			r#"
			INSERT INTO team_memberships (id, team_id, user_id, role, created_at)
			VALUES (?, ?, ?, ?, ?)
			"#,
		)
		.bind(&id)
		.bind(team_id.to_string())
		.bind(user_id.to_string())
		.bind(role.to_string())
		.bind(&now)
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team_id, user_id = %user_id, role = %role, "member added to team");
		Ok(())
	}

	/// Get a membership for a user in a team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `user_id` - The user's UUID
	///
	/// # Returns
	/// `None` if the user is not a member.
	#[tracing::instrument(skip(self), fields(team_id = %team_id, user_id = %user_id))]
	pub async fn get_membership(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
	) -> Result<Option<TeamMembership>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, team_id, user_id, role, created_at
			FROM team_memberships
			WHERE team_id = ? AND user_id = ?
			"#,
		)
		.bind(team_id.to_string())
		.bind(user_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| self.row_to_membership(&r)).transpose()
	}

	/// Update a member's role.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `user_id` - The user's UUID
	/// * `role` - The new role
	#[tracing::instrument(skip(self), fields(team_id = %team_id, user_id = %user_id, role = %role))]
	pub async fn update_member_role(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError> {
		sqlx::query(
			r#"
			UPDATE team_memberships
			SET role = ?
			WHERE team_id = ? AND user_id = ?
			"#,
		)
		.bind(role.to_string())
		.bind(team_id.to_string())
		.bind(user_id.to_string())
		.execute(&self.pool)
		.await?;

		tracing::debug!(team_id = %team_id, user_id = %user_id, role = %role, "team member role updated");
		Ok(())
	}

	/// Remove a member from a team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	/// * `user_id` - The user's UUID
	///
	/// # Returns
	/// `true` if a member was removed, `false` if not found.
	#[tracing::instrument(skip(self), fields(team_id = %team_id, user_id = %user_id))]
	pub async fn remove_member(&self, team_id: &TeamId, user_id: &UserId) -> Result<bool, DbError> {
		let result = sqlx::query(
			r#"
			DELETE FROM team_memberships
			WHERE team_id = ? AND user_id = ?
			"#,
		)
		.bind(team_id.to_string())
		.bind(user_id.to_string())
		.execute(&self.pool)
		.await?;

		let removed = result.rows_affected() > 0;
		if removed {
			tracing::debug!(team_id = %team_id, user_id = %user_id, "member removed from team");
		}
		Ok(removed)
	}

	/// List all members of a team.
	///
	/// # Arguments
	/// * `team_id` - The team's UUID
	///
	/// # Returns
	/// List of memberships ordered by join date.
	#[tracing::instrument(skip(self), fields(team_id = %team_id))]
	pub async fn list_members(&self, team_id: &TeamId) -> Result<Vec<TeamMembership>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, team_id, user_id, role, created_at
			FROM team_memberships
			WHERE team_id = ?
			ORDER BY created_at ASC
			"#,
		)
		.bind(team_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		let members: Result<Vec<_>, _> = rows.iter().map(|r| self.row_to_membership(r)).collect();
		let members = members?;
		tracing::debug!(team_id = %team_id, count = members.len(), "listed team members");
		Ok(members)
	}

	/// Get all teams a user is a member of, with their role.
	///
	/// # Arguments
	/// * `user_id` - The user's UUID
	///
	/// # Returns
	/// List of (team, role) tuples ordered by team name.
	#[tracing::instrument(skip(self), fields(user_id = %user_id))]
	pub async fn get_teams_for_user(
		&self,
		user_id: &UserId,
	) -> Result<Vec<(Team, TeamRole)>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT t.id, t.org_id, t.name, t.slug, t.created_at, t.updated_at, m.role
			FROM teams t
			INNER JOIN team_memberships m ON t.id = m.team_id
			WHERE m.user_id = ?
			ORDER BY t.name ASC
			"#,
		)
		.bind(user_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		let mut result = Vec::with_capacity(rows.len());
		for r in &rows {
			let team = self.row_to_team(r)?;
			let role_str: String = r.get("role");
			let role = match role_str.as_str() {
				"maintainer" => TeamRole::Maintainer,
				_ => TeamRole::Member,
			};
			result.push((team, role));
		}
		tracing::debug!(user_id = %user_id, count = result.len(), "retrieved teams for user");
		Ok(result)
	}

	// =========================================================================
	// Helpers
	// =========================================================================

	fn row_to_team(&self, row: &sqlx::sqlite::SqliteRow) -> Result<Team, DbError> {
		let id_str: String = row.get("id");
		let org_id_str: String = row.get("org_id");
		let created_at: String = row.get("created_at");
		let updated_at: String = row.get("updated_at");

		let id =
			Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(format!("Invalid team ID: {e}")))?;
		let org_id = Uuid::parse_str(&org_id_str)
			.map_err(|e| DbError::Internal(format!("Invalid org_id: {e}")))?;

		Ok(Team {
			id: TeamId::new(id),
			org_id: OrgId::new(org_id),
			name: row.get("name"),
			slug: row.get("slug"),
			created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
				.map_err(|e| DbError::Internal(format!("Invalid created_at: {e}")))?
				.with_timezone(&Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
				.map_err(|e| DbError::Internal(format!("Invalid updated_at: {e}")))?
				.with_timezone(&Utc),
		})
	}

	fn row_to_membership(&self, row: &sqlx::sqlite::SqliteRow) -> Result<TeamMembership, DbError> {
		let id_str: String = row.get("id");
		let team_id_str: String = row.get("team_id");
		let user_id_str: String = row.get("user_id");
		let role_str: String = row.get("role");
		let created_at: String = row.get("created_at");

		let id = Uuid::parse_str(&id_str)
			.map_err(|e| DbError::Internal(format!("Invalid membership ID: {e}")))?;
		let team_id = Uuid::parse_str(&team_id_str)
			.map_err(|e| DbError::Internal(format!("Invalid team_id: {e}")))?;
		let user_id = Uuid::parse_str(&user_id_str)
			.map_err(|e| DbError::Internal(format!("Invalid user_id: {e}")))?;
		let role = match role_str.as_str() {
			"maintainer" => TeamRole::Maintainer,
			_ => TeamRole::Member,
		};

		Ok(TeamMembership {
			id,
			team_id: TeamId::new(team_id),
			user_id: UserId::new(user_id),
			role,
			created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
				.map_err(|e| DbError::Internal(format!("Invalid created_at: {e}")))?
				.with_timezone(&Utc),
		})
	}

	fn row_to_scim_team(&self, row: &sqlx::sqlite::SqliteRow) -> Result<ScimTeam, DbError> {
		let id_str: String = row.get("id");
		let org_id_str: String = row.get("org_id");
		let created_at: String = row.get("created_at");
		let updated_at: String = row.get("updated_at");
		let scim_managed: i64 = row.get("scim_managed");

		let id =
			Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(format!("Invalid team ID: {e}")))?;
		let org_id = Uuid::parse_str(&org_id_str)
			.map_err(|e| DbError::Internal(format!("Invalid org_id: {e}")))?;

		Ok(ScimTeam {
			id: TeamId::new(id),
			org_id: OrgId::new(org_id),
			name: row.get("name"),
			slug: row.get("slug"),
			scim_external_id: row.get("scim_external_id"),
			scim_managed: scim_managed == 1,
			created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
				.map_err(|e| DbError::Internal(format!("Invalid created_at: {e}")))?
				.with_timezone(&Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
				.map_err(|e| DbError::Internal(format!("Invalid updated_at: {e}")))?
				.with_timezone(&Utc),
		})
	}
}

#[async_trait]
impl TeamStore for TeamRepository {
	async fn create_team(&self, team: &Team) -> Result<(), DbError> {
		self.create_team(team).await
	}

	async fn get_team_by_id(&self, id: &TeamId) -> Result<Option<Team>, DbError> {
		self.get_team_by_id(id).await
	}

	async fn get_team_by_slug(&self, org_id: &OrgId, slug: &str) -> Result<Option<Team>, DbError> {
		self.get_team_by_slug(org_id, slug).await
	}

	async fn update_team(&self, team: &Team) -> Result<(), DbError> {
		self.update_team(team).await
	}

	async fn delete_team(&self, id: &TeamId) -> Result<bool, DbError> {
		self.delete_team(id).await
	}

	async fn list_teams_for_org(&self, org_id: &OrgId) -> Result<Vec<Team>, DbError> {
		self.list_teams_for_org(org_id).await
	}

	async fn create_scim_team(
		&self,
		org_id: &OrgId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<TeamId, DbError> {
		self.create_scim_team(org_id, name, scim_external_id).await
	}

	async fn update_scim_team(
		&self,
		team_id: &TeamId,
		name: &str,
		scim_external_id: Option<&str>,
	) -> Result<(), DbError> {
		self.update_scim_team(team_id, name, scim_external_id).await
	}

	async fn delete_scim_team(&self, team_id: &TeamId, org_id: &OrgId) -> Result<bool, DbError> {
		self.delete_scim_team(team_id, org_id).await
	}

	async fn set_team_members(&self, team_id: &TeamId, user_ids: &[UserId]) -> Result<(), DbError> {
		self.set_team_members(team_id, user_ids).await
	}

	async fn list_scim_teams(
		&self,
		org_id: &OrgId,
		limit: i64,
		offset: i64,
	) -> Result<Vec<ScimTeam>, DbError> {
		self.list_scim_teams(org_id, limit, offset).await
	}

	async fn count_teams_in_org(&self, org_id: &OrgId) -> Result<i64, DbError> {
		self.count_teams_in_org(org_id).await
	}

	async fn get_team_with_scim_fields(
		&self,
		team_id: &TeamId,
		org_id: &OrgId,
	) -> Result<Option<ScimTeam>, DbError> {
		self.get_team_with_scim_fields(team_id, org_id).await
	}

	async fn list_scim_group_members(
		&self,
		team_id: &TeamId,
	) -> Result<Vec<(UserId, Option<String>)>, DbError> {
		self.list_scim_group_members(team_id).await
	}

	async fn add_member(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError> {
		self.add_member(team_id, user_id, role).await
	}

	async fn get_membership(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
	) -> Result<Option<TeamMembership>, DbError> {
		self.get_membership(team_id, user_id).await
	}

	async fn update_member_role(
		&self,
		team_id: &TeamId,
		user_id: &UserId,
		role: TeamRole,
	) -> Result<(), DbError> {
		self.update_member_role(team_id, user_id, role).await
	}

	async fn remove_member(&self, team_id: &TeamId, user_id: &UserId) -> Result<bool, DbError> {
		self.remove_member(team_id, user_id).await
	}

	async fn list_members(&self, team_id: &TeamId) -> Result<Vec<TeamMembership>, DbError> {
		self.list_members(team_id).await
	}

	async fn get_teams_for_user(&self, user_id: &UserId) -> Result<Vec<(Team, TeamRole)>, DbError> {
		self.get_teams_for_user(user_id).await
	}
}

fn slug_from_name(name: &str) -> String {
	name
		.to_lowercase()
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
		.collect::<String>()
		.split('-')
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>()
		.join("-")
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::collections::HashSet;
	use std::str::FromStr;

	proptest! {
		#[test]
		fn team_id_generation_is_unique(count in 1..1000usize) {
			let mut ids = HashSet::new();
			for _ in 0..count {
				let id = TeamId::generate();
				prop_assert!(ids.insert(id.to_string()), "Generated duplicate TeamId");
			}
		}

		#[test]
		fn team_slug_validation(slug in "[a-z0-9-]{1,50}") {
			let is_valid = slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
			prop_assert!(is_valid, "Team slug should only contain lowercase alphanumeric and dashes");
			prop_assert!(!slug.is_empty(), "Team slug should not be empty");
		}

		#[test]
		fn team_membership_id_is_uuid(_unused: u8) {
			let id = Uuid::new_v4();
			prop_assert!(id.to_string().len() == 36, "UUID should be 36 characters");
		}
	}

	async fn create_team_test_pool() -> SqlitePool {
		let options = SqliteConnectOptions::from_str(":memory:")
			.unwrap()
			.create_if_missing(true);

		let pool = SqlitePoolOptions::new()
			.max_connections(1)
			.connect_with(options)
			.await
			.expect("Failed to create test pool");

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS users (
				id TEXT PRIMARY KEY,
				display_name TEXT NOT NULL,
				username TEXT UNIQUE,
				primary_email TEXT UNIQUE,
				avatar_url TEXT,
				email_visible INTEGER DEFAULT 1,
				is_system_admin INTEGER DEFAULT 0,
				is_support INTEGER DEFAULT 0,
				is_auditor INTEGER DEFAULT 0,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT,
				locale TEXT DEFAULT NULL
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS organizations (
				id TEXT PRIMARY KEY,
				name TEXT NOT NULL,
				slug TEXT UNIQUE NOT NULL,
				visibility TEXT NOT NULL DEFAULT 'public',
				is_personal INTEGER DEFAULT 0,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS teams (
				id TEXT PRIMARY KEY,
				org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
				name TEXT NOT NULL,
				slug TEXT NOT NULL,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				scim_external_id TEXT,
				scim_managed INTEGER NOT NULL DEFAULT 0,
				UNIQUE(org_id, slug)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS team_memberships (
				id TEXT PRIMARY KEY,
				team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
				user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
				role TEXT NOT NULL,
				created_at TEXT NOT NULL,
				UNIQUE(team_id, user_id)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_team_repo() -> TeamRepository {
		let pool = create_team_test_pool().await;
		TeamRepository::new(pool)
	}

	fn make_test_team(org_id: &OrgId, slug: &str, name: &str) -> Team {
		let now = Utc::now();
		Team {
			id: TeamId::generate(),
			org_id: org_id.clone(),
			name: name.to_string(),
			slug: slug.to_string(),
			created_at: now,
			updated_at: now,
		}
	}

	async fn insert_test_org(pool: &SqlitePool, org_id: &OrgId) {
		let now = Utc::now().to_rfc3339();
		sqlx::query(
			r#"
			INSERT INTO organizations (id, name, slug, visibility, created_at, updated_at)
			VALUES (?, 'Test Org', ?, 'private', ?, ?)
			"#,
		)
		.bind(org_id.to_string())
		.bind(format!("org-{}", Uuid::new_v4()))
		.bind(&now)
		.bind(&now)
		.execute(pool)
		.await
		.unwrap();
	}

	async fn insert_test_user(pool: &SqlitePool, user_id: &UserId) {
		let now = Utc::now().to_rfc3339();
		sqlx::query(
			r#"
			INSERT INTO users (id, display_name, created_at, updated_at)
			VALUES (?, 'Test User', ?, ?)
			"#,
		)
		.bind(user_id.to_string())
		.bind(&now)
		.bind(&now)
		.execute(pool)
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_create_and_get_team() {
		let pool = create_team_test_pool().await;
		let repo = TeamRepository::new(pool.clone());

		let org_id = OrgId::generate();
		insert_test_org(&pool, &org_id).await;

		let team = make_test_team(&org_id, "engineering", "Engineering");
		repo.create_team(&team).await.unwrap();

		let fetched = repo.get_team_by_id(&team.id).await.unwrap();
		assert!(fetched.is_some());
		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, team.id);
		assert_eq!(fetched.org_id, org_id);
		assert_eq!(fetched.name, "Engineering");
		assert_eq!(fetched.slug, "engineering");
	}

	#[tokio::test]
	async fn test_get_team_not_found() {
		let repo = make_team_repo().await;
		let non_existent_id = TeamId::generate();

		let result = repo.get_team_by_id(&non_existent_id).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_add_and_remove_member() {
		let pool = create_team_test_pool().await;
		let repo = TeamRepository::new(pool.clone());

		let org_id = OrgId::generate();
		insert_test_org(&pool, &org_id).await;

		let team = make_test_team(&org_id, "backend", "Backend Team");
		repo.create_team(&team).await.unwrap();

		let user_id = UserId::generate();
		insert_test_user(&pool, &user_id).await;

		repo
			.add_member(&team.id, &user_id, TeamRole::Member)
			.await
			.unwrap();

		let membership = repo.get_membership(&team.id, &user_id).await.unwrap();
		assert!(membership.is_some());
		let membership = membership.unwrap();
		assert_eq!(membership.team_id, team.id);
		assert_eq!(membership.user_id, user_id);
		assert_eq!(membership.role, TeamRole::Member);

		let removed = repo.remove_member(&team.id, &user_id).await.unwrap();
		assert!(removed);

		let membership_after = repo.get_membership(&team.id, &user_id).await.unwrap();
		assert!(membership_after.is_none());
	}

	#[tokio::test]
	async fn test_list_teams_for_org() {
		let pool = create_team_test_pool().await;
		let repo = TeamRepository::new(pool.clone());

		let org1 = OrgId::generate();
		let org2 = OrgId::generate();
		insert_test_org(&pool, &org1).await;
		insert_test_org(&pool, &org2).await;

		let team1 = make_test_team(&org1, "team-a", "Team A");
		let team2 = make_test_team(&org1, "team-b", "Team B");
		let team3 = make_test_team(&org2, "team-c", "Team C");

		repo.create_team(&team1).await.unwrap();
		repo.create_team(&team2).await.unwrap();
		repo.create_team(&team3).await.unwrap();

		let org1_teams = repo.list_teams_for_org(&org1).await.unwrap();
		assert_eq!(org1_teams.len(), 2);

		let team_ids: HashSet<_> = org1_teams.iter().map(|t| t.id.clone()).collect();
		assert!(team_ids.contains(&team1.id));
		assert!(team_ids.contains(&team2.id));
		assert!(!team_ids.contains(&team3.id));

		let org2_teams = repo.list_teams_for_org(&org2).await.unwrap();
		assert_eq!(org2_teams.len(), 1);
		assert_eq!(org2_teams[0].id, team3.id);
	}
}
