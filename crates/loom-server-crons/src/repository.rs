// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Repository layer for crons database operations.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::instrument;

use loom_crons_core::{
	CheckIn, CheckInId, CheckInStatus, Monitor, MonitorHealth, MonitorId, MonitorSchedule, OrgId,
};

use crate::error::{CronsServerError, Result};

/// Repository trait for crons operations.
#[async_trait]
pub trait CronsRepository: Send + Sync {
	// Monitor operations
	async fn create_monitor(&self, monitor: &Monitor) -> Result<()>;
	async fn get_monitor_by_id(&self, id: MonitorId) -> Result<Option<Monitor>>;
	async fn get_monitor_by_slug(&self, org_id: OrgId, slug: &str) -> Result<Option<Monitor>>;
	async fn get_monitor_by_ping_key(&self, ping_key: &str) -> Result<Option<Monitor>>;
	async fn list_monitors(&self, org_id: OrgId) -> Result<Vec<Monitor>>;
	async fn list_all_active_monitors(&self) -> Result<Vec<Monitor>>;
	async fn update_monitor(&self, monitor: &Monitor) -> Result<()>;
	async fn delete_monitor(&self, id: MonitorId) -> Result<bool>;

	// Check-in operations
	async fn create_checkin(&self, checkin: &CheckIn) -> Result<()>;
	async fn get_checkin_by_id(&self, id: CheckInId) -> Result<Option<CheckIn>>;
	async fn list_checkins(&self, monitor_id: MonitorId, limit: u32) -> Result<Vec<CheckIn>>;
	async fn update_checkin(&self, checkin: &CheckIn) -> Result<()>;

	// Monitor state updates
	async fn update_monitor_health(&self, id: MonitorId, health: MonitorHealth) -> Result<()>;
	async fn update_monitor_last_checkin(
		&self,
		id: MonitorId,
		status: CheckInStatus,
		next_expected_at: Option<chrono::DateTime<Utc>>,
	) -> Result<()>;
	async fn increment_monitor_stats(&self, id: MonitorId, is_failure: bool) -> Result<()>;

	// Background job queries
	/// Find monitors that are overdue (next_expected_at + margin < now) and haven't received a check-in since.
	async fn list_overdue_monitors(&self, now: chrono::DateTime<Utc>) -> Result<Vec<Monitor>>;

	/// Find in-progress check-ins that have exceeded the monitor's max_runtime_minutes.
	async fn list_timed_out_checkins(
		&self,
		now: chrono::DateTime<Utc>,
	) -> Result<Vec<(CheckIn, Monitor)>>;
}

/// SQLite implementation of the crons repository.
#[derive(Clone)]
pub struct SqliteCronsRepository {
	pool: SqlitePool,
}

impl SqliteCronsRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}
}

