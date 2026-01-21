// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loom_common_secret::SecretString;
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

use crate::error::DbError;

#[async_trait]
pub trait ScmStore: Send + Sync {
	async fn create_repo(&self, repo: &RepoRecord) -> Result<(), DbError>;
	async fn get_repo_by_id(&self, id: Uuid) -> Result<Option<RepoRecord>, DbError>;
	async fn get_repo_by_owner_and_name(
		&self,
		owner_type: &str,
		owner_id: Uuid,
		name: &str,
	) -> Result<Option<RepoRecord>, DbError>;
	async fn list_repos_by_owner(
		&self,
		owner_type: &str,
		owner_id: Uuid,
	) -> Result<Vec<RepoRecord>, DbError>;
	async fn update_repo(&self, repo: &RepoRecord) -> Result<(), DbError>;
	async fn soft_delete_repo(&self, id: Uuid) -> Result<(), DbError>;
	async fn hard_delete_repo(&self, id: Uuid) -> Result<(), DbError>;
	async fn list_all_repo_ids(&self) -> Result<Vec<Uuid>, DbError>;

	async fn grant_team_access(
		&self,
		repo_id: Uuid,
		team_id: Uuid,
		role: &str,
	) -> Result<(), DbError>;
	async fn revoke_team_access(&self, repo_id: Uuid, team_id: Uuid) -> Result<(), DbError>;
	async fn list_repo_team_access(
		&self,
		repo_id: Uuid,
	) -> Result<Vec<RepoTeamAccessRecord>, DbError>;
	async fn get_user_roles_via_teams(
		&self,
		user_id: Uuid,
		repo_id: Uuid,
	) -> Result<Vec<String>, DbError>;

	async fn create_webhook(&self, webhook: &WebhookRecord) -> Result<(), DbError>;
	async fn get_webhook_by_id(&self, id: Uuid) -> Result<Option<WebhookRecord>, DbError>;
	async fn list_webhooks_by_repo(&self, repo_id: Uuid) -> Result<Vec<WebhookRecord>, DbError>;
	async fn list_webhooks_by_org(&self, org_id: Uuid) -> Result<Vec<WebhookRecord>, DbError>;
	async fn delete_webhook(&self, id: Uuid) -> Result<(), DbError>;

	async fn create_webhook_delivery(&self, delivery: &WebhookDeliveryRecord) -> Result<(), DbError>;
	async fn update_webhook_delivery(&self, delivery: &WebhookDeliveryRecord) -> Result<(), DbError>;
	async fn get_pending_webhook_deliveries(&self) -> Result<Vec<WebhookDeliveryRecord>, DbError>;
	async fn get_webhook_for_delivery(
		&self,
		delivery_id: Uuid,
	) -> Result<Option<WebhookRecord>, DbError>;

