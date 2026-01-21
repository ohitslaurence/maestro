// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::{DbError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
	Running,
	Succeeded,
	Failed,
	Cancelled,
}

impl JobStatus {
	pub fn as_str(&self) -> &'static str {
		match self {
			JobStatus::Running => "running",
			JobStatus::Succeeded => "succeeded",
			JobStatus::Failed => "failed",
			JobStatus::Cancelled => "cancelled",
		}
	}
}

impl std::str::FromStr for JobStatus {
	type Err = String;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		match s {
			"running" => Ok(JobStatus::Running),
			"succeeded" => Ok(JobStatus::Succeeded),
			"failed" => Ok(JobStatus::Failed),
			"cancelled" => Ok(JobStatus::Cancelled),
			_ => Err(format!("unknown job status: {s}")),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
	Schedule,
	Manual,
	Retry,
}

impl TriggerSource {
	pub fn as_str(&self) -> &'static str {
		match self {
			TriggerSource::Schedule => "schedule",
			TriggerSource::Manual => "manual",
			TriggerSource::Retry => "retry",
		}
	}
}

impl std::str::FromStr for TriggerSource {
	type Err = String;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		match s {
			"schedule" => Ok(TriggerSource::Schedule),
			"manual" => Ok(TriggerSource::Manual),
			"retry" => Ok(TriggerSource::Retry),
			_ => Err(format!("unknown trigger source: {s}")),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDefinition {
	pub id: String,
	pub name: String,
	pub description: String,
	pub job_type: String,
	pub interval_secs: Option<i64>,
	pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
	pub id: String,
	pub job_id: String,
	pub status: JobStatus,
	pub started_at: DateTime<Utc>,
	pub completed_at: Option<DateTime<Utc>>,
	pub duration_ms: Option<i64>,
	pub error_message: Option<String>,
	pub retry_count: u32,
	pub triggered_by: TriggerSource,
	pub metadata: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct JobRepository {
	pool: SqlitePool,
}

impl JobRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	#[tracing::instrument(skip(self, def), fields(job_id = %def.id))]
	pub async fn upsert_definition(&self, def: &JobDefinition) -> Result<()> {
		let now = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);
		sqlx::query(
			r#"
            INSERT INTO job_definitions (id, name, description, job_type, interval_secs, enabled, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                job_type = excluded.job_type,
                interval_secs = excluded.interval_secs,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            "#,
		)
		.bind(&def.id)
		.bind(&def.name)
		.bind(&def.description)
		.bind(&def.job_type)
		.bind(def.interval_secs)
		.bind(def.enabled)
		.bind(&now)
		.bind(&now)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self))]
	pub async fn get_definition(&self, id: &str) -> Result<Option<JobDefinition>> {
		let row = sqlx::query_as::<_, (String, String, String, String, Option<i64>, bool)>(
			"SELECT id, name, description, job_type, interval_secs, enabled FROM job_definitions WHERE id = ?",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;

		Ok(row.map(
			|(id, name, description, job_type, interval_secs, enabled)| JobDefinition {
				id,
				name,
				description,
				job_type,
				interval_secs,
				enabled,
			},
		))
	}

	#[tracing::instrument(skip(self))]
	pub async fn list_definitions(&self) -> Result<Vec<JobDefinition>> {
		let rows = sqlx::query_as::<_, (String, String, String, String, Option<i64>, bool)>(
			"SELECT id, name, description, job_type, interval_secs, enabled FROM job_definitions ORDER BY name",
		)
		.fetch_all(&self.pool)
		.await?;

		Ok(
			rows
				.into_iter()
				.map(
					|(id, name, description, job_type, interval_secs, enabled)| JobDefinition {
						id,
						name,
						description,
						job_type,
						interval_secs,
						enabled,
					},
				)
				.collect(),
		)
	}

	#[tracing::instrument(skip(self))]
	pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
		let result = sqlx::query("UPDATE job_definitions SET enabled = ? WHERE id = ?")
			.bind(enabled)
			.bind(id)
			.execute(&self.pool)
			.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound(id.to_string()));
		}

		Ok(())
	}

	#[tracing::instrument(skip(self, run), fields(run_id = %run.id, job_id = %run.job_id))]
	pub async fn record_run_start(&self, run: &JobRun) -> Result<()> {
		sqlx::query(
			r#"
            INSERT INTO job_runs (id, job_id, status, started_at, retry_count, triggered_by)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
		)
		.bind(&run.id)
		.bind(&run.job_id)
		.bind(run.status.as_str())
		.bind(run.started_at)
		.bind(run.retry_count as i64)
		.bind(run.triggered_by.as_str())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self, metadata))]
	pub async fn record_run_complete(
		&self,
		run_id: &str,
		status: JobStatus,
		error: Option<String>,
		metadata: Option<serde_json::Value>,
	) -> Result<()> {
		let now = Utc::now();
		let metadata_str = metadata.map(|m| m.to_string());

		sqlx::query(
			r#"
            UPDATE job_runs
            SET status = ?,
                completed_at = ?,
                duration_ms = CAST((julianday(?) - julianday(started_at)) * 86400000 AS INTEGER),
                error_message = ?,
                metadata = ?
            WHERE id = ?
            "#,
		)
		.bind(status.as_str())
		.bind(now)
		.bind(now)
		.bind(error)
		.bind(metadata_str)
		.bind(run_id)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self))]
	pub async fn get_run(&self, run_id: &str) -> Result<Option<JobRun>> {
		let row = sqlx::query_as::<_, (String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<i64>, Option<String>, i64, String, Option<String>)>(
            r#"
            SELECT id, job_id, status, started_at, completed_at, duration_ms, error_message, retry_count, triggered_by, metadata
            FROM job_runs
            WHERE id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;

		row
			.map(
				|(
					id,
					job_id,
					status,
					started_at,
					completed_at,
					duration_ms,
					error_message,
					retry_count,
					triggered_by,
					metadata,
				)| {
					Ok(JobRun {
						id,
						job_id,
						status: status.parse().map_err(|e: String| DbError::Internal(e))?,
						started_at,
						completed_at,
						duration_ms,
						error_message,
						retry_count: retry_count as u32,
						triggered_by: triggered_by
							.parse()
							.map_err(|e: String| DbError::Internal(e))?,
						metadata: metadata
							.as_deref()
							.and_then(|s| serde_json::from_str(s).ok()),
					})
				},
			)
			.transpose()
	}

	#[tracing::instrument(skip(self))]
	pub async fn list_runs(&self, job_id: &str, limit: u32, offset: u32) -> Result<Vec<JobRun>> {
		let rows = sqlx::query_as::<_, (String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<i64>, Option<String>, i64, String, Option<String>)>(
            r#"
            SELECT id, job_id, status, started_at, completed_at, duration_ms, error_message, retry_count, triggered_by, metadata
            FROM job_runs
            WHERE job_id = ?
            ORDER BY started_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(job_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;

		rows
			.into_iter()
			.map(
				|(
					id,
					job_id,
					status,
					started_at,
					completed_at,
					duration_ms,
					error_message,
					retry_count,
					triggered_by,
					metadata,
				)| {
					Ok(JobRun {
						id,
						job_id,
						status: status.parse().map_err(|e: String| DbError::Internal(e))?,
						started_at,
						completed_at,
						duration_ms,
						error_message,
						retry_count: retry_count as u32,
						triggered_by: triggered_by
							.parse()
							.map_err(|e: String| DbError::Internal(e))?,
						metadata: metadata
							.as_deref()
							.and_then(|s| serde_json::from_str(s).ok()),
					})
				},
			)
			.collect()
	}

	#[tracing::instrument(skip(self))]
	pub async fn get_last_run(&self, job_id: &str) -> Result<Option<JobRun>> {
		let row = sqlx::query_as::<_, (String, String, String, DateTime<Utc>, Option<DateTime<Utc>>, Option<i64>, Option<String>, i64, String, Option<String>)>(
            r#"
            SELECT id, job_id, status, started_at, completed_at, duration_ms, error_message, retry_count, triggered_by, metadata
            FROM job_runs
            WHERE job_id = ?
            ORDER BY started_at DESC
            LIMIT 1
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

		row
			.map(
				|(
					id,
					job_id,
					status,
					started_at,
					completed_at,
					duration_ms,
					error_message,
					retry_count,
					triggered_by,
					metadata,
				)| {
					Ok(JobRun {
						id,
						job_id,
						status: status.parse().map_err(|e: String| DbError::Internal(e))?,
						started_at,
						completed_at,
						duration_ms,
						error_message,
						retry_count: retry_count as u32,
						triggered_by: triggered_by
							.parse()
							.map_err(|e: String| DbError::Internal(e))?,
						metadata: metadata
							.as_deref()
							.and_then(|s| serde_json::from_str(s).ok()),
					})
				},
			)
			.transpose()
	}

	#[tracing::instrument(skip(self))]
	pub async fn count_consecutive_failures(&self, job_id: &str) -> Result<u32> {
		let row = sqlx::query_as::<_, (i64,)>(
			r#"
            WITH ranked AS (
                SELECT status,
                       ROW_NUMBER() OVER (ORDER BY started_at DESC) as rn
                FROM job_runs
                WHERE job_id = ?
            )
            SELECT COUNT(*) as count
            FROM ranked
            WHERE status = 'failed'
              AND rn <= (
                  SELECT COALESCE(MIN(rn) - 1, (SELECT COUNT(*) FROM ranked))
                  FROM ranked
                  WHERE status != 'failed'
              )
            "#,
		)
		.bind(job_id)
		.fetch_one(&self.pool)
		.await?;

		Ok(row.0 as u32)
	}

	#[tracing::instrument(skip(self))]
	pub async fn delete_old_runs(&self, before: DateTime<Utc>) -> Result<u64> {
		let result = sqlx::query("DELETE FROM job_runs WHERE completed_at < ?")
			.bind(before)
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected())
	}

	#[tracing::instrument(skip(self))]
	pub async fn cleanup_old_runs(&self, retention_days: u32) -> Result<u64> {
		let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
		self.delete_old_runs(cutoff).await
	}
}

#[async_trait]
pub trait JobStore: Send + Sync {
	async fn upsert_definition(&self, def: &JobDefinition) -> Result<()>;
	async fn get_definition(&self, id: &str) -> Result<Option<JobDefinition>>;
	async fn list_definitions(&self) -> Result<Vec<JobDefinition>>;
	async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()>;
	async fn record_run_start(&self, run: &JobRun) -> Result<()>;
	async fn record_run_complete(
		&self,
		run_id: &str,
		status: JobStatus,
		error: Option<String>,
		metadata: Option<serde_json::Value>,
	) -> Result<()>;
	async fn get_run(&self, run_id: &str) -> Result<Option<JobRun>>;
	async fn list_runs(&self, job_id: &str, limit: u32, offset: u32) -> Result<Vec<JobRun>>;
	async fn get_last_run(&self, job_id: &str) -> Result<Option<JobRun>>;
	async fn count_consecutive_failures(&self, job_id: &str) -> Result<u32>;
	async fn delete_old_runs(&self, before: DateTime<Utc>) -> Result<u64>;
	async fn cleanup_old_runs(&self, retention_days: u32) -> Result<u64>;
}

#[async_trait]
impl JobStore for JobRepository {
	async fn upsert_definition(&self, def: &JobDefinition) -> Result<()> {
		self.upsert_definition(def).await
	}

	async fn get_definition(&self, id: &str) -> Result<Option<JobDefinition>> {
		self.get_definition(id).await
	}

	async fn list_definitions(&self) -> Result<Vec<JobDefinition>> {
		self.list_definitions().await
	}

	async fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
		self.set_enabled(id, enabled).await
	}

	async fn record_run_start(&self, run: &JobRun) -> Result<()> {
		self.record_run_start(run).await
	}

	async fn record_run_complete(
		&self,
		run_id: &str,
		status: JobStatus,
		error: Option<String>,
		metadata: Option<serde_json::Value>,
	) -> Result<()> {
		self
			.record_run_complete(run_id, status, error, metadata)
			.await
	}

	async fn get_run(&self, run_id: &str) -> Result<Option<JobRun>> {
		self.get_run(run_id).await
	}

	async fn list_runs(&self, job_id: &str, limit: u32, offset: u32) -> Result<Vec<JobRun>> {
		self.list_runs(job_id, limit, offset).await
	}

	async fn get_last_run(&self, job_id: &str) -> Result<Option<JobRun>> {
		self.get_last_run(job_id).await
	}

	async fn count_consecutive_failures(&self, job_id: &str) -> Result<u32> {
		self.count_consecutive_failures(job_id).await
	}

	async fn delete_old_runs(&self, before: DateTime<Utc>) -> Result<u64> {
		self.delete_old_runs(before).await
	}

	async fn cleanup_old_runs(&self, retention_days: u32) -> Result<u64> {
		self.cleanup_old_runs(retention_days).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::create_job_test_pool;

	fn make_definition(id: &str, name: &str) -> JobDefinition {
		JobDefinition {
			id: id.to_string(),
			name: name.to_string(),
			description: "Test description".to_string(),
			job_type: "periodic".to_string(),
			interval_secs: Some(60),
			enabled: true,
		}
	}

	#[tokio::test]
	async fn test_upsert_and_get_definition() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		let retrieved = repo.get_definition("job-1").await.unwrap().unwrap();
		assert_eq!(retrieved.id, "job-1");
		assert_eq!(retrieved.name, "Test Job");
		assert!(retrieved.enabled);

		let updated = JobDefinition {
			name: "Updated Job".to_string(),
			enabled: false,
			..def
		};
		repo.upsert_definition(&updated).await.unwrap();

		let retrieved = repo.get_definition("job-1").await.unwrap().unwrap();
		assert_eq!(retrieved.name, "Updated Job");
		assert!(!retrieved.enabled);
	}

	#[tokio::test]
	async fn test_list_definitions() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		repo
			.upsert_definition(&make_definition("job-1", "Alpha"))
			.await
			.unwrap();
		repo
			.upsert_definition(&make_definition("job-2", "Beta"))
			.await
			.unwrap();
		repo
			.upsert_definition(&make_definition("job-3", "Gamma"))
			.await
			.unwrap();

		let defs = repo.list_definitions().await.unwrap();
		assert_eq!(defs.len(), 3);
		assert_eq!(defs[0].name, "Alpha");
		assert_eq!(defs[1].name, "Beta");
		assert_eq!(defs[2].name, "Gamma");
	}

	#[tokio::test]
	async fn test_set_enabled() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		repo.set_enabled("job-1", false).await.unwrap();
		let retrieved = repo.get_definition("job-1").await.unwrap().unwrap();
		assert!(!retrieved.enabled);

		repo.set_enabled("job-1", true).await.unwrap();
		let retrieved = repo.get_definition("job-1").await.unwrap().unwrap();
		assert!(retrieved.enabled);
	}

	#[tokio::test]
	async fn test_set_enabled_not_found() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let result = repo.set_enabled("nonexistent", true).await;
		assert!(matches!(result, Err(DbError::NotFound(_))));
	}

	#[tokio::test]
	async fn test_record_and_get_run() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		let run = JobRun {
			id: "run-1".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now(),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&run).await.unwrap();

		let retrieved = repo.get_run("run-1").await.unwrap().unwrap();
		assert_eq!(retrieved.id, "run-1");
		assert_eq!(retrieved.job_id, "job-1");
		assert_eq!(retrieved.status, JobStatus::Running);
		assert_eq!(retrieved.retry_count, 0);
	}

	#[tokio::test]
	async fn test_record_run_complete() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		let run = JobRun {
			id: "run-1".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now(),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&run).await.unwrap();

		repo
			.record_run_complete(
				"run-1",
				JobStatus::Failed,
				Some("Something went wrong".to_string()),
				None,
			)
			.await
			.unwrap();

		let completed = repo.get_run("run-1").await.unwrap().unwrap();
		assert_eq!(completed.status, JobStatus::Failed);
		assert_eq!(
			completed.error_message.as_deref(),
			Some("Something went wrong")
		);
	}

	#[tokio::test]
	async fn test_get_last_run() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		assert!(repo.get_last_run("job-1").await.unwrap().is_none());

		let run1 = JobRun {
			id: "run-1".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now() - chrono::Duration::hours(1),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&run1).await.unwrap();

		let run2 = JobRun {
			id: "run-2".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now(),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&run2).await.unwrap();

		let last = repo.get_last_run("job-1").await.unwrap().unwrap();
		assert_eq!(last.id, "run-2");
	}

	#[tokio::test]
	async fn test_count_consecutive_failures_all_failed() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		for i in 0..3 {
			let run = JobRun {
				id: format!("run-{i}"),
				job_id: "job-1".to_string(),
				status: JobStatus::Failed,
				started_at: Utc::now() - chrono::Duration::minutes(3 - i),
				completed_at: Some(Utc::now() - chrono::Duration::minutes(3 - i)),
				duration_ms: Some(100),
				error_message: Some("Error".to_string()),
				retry_count: 0,
				triggered_by: TriggerSource::Schedule,
				metadata: None,
			};
			repo.record_run_start(&run).await.unwrap();
			repo
				.record_run_complete(&run.id, JobStatus::Failed, Some("Error".to_string()), None)
				.await
				.unwrap();
		}

		let count = repo.count_consecutive_failures("job-1").await.unwrap();
		assert_eq!(count, 3);
	}

	#[tokio::test]
	async fn test_count_consecutive_failures_with_success() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		let success_run = JobRun {
			id: "run-0".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Succeeded,
			started_at: Utc::now() - chrono::Duration::minutes(10),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&success_run).await.unwrap();
		repo
			.record_run_complete("run-0", JobStatus::Succeeded, None, None)
			.await
			.unwrap();

		for i in 1..=2 {
			let run = JobRun {
				id: format!("run-{i}"),
				job_id: "job-1".to_string(),
				status: JobStatus::Failed,
				started_at: Utc::now() - chrono::Duration::minutes(5 - i as i64),
				completed_at: None,
				duration_ms: None,
				error_message: None,
				retry_count: 0,
				triggered_by: TriggerSource::Schedule,
				metadata: None,
			};
			repo.record_run_start(&run).await.unwrap();
			repo
				.record_run_complete(&run.id, JobStatus::Failed, Some("Error".to_string()), None)
				.await
				.unwrap();
		}

		let count = repo.count_consecutive_failures("job-1").await.unwrap();
		assert_eq!(count, 2);
	}

	#[tokio::test]
	async fn test_count_consecutive_failures_no_runs() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool);

		let count = repo
			.count_consecutive_failures("nonexistent")
			.await
			.unwrap();
		assert_eq!(count, 0);
	}

	#[tokio::test]
	async fn test_cleanup_old_runs() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool.clone());

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		let old_run = JobRun {
			id: "old-run".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now() - chrono::Duration::days(10),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&old_run).await.unwrap();
		repo
			.record_run_complete("old-run", JobStatus::Succeeded, None, None)
			.await
			.unwrap();

		sqlx::query("UPDATE job_runs SET completed_at = ? WHERE id = ?")
			.bind(Utc::now() - chrono::Duration::days(10))
			.bind("old-run")
			.execute(&pool)
			.await
			.unwrap();

		let new_run = JobRun {
			id: "new-run".to_string(),
			job_id: "job-1".to_string(),
			status: JobStatus::Running,
			started_at: Utc::now(),
			completed_at: None,
			duration_ms: None,
			error_message: None,
			retry_count: 0,
			triggered_by: TriggerSource::Schedule,
			metadata: None,
		};
		repo.record_run_start(&new_run).await.unwrap();
		repo
			.record_run_complete("new-run", JobStatus::Succeeded, None, None)
			.await
			.unwrap();

		let deleted = repo.cleanup_old_runs(7).await.unwrap();
		assert_eq!(deleted, 1);

		assert!(repo.get_run("old-run").await.unwrap().is_none());
		assert!(repo.get_run("new-run").await.unwrap().is_some());
	}

	#[tokio::test]
	async fn test_delete_old_runs_before_cutoff() {
		let pool = create_job_test_pool().await;
		let repo = JobRepository::new(pool.clone());

		let def = make_definition("job-1", "Test Job");
		repo.upsert_definition(&def).await.unwrap();

		for i in 0..5 {
			let run = JobRun {
				id: format!("run-{i}"),
				job_id: "job-1".to_string(),
				status: JobStatus::Running,
				started_at: Utc::now() - chrono::Duration::days(i + 1),
				completed_at: None,
				duration_ms: None,
				error_message: None,
				retry_count: 0,
				triggered_by: TriggerSource::Schedule,
				metadata: None,
			};
			repo.record_run_start(&run).await.unwrap();
			repo
				.record_run_complete(&run.id, JobStatus::Succeeded, None, None)
				.await
				.unwrap();

			sqlx::query("UPDATE job_runs SET completed_at = ? WHERE id = ?")
				.bind(Utc::now() - chrono::Duration::days(i + 1))
				.bind(&run.id)
				.execute(&pool)
				.await
				.unwrap();
		}

		let cutoff = Utc::now() - chrono::Duration::days(3);
		let deleted = repo.delete_old_runs(cutoff).await.unwrap();
		assert_eq!(deleted, 3);

		let remaining = repo.list_runs("job-1", 10, 0).await.unwrap();
		assert_eq!(remaining.len(), 2);
	}
}