#[async_trait]
impl CronsRepository for SqliteCronsRepository {
	#[instrument(skip(self, monitor), fields(monitor_id = %monitor.id, slug = %monitor.slug))]
	async fn create_monitor(&self, monitor: &Monitor) -> Result<()> {
		let environments_json = serde_json::to_string(&monitor.environments)?;

		sqlx::query(
			r#"
			INSERT INTO cron_monitors (
				id, org_id, slug, name, description,
				status, health,
				schedule_type, schedule_value, timezone,
				checkin_margin_minutes, max_runtime_minutes,
				ping_key, environments,
				last_checkin_at, last_checkin_status, next_expected_at,
				consecutive_failures, total_checkins, total_failures,
				created_at, updated_at
			)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(monitor.id.0.to_string())
		.bind(monitor.org_id.0.to_string())
		.bind(&monitor.slug)
		.bind(&monitor.name)
		.bind(&monitor.description)
		.bind(monitor.status.to_string())
		.bind(monitor.health.to_string())
		.bind(monitor.schedule.schedule_type())
		.bind(monitor.schedule.schedule_value())
		.bind(&monitor.timezone)
		.bind(monitor.checkin_margin_minutes as i32)
		.bind(monitor.max_runtime_minutes.map(|m| m as i32))
		.bind(&monitor.ping_key)
		.bind(environments_json)
		.bind(monitor.last_checkin_at.map(|dt| dt.to_rfc3339()))
		.bind(monitor.last_checkin_status.map(|s| s.to_string()))
		.bind(monitor.next_expected_at.map(|dt| dt.to_rfc3339()))
		.bind(monitor.consecutive_failures as i32)
		.bind(monitor.total_checkins as i64)
		.bind(monitor.total_failures as i64)
		.bind(monitor.created_at.to_rfc3339())
		.bind(monitor.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(monitor_id = %id))]
	async fn get_monitor_by_id(&self, id: MonitorId) -> Result<Option<Monitor>> {
		let row = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id, slug = %slug))]
	async fn get_monitor_by_slug(&self, org_id: OrgId, slug: &str) -> Result<Option<Monitor>> {
		let row = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE org_id = ? AND slug = ?
			"#,
		)
		.bind(org_id.0.to_string())
		.bind(slug)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(ping_key = %ping_key))]
	async fn get_monitor_by_ping_key(&self, ping_key: &str) -> Result<Option<Monitor>> {
		let row = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE ping_key = ?
			"#,
		)
		.bind(ping_key)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id))]
	async fn list_monitors(&self, org_id: OrgId) -> Result<Vec<Monitor>> {
		let rows = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE org_id = ?
			ORDER BY slug ASC
			"#,
		)
		.bind(org_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, monitor), fields(monitor_id = %monitor.id))]
	async fn update_monitor(&self, monitor: &Monitor) -> Result<()> {
		let environments_json = serde_json::to_string(&monitor.environments)?;

		sqlx::query(
			r#"
			UPDATE cron_monitors
			SET name = ?, description = ?,
				status = ?, health = ?,
				schedule_type = ?, schedule_value = ?, timezone = ?,
				checkin_margin_minutes = ?, max_runtime_minutes = ?,
				environments = ?,
				updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&monitor.name)
		.bind(&monitor.description)
		.bind(monitor.status.to_string())
		.bind(monitor.health.to_string())
		.bind(monitor.schedule.schedule_type())
		.bind(monitor.schedule.schedule_value())
		.bind(&monitor.timezone)
		.bind(monitor.checkin_margin_minutes as i32)
		.bind(monitor.max_runtime_minutes.map(|m| m as i32))
		.bind(environments_json)
		.bind(Utc::now().to_rfc3339())
		.bind(monitor.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(monitor_id = %id))]
	async fn delete_monitor(&self, id: MonitorId) -> Result<bool> {
		let result = sqlx::query("DELETE FROM cron_monitors WHERE id = ?")
			.bind(id.0.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected() > 0)
	}

	#[instrument(skip(self, checkin), fields(checkin_id = %checkin.id, monitor_id = %checkin.monitor_id))]
	async fn create_checkin(&self, checkin: &CheckIn) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO cron_checkins (
				id, monitor_id, status,
				started_at, finished_at, duration_ms,
				environment, release,
				exit_code, output, crash_event_id,
				source, created_at
			)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(checkin.id.0.to_string())
		.bind(checkin.monitor_id.0.to_string())
		.bind(checkin.status.to_string())
		.bind(checkin.started_at.map(|dt| dt.to_rfc3339()))
		.bind(checkin.finished_at.to_rfc3339())
		.bind(checkin.duration_ms.map(|d| d as i64))
		.bind(&checkin.environment)
		.bind(&checkin.release)
		.bind(checkin.exit_code)
		.bind(&checkin.output)
		.bind(&checkin.crash_event_id)
		.bind(checkin.source.to_string())
		.bind(checkin.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(checkin_id = %id))]
	async fn get_checkin_by_id(&self, id: CheckInId) -> Result<Option<CheckIn>> {
		let row = sqlx::query_as::<_, CheckInRow>(
			r#"
			SELECT id, monitor_id, status,
				   started_at, finished_at, duration_ms,
				   environment, release,
				   exit_code, output, crash_event_id,
				   source, created_at
			FROM cron_checkins
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(monitor_id = %monitor_id))]
	async fn list_checkins(&self, monitor_id: MonitorId, limit: u32) -> Result<Vec<CheckIn>> {
		let rows = sqlx::query_as::<_, CheckInRow>(
			r#"
			SELECT id, monitor_id, status,
				   started_at, finished_at, duration_ms,
				   environment, release,
				   exit_code, output, crash_event_id,
				   source, created_at
			FROM cron_checkins
			WHERE monitor_id = ?
			ORDER BY created_at DESC
			LIMIT ?
			"#,
		)
		.bind(monitor_id.0.to_string())
		.bind(limit as i64)
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, checkin), fields(checkin_id = %checkin.id))]
	async fn update_checkin(&self, checkin: &CheckIn) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE cron_checkins
			SET status = ?, finished_at = ?, duration_ms = ?,
				exit_code = ?, output = ?, crash_event_id = ?
			WHERE id = ?
			"#,
		)
		.bind(checkin.status.to_string())
		.bind(checkin.finished_at.to_rfc3339())
		.bind(checkin.duration_ms.map(|d| d as i64))
		.bind(checkin.exit_code)
		.bind(&checkin.output)
		.bind(&checkin.crash_event_id)
		.bind(checkin.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(monitor_id = %id, health = %health))]
	async fn update_monitor_health(&self, id: MonitorId, health: MonitorHealth) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE cron_monitors
			SET health = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(health.to_string())
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(monitor_id = %id, status = %status))]
	async fn update_monitor_last_checkin(
		&self,
		id: MonitorId,
		status: CheckInStatus,
		next_expected_at: Option<chrono::DateTime<Utc>>,
	) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE cron_monitors
			SET last_checkin_at = ?,
				last_checkin_status = ?,
				next_expected_at = ?,
				updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(Utc::now().to_rfc3339())
		.bind(status.to_string())
		.bind(next_expected_at.map(|dt| dt.to_rfc3339()))
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(monitor_id = %id, is_failure = is_failure))]
	async fn increment_monitor_stats(&self, id: MonitorId, is_failure: bool) -> Result<()> {
		if is_failure {
			sqlx::query(
				r#"
				UPDATE cron_monitors
				SET total_checkins = total_checkins + 1,
					total_failures = total_failures + 1,
					consecutive_failures = consecutive_failures + 1,
					updated_at = ?
				WHERE id = ?
				"#,
			)
			.bind(Utc::now().to_rfc3339())
			.bind(id.0.to_string())
			.execute(&self.pool)
			.await?;
		} else {
			sqlx::query(
				r#"
				UPDATE cron_monitors
				SET total_checkins = total_checkins + 1,
					consecutive_failures = 0,
					updated_at = ?
				WHERE id = ?
				"#,
			)
			.bind(Utc::now().to_rfc3339())
			.bind(id.0.to_string())
			.execute(&self.pool)
			.await?;
		}

		Ok(())
	}

	#[instrument(skip(self))]
	async fn list_all_active_monitors(&self) -> Result<Vec<Monitor>> {
		let rows = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE status = 'active'
			ORDER BY slug ASC
			"#,
		)
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self))]
	async fn list_overdue_monitors(&self, now: chrono::DateTime<Utc>) -> Result<Vec<Monitor>> {
		// Find monitors where:
		// 1. status = 'active'
		// 2. next_expected_at is not null
		// 3. next_expected_at + margin_minutes < now (overdue)
		// 4. Either no last_checkin_at, or last_checkin_at < next_expected_at (hasn't checked in since expectation)
		//
		// SQLite doesn't have native datetime arithmetic, so we use strftime to compare timestamps
		let now_str = now.to_rfc3339();
		let rows = sqlx::query_as::<_, MonitorRow>(
			r#"
			SELECT id, org_id, slug, name, description,
				   status, health,
				   schedule_type, schedule_value, timezone,
				   checkin_margin_minutes, max_runtime_minutes,
				   ping_key, environments,
				   last_checkin_at, last_checkin_status, next_expected_at,
				   consecutive_failures, total_checkins, total_failures,
				   created_at, updated_at
			FROM cron_monitors
			WHERE status = 'active'
			  AND next_expected_at IS NOT NULL
			  AND datetime(next_expected_at, '+' || checkin_margin_minutes || ' minutes') < datetime(?)
			  AND (last_checkin_at IS NULL OR last_checkin_at < next_expected_at)
			  AND health != 'missed'
			"#,
		)
		.bind(&now_str)
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self))]
	async fn list_timed_out_checkins(
		&self,
		now: chrono::DateTime<Utc>,
	) -> Result<Vec<(CheckIn, Monitor)>> {
		// Find check-ins where:
		// 1. status = 'in_progress'
		// 2. monitor has max_runtime_minutes set
		// 3. started_at + max_runtime_minutes < now
		let now_str = now.to_rfc3339();

		// We need to join checkins with monitors to get the max_runtime_minutes
		let rows = sqlx::query_as::<_, CheckInWithMonitorRow>(
			r#"
			SELECT
				c.id as checkin_id, c.monitor_id, c.status as checkin_status,
				c.started_at, c.finished_at, c.duration_ms,
				c.environment, c.release, c.exit_code, c.output, c.crash_event_id,
				c.source, c.created_at as checkin_created_at,
				m.id as monitor_row_id, m.org_id, m.slug, m.name, m.description,
				m.status as monitor_status, m.health,
				m.schedule_type, m.schedule_value, m.timezone,
				m.checkin_margin_minutes, m.max_runtime_minutes,
				m.ping_key, m.environments,
				m.last_checkin_at, m.last_checkin_status, m.next_expected_at,
				m.consecutive_failures, m.total_checkins, m.total_failures,
				m.created_at as monitor_created_at, m.updated_at as monitor_updated_at
			FROM cron_checkins c
			JOIN cron_monitors m ON m.id = c.monitor_id
			WHERE c.status = 'in_progress'
			  AND m.max_runtime_minutes IS NOT NULL
			  AND c.started_at IS NOT NULL
			  AND datetime(c.started_at, '+' || m.max_runtime_minutes || ' minutes') < datetime(?)
			"#,
		)
		.bind(&now_str)
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(|row| row.try_into()).collect()
	}
}