	async fn create_maintenance_job(&self, job: &MaintenanceJobRecord) -> Result<(), DbError>;
	async fn get_maintenance_job_by_id(
		&self,
		id: Uuid,
	) -> Result<Option<MaintenanceJobRecord>, DbError>;
	async fn list_maintenance_jobs_by_repo(
		&self,
		repo_id: Uuid,
		limit: u32,
	) -> Result<Vec<MaintenanceJobRecord>, DbError>;
	async fn list_pending_maintenance_jobs(&self) -> Result<Vec<MaintenanceJobRecord>, DbError>;
	async fn update_maintenance_job_status(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError>;
	async fn mark_maintenance_job_started(&self, id: Uuid) -> Result<(), DbError>;
	async fn mark_maintenance_job_finished(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError>;
}

#[derive(Clone)]
pub struct ScmRepository {
	pool: SqlitePool,
}

impl ScmRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	// =========================================================================
	// Repository CRUD
	// =========================================================================

	#[tracing::instrument(skip(self, repo), fields(repo_id = %repo.id, name = %repo.name))]
	pub async fn create_repo(&self, repo: &RepoRecord) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO repos (id, owner_type, owner_id, name, visibility, default_branch, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(repo.id.to_string())
		.bind(&repo.owner_type)
		.bind(repo.owner_id.to_string())
		.bind(&repo.name)
		.bind(&repo.visibility)
		.bind(&repo.default_branch)
		.bind(repo.created_at.to_rfc3339())
		.bind(repo.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await
		.map_err(|e| match e {
			sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
				DbError::Conflict("Repository already exists".to_string())
			}
			_ => DbError::Sqlx(e),
		})?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %id))]
	pub async fn get_repo_by_id(&self, id: Uuid) -> Result<Option<RepoRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, name, visibility, default_branch, deleted_at, created_at, updated_at
			FROM repos
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_repo(&r)).transpose()
	}

	#[tracing::instrument(skip(self), fields(owner_type = %owner_type, owner_id = %owner_id, name = %name))]
	pub async fn get_repo_by_owner_and_name(
		&self,
		owner_type: &str,
		owner_id: Uuid,
		name: &str,
	) -> Result<Option<RepoRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, name, visibility, default_branch, deleted_at, created_at, updated_at
			FROM repos
			WHERE owner_type = ? AND owner_id = ? AND name = ? AND deleted_at IS NULL
			"#,
		)
		.bind(owner_type)
		.bind(owner_id.to_string())
		.bind(name)
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_repo(&r)).transpose()
	}

	#[tracing::instrument(skip(self), fields(owner_type = %owner_type, owner_id = %owner_id))]
	pub async fn list_repos_by_owner(
		&self,
		owner_type: &str,
		owner_id: Uuid,
	) -> Result<Vec<RepoRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, name, visibility, default_branch, deleted_at, created_at, updated_at
			FROM repos
			WHERE owner_type = ? AND owner_id = ? AND deleted_at IS NULL
			ORDER BY name ASC
			"#,
		)
		.bind(owner_type)
		.bind(owner_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_repo).collect()
	}

	#[tracing::instrument(skip(self, repo), fields(repo_id = %repo.id))]
	pub async fn update_repo(&self, repo: &RepoRecord) -> Result<(), DbError> {
		let updated_at = Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
			UPDATE repos
			SET name = ?, visibility = ?, default_branch = ?, updated_at = ?
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(&repo.name)
		.bind(&repo.visibility)
		.bind(&repo.default_branch)
		.bind(&updated_at)
		.bind(repo.id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Repository not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %id))]
	pub async fn soft_delete_repo(&self, id: Uuid) -> Result<(), DbError> {
		let deleted_at = Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
			UPDATE repos
			SET deleted_at = ?, updated_at = ?
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(&deleted_at)
		.bind(&deleted_at)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Repository not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %id))]
	pub async fn hard_delete_repo(&self, id: Uuid) -> Result<(), DbError> {
		let result = sqlx::query(r#"DELETE FROM repos WHERE id = ?"#)
			.bind(id.to_string())
			.execute(&self.pool)
			.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Repository not found".to_string()));
		}

		Ok(())
	}

	/// List all non-deleted repository IDs, ordered by creation time.
	/// Used for maintenance sweeps across all repositories.
	#[tracing::instrument(skip(self))]
	pub async fn list_all_repo_ids(&self) -> Result<Vec<Uuid>, DbError> {
		let rows = sqlx::query_scalar::<_, String>(
			r#"SELECT id FROM repos WHERE deleted_at IS NULL ORDER BY created_at ASC"#,
		)
		.fetch_all(&self.pool)
		.await?;

		rows
			.into_iter()
			.map(|id_str| Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string())))
			.collect()
	}

	// =========================================================================
	// Repo Team Access
	// =========================================================================

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id, team_id = %team_id, role = %role))]
	pub async fn grant_team_access(
		&self,
		repo_id: Uuid,
		team_id: Uuid,
		role: &str,
	) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO repo_team_access (repo_id, team_id, role)
			VALUES (?, ?, ?)
			ON CONFLICT (repo_id, team_id) DO UPDATE SET role = excluded.role
			"#,
		)
		.bind(repo_id.to_string())
		.bind(team_id.to_string())
		.bind(role)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id, team_id = %team_id))]
	pub async fn revoke_team_access(&self, repo_id: Uuid, team_id: Uuid) -> Result<(), DbError> {
		let result = sqlx::query(
			r#"
			DELETE FROM repo_team_access
			WHERE repo_id = ? AND team_id = ?
			"#,
		)
		.bind(repo_id.to_string())
		.bind(team_id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Team access not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id))]
	pub async fn list_repo_team_access(
		&self,
		repo_id: Uuid,
	) -> Result<Vec<RepoTeamAccessRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT repo_id, team_id, role
			FROM repo_team_access
			WHERE repo_id = ?
			"#,
		)
		.bind(repo_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_team_access).collect()
	}

	#[tracing::instrument(skip(self), fields(user_id = %user_id, repo_id = %repo_id))]
	pub async fn get_user_roles_via_teams(
		&self,
		user_id: Uuid,
		repo_id: Uuid,
	) -> Result<Vec<String>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT rta.role
			FROM repo_team_access rta
			INNER JOIN team_memberships tm ON rta.team_id = tm.team_id
			WHERE tm.user_id = ? AND rta.repo_id = ?
			"#,
		)
		.bind(user_id.to_string())
		.bind(repo_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		Ok(rows.iter().map(|r| r.get::<String, _>("role")).collect())
	}

	// =========================================================================
	// Webhook CRUD
	// =========================================================================

	#[tracing::instrument(skip(self, webhook), fields(webhook_id = %webhook.id))]
	pub async fn create_webhook(&self, webhook: &WebhookRecord) -> Result<(), DbError> {
		let events_json = serde_json::to_string(&webhook.events)?;

		sqlx::query(
			r#"
			INSERT INTO webhooks (id, owner_type, owner_id, url, secret, payload_format, events, enabled, created_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(webhook.id.to_string())
		.bind(&webhook.owner_type)
		.bind(webhook.owner_id.to_string())
		.bind(&webhook.url)
		.bind(webhook.secret.expose())
		.bind(&webhook.payload_format)
		.bind(&events_json)
		.bind(webhook.enabled as i32)
		.bind(webhook.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(webhook_id = %id))]
	pub async fn get_webhook_by_id(&self, id: Uuid) -> Result<Option<WebhookRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, url, secret, payload_format, events, enabled, created_at
			FROM webhooks
			WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_webhook(&r)).transpose()
	}

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id))]
	pub async fn list_webhooks_by_repo(&self, repo_id: Uuid) -> Result<Vec<WebhookRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, url, secret, payload_format, events, enabled, created_at
			FROM webhooks
			WHERE owner_type = 'repo' AND owner_id = ?
			ORDER BY created_at DESC
			"#,
		)
		.bind(repo_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_webhook).collect()
	}

	#[tracing::instrument(skip(self), fields(org_id = %org_id))]
	pub async fn list_webhooks_by_org(&self, org_id: Uuid) -> Result<Vec<WebhookRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, owner_type, owner_id, url, secret, payload_format, events, enabled, created_at
			FROM webhooks
			WHERE owner_type = 'org' AND owner_id = ?
			ORDER BY created_at DESC
			"#,
		)
		.bind(org_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_webhook).collect()
	}

	#[tracing::instrument(skip(self), fields(webhook_id = %id))]
	pub async fn delete_webhook(&self, id: Uuid) -> Result<(), DbError> {
		let result = sqlx::query(r#"DELETE FROM webhooks WHERE id = ?"#)
			.bind(id.to_string())
			.execute(&self.pool)
			.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Webhook not found".to_string()));
		}

		Ok(())
	}

	// =========================================================================
	// Webhook Deliveries
	// =========================================================================

	#[tracing::instrument(skip(self, delivery), fields(delivery_id = %delivery.id, webhook_id = %delivery.webhook_id))]
	pub async fn create_webhook_delivery(
		&self,
		delivery: &WebhookDeliveryRecord,
	) -> Result<(), DbError> {
		let payload_json = serde_json::to_string(&delivery.payload)?;

		sqlx::query(
			r#"
			INSERT INTO webhook_deliveries (id, webhook_id, event, payload, response_code, response_body, delivered_at, attempts, next_retry_at, status)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(delivery.id.to_string())
		.bind(delivery.webhook_id.to_string())
		.bind(&delivery.event)
		.bind(&payload_json)
		.bind(delivery.response_code)
		.bind(&delivery.response_body)
		.bind(delivery.delivered_at.map(|d| d.to_rfc3339()))
		.bind(delivery.attempts)
		.bind(delivery.next_retry_at.map(|d| d.to_rfc3339()))
		.bind(&delivery.status)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self, delivery), fields(delivery_id = %delivery.id))]
	pub async fn update_webhook_delivery(
		&self,
		delivery: &WebhookDeliveryRecord,
	) -> Result<(), DbError> {
		let result = sqlx::query(
			r#"
			UPDATE webhook_deliveries
			SET response_code = ?, response_body = ?, delivered_at = ?, attempts = ?, next_retry_at = ?, status = ?
			WHERE id = ?
			"#,
		)
		.bind(delivery.response_code)
		.bind(&delivery.response_body)
		.bind(delivery.delivered_at.map(|d| d.to_rfc3339()))
		.bind(delivery.attempts)
		.bind(delivery.next_retry_at.map(|d| d.to_rfc3339()))
		.bind(&delivery.status)
		.bind(delivery.id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Webhook delivery not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self))]
	pub async fn get_pending_webhook_deliveries(
		&self,
	) -> Result<Vec<WebhookDeliveryRecord>, DbError> {
		let now = Utc::now().to_rfc3339();
		let rows = sqlx::query(
			r#"
			SELECT id, webhook_id, event, payload, response_code, response_body, delivered_at, attempts, next_retry_at, status
			FROM webhook_deliveries
			WHERE status = 'pending' AND (next_retry_at IS NULL OR next_retry_at <= ?)
			ORDER BY next_retry_at ASC, id ASC
			LIMIT 100
			"#,
		)
		.bind(&now)
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_delivery).collect()
	}

	#[tracing::instrument(skip(self), fields(delivery_id = %delivery_id))]
	pub async fn get_webhook_for_delivery(
		&self,
		delivery_id: Uuid,
	) -> Result<Option<WebhookRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT w.id, w.owner_type, w.owner_id, w.url, w.secret, w.payload_format, w.events, w.enabled, w.created_at
			FROM webhooks w
			INNER JOIN webhook_deliveries d ON d.webhook_id = w.id
			WHERE d.id = ?
			"#,
		)
		.bind(delivery_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_webhook(&r)).transpose()
	}

	// =========================================================================
	// Maintenance Jobs
	// =========================================================================

	#[tracing::instrument(skip(self, job), fields(job_id = %job.id))]
	pub async fn create_maintenance_job(&self, job: &MaintenanceJobRecord) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO repo_maintenance_jobs (id, repo_id, task, status, started_at, finished_at, error, created_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(job.id.to_string())
		.bind(job.repo_id.map(|id| id.to_string()))
		.bind(&job.task)
		.bind(&job.status)
		.bind(job.started_at.map(|t| t.to_rfc3339()))
		.bind(job.finished_at.map(|t| t.to_rfc3339()))
		.bind(&job.error)
		.bind(job.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(job_id = %id))]
	pub async fn get_maintenance_job_by_id(
		&self,
		id: Uuid,
	) -> Result<Option<MaintenanceJobRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, repo_id, task, status, started_at, finished_at, error, created_at
			FROM repo_maintenance_jobs
			WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_maintenance_job(&r)).transpose()
	}

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id, limit = limit))]
	pub async fn list_maintenance_jobs_by_repo(
		&self,
		repo_id: Uuid,
		limit: u32,
	) -> Result<Vec<MaintenanceJobRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, repo_id, task, status, started_at, finished_at, error, created_at
			FROM repo_maintenance_jobs
			WHERE repo_id = ?
			ORDER BY created_at DESC
			LIMIT ?
			"#,
		)
		.bind(repo_id.to_string())
		.bind(limit as i64)
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_maintenance_job).collect()
	}

	#[tracing::instrument(skip(self))]
	pub async fn list_pending_maintenance_jobs(&self) -> Result<Vec<MaintenanceJobRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, repo_id, task, status, started_at, finished_at, error, created_at
			FROM repo_maintenance_jobs
			WHERE status = 'pending'
			ORDER BY created_at ASC
			"#,
		)
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_maintenance_job).collect()
	}

	#[tracing::instrument(skip(self), fields(job_id = %id, status = %status))]
	pub async fn update_maintenance_job_status(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError> {
		let result = sqlx::query(
			r#"
			UPDATE repo_maintenance_jobs
			SET status = ?, error = ?
			WHERE id = ?
			"#,
		)
		.bind(status)
		.bind(error)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Maintenance job not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(job_id = %id))]
	pub async fn mark_maintenance_job_started(&self, id: Uuid) -> Result<(), DbError> {
		let now = Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
			UPDATE repo_maintenance_jobs
			SET status = 'running', started_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&now)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Maintenance job not found".to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(job_id = %id, status = %status))]
	pub async fn mark_maintenance_job_finished(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError> {
		let now = Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
			UPDATE repo_maintenance_jobs
			SET status = ?, finished_at = ?, error = ?
			WHERE id = ?
			"#,
		)
		.bind(status)
		.bind(&now)
		.bind(error)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound("Maintenance job not found".to_string()));
		}

		Ok(())
	}
}