// Database row types for sqlx

#[derive(sqlx::FromRow)]
struct MonitorRow {
	id: String,
	org_id: String,
	slug: String,
	name: String,
	description: Option<String>,
	status: String,
	health: String,
	schedule_type: String,
	schedule_value: String,
	timezone: String,
	checkin_margin_minutes: i32,
	max_runtime_minutes: Option<i32>,
	ping_key: String,
	environments: String,
	last_checkin_at: Option<String>,
	last_checkin_status: Option<String>,
	next_expected_at: Option<String>,
	consecutive_failures: i32,
	total_checkins: i64,
	total_failures: i64,
	created_at: String,
	updated_at: String,
}

impl TryFrom<MonitorRow> for Monitor {
	type Error = CronsServerError;

	fn try_from(row: MonitorRow) -> Result<Self> {
		let environments: Vec<String> = serde_json::from_str(&row.environments)?;

		let schedule = match row.schedule_type.as_str() {
			"cron" => MonitorSchedule::Cron {
				expression: row.schedule_value,
			},
			"interval" => MonitorSchedule::Interval {
				minutes: row
					.schedule_value
					.parse()
					.map_err(|_| CronsServerError::Internal("Invalid interval value".to_string()))?,
			},
			_ => {
				return Err(CronsServerError::Internal(format!(
					"Unknown schedule type: {}",
					row.schedule_type
				)))
			}
		};

		Ok(Monitor {
			id: row
				.id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid monitor ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid org ID".to_string()))?,
			slug: row.slug,
			name: row.name,
			description: row.description,
			status: row
				.status
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid status".to_string()))?,
			health: row
				.health
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid health".to_string()))?,
			schedule,
			timezone: row.timezone,
			checkin_margin_minutes: row.checkin_margin_minutes as u32,
			max_runtime_minutes: row.max_runtime_minutes.map(|m| m as u32),
			ping_key: row.ping_key,
			environments,
			last_checkin_at: row
				.last_checkin_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid last_checkin_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			last_checkin_status: row
				.last_checkin_status
				.map(|s| {
					s.parse()
						.map_err(|_| CronsServerError::Internal("Invalid last_checkin_status".to_string()))
				})
				.transpose()?,
			next_expected_at: row
				.next_expected_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid next_expected_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			consecutive_failures: row.consecutive_failures as u32,
			total_checkins: row.total_checkins as u64,
			total_failures: row.total_failures as u64,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| CronsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
				.map_err(|_| CronsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct CheckInRow {
	id: String,
	monitor_id: String,
	status: String,
	started_at: Option<String>,
	finished_at: String,
	duration_ms: Option<i64>,
	environment: Option<String>,
	release: Option<String>,
	exit_code: Option<i32>,
	output: Option<String>,
	crash_event_id: Option<String>,
	source: String,
	created_at: String,
}

impl TryFrom<CheckInRow> for CheckIn {
	type Error = CronsServerError;

	fn try_from(row: CheckInRow) -> Result<Self> {
		Ok(CheckIn {
			id: row
				.id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid check-in ID".to_string()))?,
			monitor_id: row
				.monitor_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid monitor ID".to_string()))?,
			status: row
				.status
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid status".to_string()))?,
			started_at: row
				.started_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid started_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			finished_at: chrono::DateTime::parse_from_rfc3339(&row.finished_at)
				.map_err(|_| CronsServerError::Internal("Invalid finished_at".to_string()))?
				.with_timezone(&chrono::Utc),
			duration_ms: row.duration_ms.map(|d| d as u64),
			environment: row.environment,
			release: row.release,
			exit_code: row.exit_code,
			output: row.output,
			crash_event_id: row.crash_event_id,
			source: row
				.source
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid source".to_string()))?,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| CronsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

/// Joined row type for check-ins with their monitors (for timeout detection).
#[derive(sqlx::FromRow)]
struct CheckInWithMonitorRow {
	// Check-in fields
	checkin_id: String,
	monitor_id: String,
	checkin_status: String,
	started_at: Option<String>,
	finished_at: String,
	duration_ms: Option<i64>,
	environment: Option<String>,
	release: Option<String>,
	exit_code: Option<i32>,
	output: Option<String>,
	crash_event_id: Option<String>,
	source: String,
	checkin_created_at: String,
	// Monitor fields
	#[allow(dead_code)]
	monitor_row_id: String,
	org_id: String,
	slug: String,
	name: String,
	description: Option<String>,
	monitor_status: String,
	health: String,
	schedule_type: String,
	schedule_value: String,
	timezone: String,
	checkin_margin_minutes: i32,
	max_runtime_minutes: Option<i32>,
	ping_key: String,
	environments: String,
	last_checkin_at: Option<String>,
	last_checkin_status: Option<String>,
	next_expected_at: Option<String>,
	consecutive_failures: i32,
	total_checkins: i64,
	total_failures: i64,
	monitor_created_at: String,
	monitor_updated_at: String,
}

impl TryFrom<CheckInWithMonitorRow> for (CheckIn, Monitor) {
	type Error = CronsServerError;

	fn try_from(row: CheckInWithMonitorRow) -> Result<Self> {
		let checkin = CheckIn {
			id: row
				.checkin_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid check-in ID".to_string()))?,
			monitor_id: row
				.monitor_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid monitor ID".to_string()))?,
			status: row
				.checkin_status
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid status".to_string()))?,
			started_at: row
				.started_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid started_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			finished_at: chrono::DateTime::parse_from_rfc3339(&row.finished_at)
				.map_err(|_| CronsServerError::Internal("Invalid finished_at".to_string()))?
				.with_timezone(&chrono::Utc),
			duration_ms: row.duration_ms.map(|d| d as u64),
			environment: row.environment,
			release: row.release,
			exit_code: row.exit_code,
			output: row.output,
			crash_event_id: row.crash_event_id,
			source: row
				.source
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid source".to_string()))?,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.checkin_created_at)
				.map_err(|_| CronsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
		};

		let environments: Vec<String> = serde_json::from_str(&row.environments)?;

		let schedule = match row.schedule_type.as_str() {
			"cron" => MonitorSchedule::Cron {
				expression: row.schedule_value,
			},
			"interval" => MonitorSchedule::Interval {
				minutes: row
					.schedule_value
					.parse()
					.map_err(|_| CronsServerError::Internal("Invalid interval value".to_string()))?,
			},
			_ => {
				return Err(CronsServerError::Internal(format!(
					"Unknown schedule type: {}",
					row.schedule_type
				)))
			}
		};

		let monitor = Monitor {
			id: row
				.monitor_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid monitor ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid org ID".to_string()))?,
			slug: row.slug,
			name: row.name,
			description: row.description,
			status: row
				.monitor_status
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid status".to_string()))?,
			health: row
				.health
				.parse()
				.map_err(|_| CronsServerError::Internal("Invalid health".to_string()))?,
			schedule,
			timezone: row.timezone,
			checkin_margin_minutes: row.checkin_margin_minutes as u32,
			max_runtime_minutes: row.max_runtime_minutes.map(|m| m as u32),
			ping_key: row.ping_key,
			environments,
			last_checkin_at: row
				.last_checkin_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid last_checkin_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			last_checkin_status: row
				.last_checkin_status
				.map(|s| {
					s.parse()
						.map_err(|_| CronsServerError::Internal("Invalid last_checkin_status".to_string()))
				})
				.transpose()?,
			next_expected_at: row
				.next_expected_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| CronsServerError::Internal("Invalid next_expected_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			consecutive_failures: row.consecutive_failures as u32,
			total_checkins: row.total_checkins as u64,
			total_failures: row.total_failures as u64,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.monitor_created_at)
				.map_err(|_| CronsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.monitor_updated_at)
				.map_err(|_| CronsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
		};

		Ok((checkin, monitor))
	}
}