#[async_trait]
impl ScmStore for ScmRepository {
	async fn create_repo(&self, repo: &RepoRecord) -> Result<(), DbError> {
		ScmRepository::create_repo(self, repo).await
	}

	async fn get_repo_by_id(&self, id: Uuid) -> Result<Option<RepoRecord>, DbError> {
		ScmRepository::get_repo_by_id(self, id).await
	}

	async fn get_repo_by_owner_and_name(
		&self,
		owner_type: &str,
		owner_id: Uuid,
		name: &str,
	) -> Result<Option<RepoRecord>, DbError> {
		ScmRepository::get_repo_by_owner_and_name(self, owner_type, owner_id, name).await
	}

	async fn list_repos_by_owner(
		&self,
		owner_type: &str,
		owner_id: Uuid,
	) -> Result<Vec<RepoRecord>, DbError> {
		ScmRepository::list_repos_by_owner(self, owner_type, owner_id).await
	}

	async fn update_repo(&self, repo: &RepoRecord) -> Result<(), DbError> {
		ScmRepository::update_repo(self, repo).await
	}

	async fn soft_delete_repo(&self, id: Uuid) -> Result<(), DbError> {
		ScmRepository::soft_delete_repo(self, id).await
	}

	async fn hard_delete_repo(&self, id: Uuid) -> Result<(), DbError> {
		ScmRepository::hard_delete_repo(self, id).await
	}

	async fn list_all_repo_ids(&self) -> Result<Vec<Uuid>, DbError> {
		ScmRepository::list_all_repo_ids(self).await
	}

	async fn grant_team_access(
		&self,
		repo_id: Uuid,
		team_id: Uuid,
		role: &str,
	) -> Result<(), DbError> {
		ScmRepository::grant_team_access(self, repo_id, team_id, role).await
	}

	async fn revoke_team_access(&self, repo_id: Uuid, team_id: Uuid) -> Result<(), DbError> {
		ScmRepository::revoke_team_access(self, repo_id, team_id).await
	}

	async fn list_repo_team_access(
		&self,
		repo_id: Uuid,
	) -> Result<Vec<RepoTeamAccessRecord>, DbError> {
		ScmRepository::list_repo_team_access(self, repo_id).await
	}

	async fn get_user_roles_via_teams(
		&self,
		user_id: Uuid,
		repo_id: Uuid,
	) -> Result<Vec<String>, DbError> {
		ScmRepository::get_user_roles_via_teams(self, user_id, repo_id).await
	}

	async fn create_webhook(&self, webhook: &WebhookRecord) -> Result<(), DbError> {
		ScmRepository::create_webhook(self, webhook).await
	}

	async fn get_webhook_by_id(&self, id: Uuid) -> Result<Option<WebhookRecord>, DbError> {
		ScmRepository::get_webhook_by_id(self, id).await
	}

	async fn list_webhooks_by_repo(&self, repo_id: Uuid) -> Result<Vec<WebhookRecord>, DbError> {
		ScmRepository::list_webhooks_by_repo(self, repo_id).await
	}

	async fn list_webhooks_by_org(&self, org_id: Uuid) -> Result<Vec<WebhookRecord>, DbError> {
		ScmRepository::list_webhooks_by_org(self, org_id).await
	}

	async fn delete_webhook(&self, id: Uuid) -> Result<(), DbError> {
		ScmRepository::delete_webhook(self, id).await
	}

	async fn create_webhook_delivery(&self, delivery: &WebhookDeliveryRecord) -> Result<(), DbError> {
		ScmRepository::create_webhook_delivery(self, delivery).await
	}

	async fn update_webhook_delivery(&self, delivery: &WebhookDeliveryRecord) -> Result<(), DbError> {
		ScmRepository::update_webhook_delivery(self, delivery).await
	}

	async fn get_pending_webhook_deliveries(&self) -> Result<Vec<WebhookDeliveryRecord>, DbError> {
		ScmRepository::get_pending_webhook_deliveries(self).await
	}

	async fn get_webhook_for_delivery(
		&self,
		delivery_id: Uuid,
	) -> Result<Option<WebhookRecord>, DbError> {
		ScmRepository::get_webhook_for_delivery(self, delivery_id).await
	}

	async fn create_maintenance_job(&self, job: &MaintenanceJobRecord) -> Result<(), DbError> {
		ScmRepository::create_maintenance_job(self, job).await
	}

	async fn get_maintenance_job_by_id(
		&self,
		id: Uuid,
	) -> Result<Option<MaintenanceJobRecord>, DbError> {
		ScmRepository::get_maintenance_job_by_id(self, id).await
	}

	async fn list_maintenance_jobs_by_repo(
		&self,
		repo_id: Uuid,
		limit: u32,
	) -> Result<Vec<MaintenanceJobRecord>, DbError> {
		ScmRepository::list_maintenance_jobs_by_repo(self, repo_id, limit).await
	}

	async fn list_pending_maintenance_jobs(&self) -> Result<Vec<MaintenanceJobRecord>, DbError> {
		ScmRepository::list_pending_maintenance_jobs(self).await
	}

	async fn update_maintenance_job_status(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError> {
		ScmRepository::update_maintenance_job_status(self, id, status, error).await
	}

	async fn mark_maintenance_job_started(&self, id: Uuid) -> Result<(), DbError> {
		ScmRepository::mark_maintenance_job_started(self, id).await
	}

	async fn mark_maintenance_job_finished(
		&self,
		id: Uuid,
		status: &str,
		error: Option<&str>,
	) -> Result<(), DbError> {
		ScmRepository::mark_maintenance_job_finished(self, id, status, error).await
	}
}

// =========================================================================
// Record Types (plain data structs, no domain logic)
// =========================================================================

#[derive(Debug, Clone)]
pub struct RepoRecord {
	pub id: Uuid,
	pub owner_type: String,
	pub owner_id: Uuid,
	pub name: String,
	pub visibility: String,
	pub default_branch: String,
	pub deleted_at: Option<DateTime<Utc>>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RepoTeamAccessRecord {
	pub repo_id: Uuid,
	pub team_id: Uuid,
	pub role: String,
}

#[derive(Debug, Clone)]
pub struct WebhookRecord {
	pub id: Uuid,
	pub owner_type: String,
	pub owner_id: Uuid,
	pub url: String,
	pub secret: SecretString,
	pub payload_format: String,
	pub events: Vec<String>,
	pub enabled: bool,
	pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WebhookDeliveryRecord {
	pub id: Uuid,
	pub webhook_id: Uuid,
	pub event: String,
	pub payload: serde_json::Value,
	pub response_code: Option<i32>,
	pub response_body: Option<String>,
	pub delivered_at: Option<DateTime<Utc>>,
	pub attempts: i32,
	pub next_retry_at: Option<DateTime<Utc>>,
	pub status: String,
}

#[derive(Debug, Clone)]
pub struct MaintenanceJobRecord {
	pub id: Uuid,
	pub repo_id: Option<Uuid>,
	pub task: String,
	pub status: String,
	pub started_at: Option<DateTime<Utc>>,
	pub finished_at: Option<DateTime<Utc>>,
	pub error: Option<String>,
	pub created_at: DateTime<Utc>,
}

// =========================================================================
// Row Conversion Helpers
// =========================================================================

fn row_to_repo(row: &sqlx::sqlite::SqliteRow) -> Result<RepoRecord, DbError> {
	let id_str: String = row.get("id");
	let owner_id_str: String = row.get("owner_id");
	let deleted_at_str: Option<String> = row.get("deleted_at");
	let created_at_str: String = row.get("created_at");
	let updated_at_str: String = row.get("updated_at");

	Ok(RepoRecord {
		id: Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		owner_type: row.get("owner_type"),
		owner_id: Uuid::parse_str(&owner_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		name: row.get("name"),
		visibility: row.get("visibility"),
		default_branch: row.get("default_branch"),
		deleted_at: deleted_at_str
			.map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		created_at: DateTime::parse_from_rfc3339(&created_at_str)
			.map(|d| d.with_timezone(&Utc))
			.map_err(|e| DbError::Internal(e.to_string()))?,
		updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
			.map(|d| d.with_timezone(&Utc))
			.map_err(|e| DbError::Internal(e.to_string()))?,
	})
}

fn row_to_team_access(row: &sqlx::sqlite::SqliteRow) -> Result<RepoTeamAccessRecord, DbError> {
	let repo_id_str: String = row.get("repo_id");
	let team_id_str: String = row.get("team_id");

	Ok(RepoTeamAccessRecord {
		repo_id: Uuid::parse_str(&repo_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		team_id: Uuid::parse_str(&team_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		role: row.get("role"),
	})
}

fn row_to_webhook(row: &sqlx::sqlite::SqliteRow) -> Result<WebhookRecord, DbError> {
	let id_str: String = row.get("id");
	let owner_id_str: String = row.get("owner_id");
	let events_str: String = row.get("events");
	let created_at_str: String = row.get("created_at");

	Ok(WebhookRecord {
		id: Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		owner_type: row.get("owner_type"),
		owner_id: Uuid::parse_str(&owner_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		url: row.get("url"),
		secret: SecretString::new(row.get("secret")),
		payload_format: row.get("payload_format"),
		events: serde_json::from_str(&events_str)?,
		enabled: row.get::<i32, _>("enabled") != 0,
		created_at: DateTime::parse_from_rfc3339(&created_at_str)
			.map(|d| d.with_timezone(&Utc))
			.map_err(|e| DbError::Internal(e.to_string()))?,
	})
}

fn row_to_delivery(row: &sqlx::sqlite::SqliteRow) -> Result<WebhookDeliveryRecord, DbError> {
	let id_str: String = row.get("id");
	let webhook_id_str: String = row.get("webhook_id");
	let payload_str: String = row.get("payload");
	let delivered_at_str: Option<String> = row.get("delivered_at");
	let next_retry_at_str: Option<String> = row.get("next_retry_at");

	Ok(WebhookDeliveryRecord {
		id: Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		webhook_id: Uuid::parse_str(&webhook_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		event: row.get("event"),
		payload: serde_json::from_str(&payload_str)?,
		response_code: row.get("response_code"),
		response_body: row.get("response_body"),
		delivered_at: delivered_at_str
			.map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		attempts: row.get("attempts"),
		next_retry_at: next_retry_at_str
			.map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		status: row.get("status"),
	})
}

fn row_to_maintenance_job(row: &sqlx::sqlite::SqliteRow) -> Result<MaintenanceJobRecord, DbError> {
	let id_str: String = row.get("id");
	let repo_id_str: Option<String> = row.get("repo_id");
	let started_at_str: Option<String> = row.get("started_at");
	let finished_at_str: Option<String> = row.get("finished_at");
	let created_at_str: String = row.get("created_at");

	Ok(MaintenanceJobRecord {
		id: Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		repo_id: repo_id_str
			.map(|s| Uuid::parse_str(&s))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		task: row.get("task"),
		status: row.get("status"),
		started_at: started_at_str
			.map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		finished_at: finished_at_str
			.map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
			.transpose()
			.map_err(|e| DbError::Internal(e.to_string()))?,
		error: row.get("error"),
		created_at: DateTime::parse_from_rfc3339(&created_at_str)
			.map(|d| d.with_timezone(&Utc))
			.map_err(|e| DbError::Internal(e.to_string()))?,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn make_repo() -> ScmRepository {
		let pool = crate::testing::create_scm_test_pool().await;
		ScmRepository::new(pool)
	}

	fn make_repo_record(id: Uuid, owner_id: Uuid, name: &str) -> RepoRecord {
		let now = Utc::now();
		RepoRecord {
			id,
			owner_type: "org".to_string(),
			owner_id,
			name: name.to_string(),
			visibility: "private".to_string(),
			default_branch: "cannon".to_string(),
			deleted_at: None,
			created_at: now,
			updated_at: now,
		}
	}

	#[tokio::test]
	async fn test_create_and_get_repo() {
		let repo = make_repo().await;
		let repo_id = Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef1234567890").unwrap();
		let org_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();

		let record = make_repo_record(repo_id, org_id, "my-project");
		repo.create_repo(&record).await.unwrap();

		let fetched = repo.get_repo_by_id(repo_id).await.unwrap();
		assert!(fetched.is_some());

		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, repo_id);
		assert_eq!(fetched.owner_type, "org");
		assert_eq!(fetched.owner_id, org_id);
		assert_eq!(fetched.name, "my-project");
		assert_eq!(fetched.visibility, "private");
		assert_eq!(fetched.default_branch, "cannon");
		assert!(fetched.deleted_at.is_none());
	}

	#[tokio::test]
	async fn test_get_repo_not_found() {
		let repo = make_repo().await;
		let nonexistent_id = Uuid::parse_str("deadbeef-dead-beef-dead-beefdeadbeef").unwrap();

		let result = repo.get_repo_by_id(nonexistent_id).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_list_repos_by_org() {
		let repo = make_repo().await;
		let org1_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
		let org2_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();

		let repo1_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
		let repo2_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
		let repo3_id = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc").unwrap();

		repo
			.create_repo(&make_repo_record(repo1_id, org1_id, "alpha"))
			.await
			.unwrap();
		repo
			.create_repo(&make_repo_record(repo2_id, org1_id, "beta"))
			.await
			.unwrap();
		repo
			.create_repo(&make_repo_record(repo3_id, org2_id, "gamma"))
			.await
			.unwrap();

		let org1_repos = repo.list_repos_by_owner("org", org1_id).await.unwrap();
		assert_eq!(org1_repos.len(), 2);
		assert_eq!(org1_repos[0].name, "alpha");
		assert_eq!(org1_repos[1].name, "beta");

		let org2_repos = repo.list_repos_by_owner("org", org2_id).await.unwrap();
		assert_eq!(org2_repos.len(), 1);
		assert_eq!(org2_repos[0].name, "gamma");
	}

	#[tokio::test]
	async fn test_list_all_repo_ids() {
		let repo = make_repo().await;
		let org_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();

		let repo1_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
		let repo2_id = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
		let repo3_id = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc").unwrap();

		repo
			.create_repo(&make_repo_record(repo1_id, org_id, "repo-one"))
			.await
			.unwrap();
		repo
			.create_repo(&make_repo_record(repo2_id, org_id, "repo-two"))
			.await
			.unwrap();
		repo
			.create_repo(&make_repo_record(repo3_id, org_id, "repo-three"))
			.await
			.unwrap();

		let all_ids = repo.list_all_repo_ids().await.unwrap();
		assert_eq!(all_ids.len(), 3);
		assert!(all_ids.contains(&repo1_id));
		assert!(all_ids.contains(&repo2_id));
		assert!(all_ids.contains(&repo3_id));
	}

	#[tokio::test]
	async fn test_soft_delete_excludes_from_queries() {
		let repo = make_repo().await;
		let org_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
		let repo_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();

		repo
			.create_repo(&make_repo_record(repo_id, org_id, "to-delete"))
			.await
			.unwrap();

		assert!(repo.get_repo_by_id(repo_id).await.unwrap().is_some());

		repo.soft_delete_repo(repo_id).await.unwrap();

		assert!(repo.get_repo_by_id(repo_id).await.unwrap().is_none());

		let all_ids = repo.list_all_repo_ids().await.unwrap();
		assert!(!all_ids.contains(&repo_id));
	}

	#[tokio::test]
	async fn test_get_repo_by_owner_and_name() {
		let repo = make_repo().await;
		let org_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
		let repo_id = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();

		repo
			.create_repo(&make_repo_record(repo_id, org_id, "unique-name"))
			.await
			.unwrap();

		let found = repo
			.get_repo_by_owner_and_name("org", org_id, "unique-name")
			.await
			.unwrap();
		assert!(found.is_some());
		assert_eq!(found.unwrap().id, repo_id);

		let not_found = repo
			.get_repo_by_owner_and_name("org", org_id, "nonexistent")
			.await
			.unwrap();
		assert!(not_found.is_none());
	}
}
