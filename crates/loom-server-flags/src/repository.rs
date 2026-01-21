// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::instrument;

use loom_flags_core::{
	Environment, EnvironmentId, ExposureLog, ExposureLogId, Flag, FlagConfig, FlagId,
	FlagPrerequisite, KillSwitch, KillSwitchId, OrgId, SdkKey, SdkKeyId, SdkKeyType, Strategy,
	StrategyId, Variant,
};

use crate::error::{FlagsServerError, Result};

/// Repository trait for feature flags operations.
#[async_trait]
pub trait FlagsRepository: Send + Sync {
	// Environment operations
	async fn create_environment(&self, env: &Environment) -> Result<()>;
	async fn get_environment_by_id(&self, id: EnvironmentId) -> Result<Option<Environment>>;
	async fn get_environment_by_name(&self, org_id: OrgId, name: &str)
		-> Result<Option<Environment>>;
	async fn list_environments(&self, org_id: OrgId) -> Result<Vec<Environment>>;
	async fn update_environment(&self, env: &Environment) -> Result<()>;
	async fn delete_environment(&self, id: EnvironmentId) -> Result<bool>;

	// Flag operations
	async fn create_flag(&self, flag: &Flag) -> Result<()>;
	async fn get_flag_by_id(&self, id: FlagId) -> Result<Option<Flag>>;
	async fn get_flag_by_key(&self, org_id: Option<OrgId>, key: &str) -> Result<Option<Flag>>;
	async fn list_flags(&self, org_id: Option<OrgId>, include_archived: bool) -> Result<Vec<Flag>>;
	async fn update_flag(&self, flag: &Flag) -> Result<()>;
	async fn archive_flag(&self, id: FlagId) -> Result<bool>;
	async fn restore_flag(&self, id: FlagId) -> Result<bool>;

	// Flag config operations
	async fn create_flag_config(&self, config: &FlagConfig) -> Result<()>;
	async fn get_flag_config(
		&self,
		flag_id: FlagId,
		environment_id: EnvironmentId,
	) -> Result<Option<FlagConfig>>;
	async fn list_flag_configs(&self, flag_id: FlagId) -> Result<Vec<FlagConfig>>;
	async fn update_flag_config(&self, config: &FlagConfig) -> Result<()>;

	// Strategy operations
	async fn create_strategy(&self, strategy: &Strategy) -> Result<()>;
	async fn get_strategy_by_id(&self, id: StrategyId) -> Result<Option<Strategy>>;
	async fn list_strategies(&self, org_id: Option<OrgId>) -> Result<Vec<Strategy>>;
	async fn update_strategy(&self, strategy: &Strategy) -> Result<()>;
	async fn delete_strategy(&self, id: StrategyId) -> Result<bool>;

	// Kill switch operations
	async fn create_kill_switch(&self, kill_switch: &KillSwitch) -> Result<()>;
	async fn get_kill_switch_by_id(&self, id: KillSwitchId) -> Result<Option<KillSwitch>>;
	async fn get_kill_switch_by_key(
		&self,
		org_id: Option<OrgId>,
		key: &str,
	) -> Result<Option<KillSwitch>>;
	async fn list_kill_switches(&self, org_id: Option<OrgId>) -> Result<Vec<KillSwitch>>;
	async fn list_active_kill_switches(&self, org_id: Option<OrgId>) -> Result<Vec<KillSwitch>>;
	async fn update_kill_switch(&self, kill_switch: &KillSwitch) -> Result<()>;
	async fn delete_kill_switch(&self, id: KillSwitchId) -> Result<bool>;

	// SDK key operations
	async fn create_sdk_key(&self, key: &SdkKey) -> Result<()>;
	async fn get_sdk_key_by_id(&self, id: SdkKeyId) -> Result<Option<SdkKey>>;
	async fn get_sdk_key_by_hash(&self, key_hash: &str) -> Result<Option<SdkKey>>;
	async fn list_sdk_keys(&self, environment_id: EnvironmentId) -> Result<Vec<SdkKey>>;
	async fn revoke_sdk_key(&self, id: SdkKeyId) -> Result<bool>;
	async fn update_sdk_key_last_used(&self, id: SdkKeyId) -> Result<()>;

	/// Find an SDK key by verifying the raw key against stored hashes.
	///
	/// This method iterates through all SDK keys with the given environment name
	/// and verifies the raw key against each stored hash using Argon2.
	/// This is O(n) but acceptable for connection establishment.
	///
	/// Returns the matching SDK key and its environment if found.
	async fn find_sdk_key_by_verification(
		&self,
		raw_key: &str,
		env_name: &str,
	) -> Result<Option<(SdkKey, Environment)>>;

	// Exposure log operations

	/// Creates a new exposure log entry.
	async fn create_exposure_log(&self, log: &ExposureLog) -> Result<()>;

	/// Checks if an exposure already exists for the given context hash within the deduplication window.
	///
	/// Returns true if an exposure with the same context hash exists within the last hour.
	async fn exposure_exists_within_window(
		&self,
		flag_key: &str,
		context_hash: &str,
		window_hours: u32,
	) -> Result<bool>;

	/// Lists exposure logs with optional filtering.
	async fn list_exposure_logs(
		&self,
		flag_key: Option<&str>,
		environment_id: Option<EnvironmentId>,
		start_time: Option<chrono::DateTime<Utc>>,
		end_time: Option<chrono::DateTime<Utc>>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ExposureLog>>;

	/// Gets the count of exposure logs matching the filter criteria.
	async fn count_exposure_logs(
		&self,
		flag_key: Option<&str>,
		environment_id: Option<EnvironmentId>,
		start_time: Option<chrono::DateTime<Utc>>,
		end_time: Option<chrono::DateTime<Utc>>,
	) -> Result<u64>;

	// Flag stats operations

	/// Gets statistics for a specific flag.
	async fn get_flag_stats(&self, flag_id: FlagId) -> Result<Option<loom_flags_core::FlagStats>>;

	/// Records an evaluation for a flag, updating its statistics.
	///
	/// This method updates:
	/// - `last_evaluated_at` to the current timestamp
	/// - Increments evaluation counters based on time windows
	async fn record_flag_evaluation(&self, flag_id: FlagId, flag_key: &str) -> Result<()>;

	/// Lists flags that are considered stale (not evaluated within the threshold).
	///
	/// A flag is stale if:
	/// - It has never been evaluated, OR
	/// - It was last evaluated more than `stale_threshold_days` ago
	async fn list_stale_flags(
		&self,
		org_id: Option<OrgId>,
		stale_threshold_days: u32,
	) -> Result<Vec<(Flag, Option<chrono::DateTime<Utc>>)>>;
}

/// SQLite implementation of the flags repository.
#[derive(Clone)]
pub struct SqliteFlagsRepository {
	pool: SqlitePool,
}

impl SqliteFlagsRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}
}

#[async_trait]
impl FlagsRepository for SqliteFlagsRepository {
	// Environment operations

	#[instrument(skip(self, env), fields(env_id = %env.id, org_id = %env.org_id))]
	async fn create_environment(&self, env: &Environment) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO flag_environments (id, org_id, name, color, created_at)
			VALUES (?, ?, ?, ?, ?)
			"#,
		)
		.bind(env.id.0.to_string())
		.bind(env.org_id.0.to_string())
		.bind(&env.name)
		.bind(&env.color)
		.bind(env.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(env_id = %id))]
	async fn get_environment_by_id(&self, id: EnvironmentId) -> Result<Option<Environment>> {
		let row = sqlx::query_as::<_, EnvironmentRow>(
			r#"
			SELECT id, org_id, name, color, created_at
			FROM flag_environments
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id, name = %name))]
	async fn get_environment_by_name(
		&self,
		org_id: OrgId,
		name: &str,
	) -> Result<Option<Environment>> {
		let row = sqlx::query_as::<_, EnvironmentRow>(
			r#"
			SELECT id, org_id, name, color, created_at
			FROM flag_environments
			WHERE org_id = ? AND name = ?
			"#,
		)
		.bind(org_id.0.to_string())
		.bind(name)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id))]
	async fn list_environments(&self, org_id: OrgId) -> Result<Vec<Environment>> {
		let rows = sqlx::query_as::<_, EnvironmentRow>(
			r#"
			SELECT id, org_id, name, color, created_at
			FROM flag_environments
			WHERE org_id = ?
			ORDER BY created_at ASC
			"#,
		)
		.bind(org_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, env), fields(env_id = %env.id))]
	async fn update_environment(&self, env: &Environment) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE flag_environments
			SET name = ?, color = ?
			WHERE id = ?
			"#,
		)
		.bind(&env.name)
		.bind(&env.color)
		.bind(env.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(env_id = %id))]
	async fn delete_environment(&self, id: EnvironmentId) -> Result<bool> {
		let result = sqlx::query(
			r#"
			DELETE FROM flag_environments WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	// Flag operations

	#[instrument(skip(self, flag), fields(flag_id = %flag.id, flag_key = %flag.key))]
	async fn create_flag(&self, flag: &Flag) -> Result<()> {
		let variants_json = serde_json::to_string(&flag.variants)?;
		let tags_json = serde_json::to_string(&flag.tags)?;

		sqlx::query(
			r#"
			INSERT INTO flags (id, org_id, key, name, description, tags, maintainer_user_id,
							   variants, default_variant, exposure_tracking_enabled,
							   created_at, updated_at, archived_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(flag.id.0.to_string())
		.bind(flag.org_id.map(|id| id.0.to_string()))
		.bind(&flag.key)
		.bind(&flag.name)
		.bind(&flag.description)
		.bind(tags_json)
		.bind(flag.maintainer_user_id.map(|id| id.0.to_string()))
		.bind(variants_json)
		.bind(&flag.default_variant)
		.bind(flag.exposure_tracking_enabled)
		.bind(flag.created_at.to_rfc3339())
		.bind(flag.updated_at.to_rfc3339())
		.bind(flag.archived_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		// Insert prerequisites
		for prereq in &flag.prerequisites {
			sqlx::query(
				r#"
				INSERT INTO flag_prerequisites (id, flag_id, prerequisite_flag_key, required_variant, created_at)
				VALUES (?, ?, ?, ?, ?)
				"#,
			)
			.bind(uuid::Uuid::new_v4().to_string())
			.bind(flag.id.0.to_string())
			.bind(&prereq.flag_key)
			.bind(&prereq.required_variant)
			.bind(Utc::now().to_rfc3339())
			.execute(&self.pool)
			.await?;
		}

		Ok(())
	}

	#[instrument(skip(self), fields(flag_id = %id))]
	async fn get_flag_by_id(&self, id: FlagId) -> Result<Option<Flag>> {
		let row = sqlx::query_as::<_, FlagRow>(
			r#"
			SELECT id, org_id, key, name, description, tags, maintainer_user_id,
				   variants, default_variant, exposure_tracking_enabled,
				   created_at, updated_at, archived_at
			FROM flags
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let prerequisites = self.get_flag_prerequisites(id).await?;
				Ok(Some(row.into_flag(prerequisites)?))
			}
			None => Ok(None),
		}
	}

	#[instrument(skip(self), fields(org_id = ?org_id, flag_key = %key))]
	async fn get_flag_by_key(&self, org_id: Option<OrgId>, key: &str) -> Result<Option<Flag>> {
		let row = match org_id {
			Some(org) => {
				sqlx::query_as::<_, FlagRow>(
					r#"
					SELECT id, org_id, key, name, description, tags, maintainer_user_id,
						   variants, default_variant, exposure_tracking_enabled,
						   created_at, updated_at, archived_at
					FROM flags
					WHERE org_id = ? AND key = ?
					"#,
				)
				.bind(org.0.to_string())
				.bind(key)
				.fetch_optional(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, FlagRow>(
					r#"
					SELECT id, org_id, key, name, description, tags, maintainer_user_id,
						   variants, default_variant, exposure_tracking_enabled,
						   created_at, updated_at, archived_at
					FROM flags
					WHERE org_id IS NULL AND key = ?
					"#,
				)
				.bind(key)
				.fetch_optional(&self.pool)
				.await?
			}
		};

		match row {
			Some(row) => {
				let flag_id: FlagId = row
					.id
					.parse()
					.map_err(|_| FlagsServerError::Internal("Invalid flag ID in database".to_string()))?;
				let prerequisites = self.get_flag_prerequisites(flag_id).await?;
				Ok(Some(row.into_flag(prerequisites)?))
			}
			None => Ok(None),
		}
	}

	#[instrument(skip(self), fields(org_id = ?org_id))]
	async fn list_flags(&self, org_id: Option<OrgId>, include_archived: bool) -> Result<Vec<Flag>> {
		let rows = match org_id {
			Some(org) => {
				if include_archived {
					sqlx::query_as::<_, FlagRow>(
						r#"
						SELECT id, org_id, key, name, description, tags, maintainer_user_id,
							   variants, default_variant, exposure_tracking_enabled,
							   created_at, updated_at, archived_at
						FROM flags
						WHERE org_id = ?
						ORDER BY key ASC
						"#,
					)
					.bind(org.0.to_string())
					.fetch_all(&self.pool)
					.await?
				} else {
					sqlx::query_as::<_, FlagRow>(
						r#"
						SELECT id, org_id, key, name, description, tags, maintainer_user_id,
							   variants, default_variant, exposure_tracking_enabled,
							   created_at, updated_at, archived_at
						FROM flags
						WHERE org_id = ? AND archived_at IS NULL
						ORDER BY key ASC
						"#,
					)
					.bind(org.0.to_string())
					.fetch_all(&self.pool)
					.await?
				}
			}
			None => {
				if include_archived {
					sqlx::query_as::<_, FlagRow>(
						r#"
						SELECT id, org_id, key, name, description, tags, maintainer_user_id,
							   variants, default_variant, exposure_tracking_enabled,
							   created_at, updated_at, archived_at
						FROM flags
						WHERE org_id IS NULL
						ORDER BY key ASC
						"#,
					)
					.fetch_all(&self.pool)
					.await?
				} else {
					sqlx::query_as::<_, FlagRow>(
						r#"
						SELECT id, org_id, key, name, description, tags, maintainer_user_id,
							   variants, default_variant, exposure_tracking_enabled,
							   created_at, updated_at, archived_at
						FROM flags
						WHERE org_id IS NULL AND archived_at IS NULL
						ORDER BY key ASC
						"#,
					)
					.fetch_all(&self.pool)
					.await?
				}
			}
		};

		let mut flags = Vec::with_capacity(rows.len());
		for row in rows {
			let flag_id: FlagId = row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID in database".to_string()))?;
			let prerequisites = self.get_flag_prerequisites(flag_id).await?;
			flags.push(row.into_flag(prerequisites)?);
		}

		Ok(flags)
	}

	#[instrument(skip(self, flag), fields(flag_id = %flag.id))]
	async fn update_flag(&self, flag: &Flag) -> Result<()> {
		let variants_json = serde_json::to_string(&flag.variants)?;
		let tags_json = serde_json::to_string(&flag.tags)?;

		sqlx::query(
			r#"
			UPDATE flags
			SET name = ?, description = ?, tags = ?, maintainer_user_id = ?,
				variants = ?, default_variant = ?, exposure_tracking_enabled = ?,
				updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&flag.name)
		.bind(&flag.description)
		.bind(tags_json)
		.bind(flag.maintainer_user_id.map(|id| id.0.to_string()))
		.bind(variants_json)
		.bind(&flag.default_variant)
		.bind(flag.exposure_tracking_enabled)
		.bind(Utc::now().to_rfc3339())
		.bind(flag.id.0.to_string())
		.execute(&self.pool)
		.await?;

		// Update prerequisites - delete and re-insert
		sqlx::query("DELETE FROM flag_prerequisites WHERE flag_id = ?")
			.bind(flag.id.0.to_string())
			.execute(&self.pool)
			.await?;

		for prereq in &flag.prerequisites {
			sqlx::query(
				r#"
				INSERT INTO flag_prerequisites (id, flag_id, prerequisite_flag_key, required_variant, created_at)
				VALUES (?, ?, ?, ?, ?)
				"#,
			)
			.bind(uuid::Uuid::new_v4().to_string())
			.bind(flag.id.0.to_string())
			.bind(&prereq.flag_key)
			.bind(&prereq.required_variant)
			.bind(Utc::now().to_rfc3339())
			.execute(&self.pool)
			.await?;
		}

		Ok(())
	}

	#[instrument(skip(self), fields(flag_id = %id))]
	async fn archive_flag(&self, id: FlagId) -> Result<bool> {
		let result = sqlx::query(
			r#"
			UPDATE flags
			SET archived_at = ?, updated_at = ?
			WHERE id = ? AND archived_at IS NULL
			"#,
		)
		.bind(Utc::now().to_rfc3339())
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	#[instrument(skip(self), fields(flag_id = %id))]
	async fn restore_flag(&self, id: FlagId) -> Result<bool> {
		let result = sqlx::query(
			r#"
			UPDATE flags
			SET archived_at = NULL, updated_at = ?
			WHERE id = ? AND archived_at IS NOT NULL
			"#,
		)
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	// Flag config operations

	#[instrument(skip(self, config), fields(config_id = %config.id, flag_id = %config.flag_id))]
	async fn create_flag_config(&self, config: &FlagConfig) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO flag_configs (id, flag_id, environment_id, enabled, strategy_id, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(config.id.0.to_string())
		.bind(config.flag_id.0.to_string())
		.bind(config.environment_id.0.to_string())
		.bind(config.enabled)
		.bind(config.strategy_id.map(|id| id.0.to_string()))
		.bind(config.created_at.to_rfc3339())
		.bind(config.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(flag_id = %flag_id, env_id = %environment_id))]
	async fn get_flag_config(
		&self,
		flag_id: FlagId,
		environment_id: EnvironmentId,
	) -> Result<Option<FlagConfig>> {
		let row = sqlx::query_as::<_, FlagConfigRow>(
			r#"
			SELECT id, flag_id, environment_id, enabled, strategy_id, created_at, updated_at
			FROM flag_configs
			WHERE flag_id = ? AND environment_id = ?
			"#,
		)
		.bind(flag_id.0.to_string())
		.bind(environment_id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(flag_id = %flag_id))]
	async fn list_flag_configs(&self, flag_id: FlagId) -> Result<Vec<FlagConfig>> {
		let rows = sqlx::query_as::<_, FlagConfigRow>(
			r#"
			SELECT id, flag_id, environment_id, enabled, strategy_id, created_at, updated_at
			FROM flag_configs
			WHERE flag_id = ?
			"#,
		)
		.bind(flag_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, config), fields(config_id = %config.id))]
	async fn update_flag_config(&self, config: &FlagConfig) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE flag_configs
			SET enabled = ?, strategy_id = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(config.enabled)
		.bind(config.strategy_id.map(|id| id.0.to_string()))
		.bind(Utc::now().to_rfc3339())
		.bind(config.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	// Strategy operations

	#[instrument(skip(self, strategy), fields(strategy_id = %strategy.id))]
	async fn create_strategy(&self, strategy: &Strategy) -> Result<()> {
		let conditions_json = serde_json::to_string(&strategy.conditions)?;
		let schedule_json = strategy
			.schedule
			.as_ref()
			.map(serde_json::to_string)
			.transpose()?;
		let percentage_key_json = serde_json::to_string(&strategy.percentage_key)?;

		sqlx::query(
			r#"
			INSERT INTO flag_strategies (id, org_id, name, description, conditions, percentage,
										 percentage_key, schedule, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(strategy.id.0.to_string())
		.bind(strategy.org_id.map(|id| id.0.to_string()))
		.bind(&strategy.name)
		.bind(&strategy.description)
		.bind(conditions_json)
		.bind(strategy.percentage.map(|p| p as i32))
		.bind(percentage_key_json)
		.bind(schedule_json)
		.bind(strategy.created_at.to_rfc3339())
		.bind(strategy.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(strategy_id = %id))]
	async fn get_strategy_by_id(&self, id: StrategyId) -> Result<Option<Strategy>> {
		let row = sqlx::query_as::<_, StrategyRow>(
			r#"
			SELECT id, org_id, name, description, conditions, percentage, percentage_key,
				   schedule, created_at, updated_at
			FROM flag_strategies
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = ?org_id))]
	async fn list_strategies(&self, org_id: Option<OrgId>) -> Result<Vec<Strategy>> {
		let rows = match org_id {
			Some(org) => {
				sqlx::query_as::<_, StrategyRow>(
					r#"
					SELECT id, org_id, name, description, conditions, percentage, percentage_key,
						   schedule, created_at, updated_at
					FROM flag_strategies
					WHERE org_id = ?
					ORDER BY name ASC
					"#,
				)
				.bind(org.0.to_string())
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, StrategyRow>(
					r#"
					SELECT id, org_id, name, description, conditions, percentage, percentage_key,
						   schedule, created_at, updated_at
					FROM flag_strategies
					WHERE org_id IS NULL
					ORDER BY name ASC
					"#,
				)
				.fetch_all(&self.pool)
				.await?
			}
		};

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, strategy), fields(strategy_id = %strategy.id))]
	async fn update_strategy(&self, strategy: &Strategy) -> Result<()> {
		let conditions_json = serde_json::to_string(&strategy.conditions)?;
		let schedule_json = strategy
			.schedule
			.as_ref()
			.map(serde_json::to_string)
			.transpose()?;
		let percentage_key_json = serde_json::to_string(&strategy.percentage_key)?;

		sqlx::query(
			r#"
			UPDATE flag_strategies
			SET name = ?, description = ?, conditions = ?, percentage = ?,
				percentage_key = ?, schedule = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&strategy.name)
		.bind(&strategy.description)
		.bind(conditions_json)
		.bind(strategy.percentage.map(|p| p as i32))
		.bind(percentage_key_json)
		.bind(schedule_json)
		.bind(Utc::now().to_rfc3339())
		.bind(strategy.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(strategy_id = %id))]
	async fn delete_strategy(&self, id: StrategyId) -> Result<bool> {
		let result = sqlx::query("DELETE FROM flag_strategies WHERE id = ?")
			.bind(id.0.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected() > 0)
	}

	// Kill switch operations

	#[instrument(skip(self, kill_switch), fields(kill_switch_id = %kill_switch.id, key = %kill_switch.key))]
	async fn create_kill_switch(&self, kill_switch: &KillSwitch) -> Result<()> {
		let linked_flags_json = serde_json::to_string(&kill_switch.linked_flag_keys)?;

		sqlx::query(
			r#"
			INSERT INTO kill_switches (id, org_id, key, name, description, linked_flag_keys,
									   is_active, activated_at, activated_by, activation_reason,
									   created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(kill_switch.id.0.to_string())
		.bind(kill_switch.org_id.map(|id| id.0.to_string()))
		.bind(&kill_switch.key)
		.bind(&kill_switch.name)
		.bind(&kill_switch.description)
		.bind(linked_flags_json)
		.bind(kill_switch.is_active)
		.bind(kill_switch.activated_at.map(|dt| dt.to_rfc3339()))
		.bind(kill_switch.activated_by.map(|id| id.0.to_string()))
		.bind(&kill_switch.activation_reason)
		.bind(kill_switch.created_at.to_rfc3339())
		.bind(kill_switch.updated_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(kill_switch_id = %id))]
	async fn get_kill_switch_by_id(&self, id: KillSwitchId) -> Result<Option<KillSwitch>> {
		let row = sqlx::query_as::<_, KillSwitchRow>(
			r#"
			SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
				   activated_at, activated_by, activation_reason, created_at, updated_at
			FROM kill_switches
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = ?org_id, key = %key))]
	async fn get_kill_switch_by_key(
		&self,
		org_id: Option<OrgId>,
		key: &str,
	) -> Result<Option<KillSwitch>> {
		let row = match org_id {
			Some(org) => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id = ? AND key = ?
					"#,
				)
				.bind(org.0.to_string())
				.bind(key)
				.fetch_optional(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id IS NULL AND key = ?
					"#,
				)
				.bind(key)
				.fetch_optional(&self.pool)
				.await?
			}
		};

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = ?org_id))]
	async fn list_kill_switches(&self, org_id: Option<OrgId>) -> Result<Vec<KillSwitch>> {
		let rows = match org_id {
			Some(org) => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id = ?
					ORDER BY key ASC
					"#,
				)
				.bind(org.0.to_string())
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id IS NULL
					ORDER BY key ASC
					"#,
				)
				.fetch_all(&self.pool)
				.await?
			}
		};

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self), fields(org_id = ?org_id))]
	async fn list_active_kill_switches(&self, org_id: Option<OrgId>) -> Result<Vec<KillSwitch>> {
		let rows = match org_id {
			Some(org) => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id = ? AND is_active = 1
					ORDER BY key ASC
					"#,
				)
				.bind(org.0.to_string())
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, KillSwitchRow>(
					r#"
					SELECT id, org_id, key, name, description, linked_flag_keys, is_active,
						   activated_at, activated_by, activation_reason, created_at, updated_at
					FROM kill_switches
					WHERE org_id IS NULL AND is_active = 1
					ORDER BY key ASC
					"#,
				)
				.fetch_all(&self.pool)
				.await?
			}
		};

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, kill_switch), fields(kill_switch_id = %kill_switch.id))]
	async fn update_kill_switch(&self, kill_switch: &KillSwitch) -> Result<()> {
		let linked_flags_json = serde_json::to_string(&kill_switch.linked_flag_keys)?;

		sqlx::query(
			r#"
			UPDATE kill_switches
			SET name = ?, description = ?, linked_flag_keys = ?, is_active = ?,
				activated_at = ?, activated_by = ?, activation_reason = ?, updated_at = ?
			WHERE id = ?
			"#,
		)
		.bind(&kill_switch.name)
		.bind(&kill_switch.description)
		.bind(linked_flags_json)
		.bind(kill_switch.is_active)
		.bind(kill_switch.activated_at.map(|dt| dt.to_rfc3339()))
		.bind(kill_switch.activated_by.map(|id| id.0.to_string()))
		.bind(&kill_switch.activation_reason)
		.bind(Utc::now().to_rfc3339())
		.bind(kill_switch.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(kill_switch_id = %id))]
	async fn delete_kill_switch(&self, id: KillSwitchId) -> Result<bool> {
		let result = sqlx::query("DELETE FROM kill_switches WHERE id = ?")
			.bind(id.0.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected() > 0)
	}

	// SDK key operations

	#[instrument(skip(self, key), fields(sdk_key_id = %key.id))]
	async fn create_sdk_key(&self, key: &SdkKey) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO sdk_keys (id, environment_id, key_type, name, key_hash, created_by,
								  created_at, last_used_at, revoked_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(key.id.0.to_string())
		.bind(key.environment_id.0.to_string())
		.bind(key.key_type.as_str())
		.bind(&key.name)
		.bind(&key.key_hash)
		.bind(key.created_by.0.to_string())
		.bind(key.created_at.to_rfc3339())
		.bind(key.last_used_at.map(|dt| dt.to_rfc3339()))
		.bind(key.revoked_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(sdk_key_id = %id))]
	async fn get_sdk_key_by_id(&self, id: SdkKeyId) -> Result<Option<SdkKey>> {
		let row = sqlx::query_as::<_, SdkKeyRow>(
			r#"
			SELECT id, environment_id, key_type, name, key_hash, created_by,
				   created_at, last_used_at, revoked_at
			FROM sdk_keys
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self, key_hash))]
	async fn get_sdk_key_by_hash(&self, key_hash: &str) -> Result<Option<SdkKey>> {
		let row = sqlx::query_as::<_, SdkKeyRow>(
			r#"
			SELECT id, environment_id, key_type, name, key_hash, created_by,
				   created_at, last_used_at, revoked_at
			FROM sdk_keys
			WHERE key_hash = ?
			"#,
		)
		.bind(key_hash)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(env_id = %environment_id))]
	async fn list_sdk_keys(&self, environment_id: EnvironmentId) -> Result<Vec<SdkKey>> {
		let rows = sqlx::query_as::<_, SdkKeyRow>(
			r#"
			SELECT id, environment_id, key_type, name, key_hash, created_by,
				   created_at, last_used_at, revoked_at
			FROM sdk_keys
			WHERE environment_id = ?
			ORDER BY created_at DESC
			"#,
		)
		.bind(environment_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self), fields(sdk_key_id = %id))]
	async fn revoke_sdk_key(&self, id: SdkKeyId) -> Result<bool> {
		let result = sqlx::query(
			r#"
			UPDATE sdk_keys
			SET revoked_at = ?
			WHERE id = ? AND revoked_at IS NULL
			"#,
		)
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	#[instrument(skip(self), fields(sdk_key_id = %id))]
	async fn update_sdk_key_last_used(&self, id: SdkKeyId) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE sdk_keys
			SET last_used_at = ?
			WHERE id = ?
			"#,
		)
		.bind(Utc::now().to_rfc3339())
		.bind(id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self, raw_key), fields(env_name = %env_name))]
	async fn find_sdk_key_by_verification(
		&self,
		raw_key: &str,
		env_name: &str,
	) -> Result<Option<(SdkKey, Environment)>> {
		// Find all environments with the given name (across all orgs)
		let envs = sqlx::query_as::<_, EnvironmentRow>(
			r#"
			SELECT id, org_id, name, color, created_at
			FROM flag_environments
			WHERE name = ?
			"#,
		)
		.bind(env_name)
		.fetch_all(&self.pool)
		.await?;

		// For each environment, check its SDK keys
		for env_row in envs {
			let env: Environment = env_row.try_into()?;

			// Get all SDK keys for this environment
			let sdk_keys = self.list_sdk_keys(env.id).await?;

			// Try to verify the raw key against each stored hash
			for sdk_key in sdk_keys {
				// Skip revoked keys
				if sdk_key.revoked_at.is_some() {
					continue;
				}

				// Verify the raw key against the stored hash
				match crate::sdk_auth::verify_sdk_key(raw_key, &sdk_key.key_hash) {
					Ok(true) => {
						tracing::debug!(
							sdk_key_id = %sdk_key.id,
							env_id = %env.id,
							"SDK key verified successfully"
						);
						return Ok(Some((sdk_key, env)));
					}
					Ok(false) => {
						// Key didn't match, try next one
						continue;
					}
					Err(e) => {
						// Log error but continue trying other keys
						tracing::warn!(
							sdk_key_id = %sdk_key.id,
							error = %e,
							"Failed to verify SDK key hash"
						);
						continue;
					}
				}
			}
		}

		// No matching key found
		Ok(None)
	}

	// Exposure log operations

	#[instrument(skip(self, log), fields(exposure_id = %log.id, flag_key = %log.flag_key))]
	async fn create_exposure_log(&self, log: &ExposureLog) -> Result<()> {
		let reason_json = serde_json::to_string(&log.reason)?;

		sqlx::query(
			r#"
			INSERT INTO exposure_logs (id, flag_id, flag_key, environment_id, user_id, org_id,
									   variant, reason, context_hash, timestamp)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(log.id.0.to_string())
		.bind(log.flag_id.0.to_string())
		.bind(&log.flag_key)
		.bind(log.environment_id.0.to_string())
		.bind(&log.user_id)
		.bind(&log.org_id)
		.bind(&log.variant)
		.bind(reason_json)
		.bind(&log.context_hash)
		.bind(log.timestamp.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(flag_key = %flag_key, window_hours = window_hours))]
	async fn exposure_exists_within_window(
		&self,
		flag_key: &str,
		context_hash: &str,
		window_hours: u32,
	) -> Result<bool> {
		let row: (i64,) = sqlx::query_as(
			r#"
			SELECT COUNT(*) as count
			FROM exposure_logs
			WHERE flag_key = ? AND context_hash = ?
			  AND timestamp > datetime('now', ? || ' hours')
			"#,
		)
		.bind(flag_key)
		.bind(context_hash)
		.bind(-(window_hours as i64))
		.fetch_one(&self.pool)
		.await?;

		Ok(row.0 > 0)
	}

	#[instrument(skip(self), fields(flag_key = ?flag_key, env_id = ?environment_id))]
	async fn list_exposure_logs(
		&self,
		flag_key: Option<&str>,
		environment_id: Option<EnvironmentId>,
		start_time: Option<chrono::DateTime<Utc>>,
		end_time: Option<chrono::DateTime<Utc>>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ExposureLog>> {
		let mut query = String::from(
			r#"
			SELECT id, flag_id, flag_key, environment_id, user_id, org_id,
				   variant, reason, context_hash, timestamp
			FROM exposure_logs
			WHERE 1=1
			"#,
		);

		if flag_key.is_some() {
			query.push_str(" AND flag_key = ?");
		}
		if environment_id.is_some() {
			query.push_str(" AND environment_id = ?");
		}
		if start_time.is_some() {
			query.push_str(" AND timestamp >= ?");
		}
		if end_time.is_some() {
			query.push_str(" AND timestamp <= ?");
		}

		query.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

		let mut q = sqlx::query_as::<_, ExposureLogRow>(&query);

		if let Some(key) = flag_key {
			q = q.bind(key);
		}
		if let Some(env_id) = environment_id {
			q = q.bind(env_id.0.to_string());
		}
		if let Some(start) = start_time {
			q = q.bind(start.to_rfc3339());
		}
		if let Some(end) = end_time {
			q = q.bind(end.to_rfc3339());
		}

		q = q.bind(limit as i64);
		q = q.bind(offset as i64);

		let rows = q.fetch_all(&self.pool).await?;
		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self), fields(flag_key = ?flag_key, env_id = ?environment_id))]
	async fn count_exposure_logs(
		&self,
		flag_key: Option<&str>,
		environment_id: Option<EnvironmentId>,
		start_time: Option<chrono::DateTime<Utc>>,
		end_time: Option<chrono::DateTime<Utc>>,
	) -> Result<u64> {
		let mut query = String::from(
			r#"
			SELECT COUNT(*) as count
			FROM exposure_logs
			WHERE 1=1
			"#,
		);

		if flag_key.is_some() {
			query.push_str(" AND flag_key = ?");
		}
		if environment_id.is_some() {
			query.push_str(" AND environment_id = ?");
		}
		if start_time.is_some() {
			query.push_str(" AND timestamp >= ?");
		}
		if end_time.is_some() {
			query.push_str(" AND timestamp <= ?");
		}

		let mut q = sqlx::query_as::<_, (i64,)>(&query);

		if let Some(key) = flag_key {
			q = q.bind(key);
		}
		if let Some(env_id) = environment_id {
			q = q.bind(env_id.0.to_string());
		}
		if let Some(start) = start_time {
			q = q.bind(start.to_rfc3339());
		}
		if let Some(end) = end_time {
			q = q.bind(end.to_rfc3339());
		}

		let (count,) = q.fetch_one(&self.pool).await?;
		Ok(count as u64)
	}

	// Flag stats operations

	#[instrument(skip(self), fields(flag_id = %flag_id))]
	async fn get_flag_stats(&self, flag_id: FlagId) -> Result<Option<loom_flags_core::FlagStats>> {
		let row = sqlx::query_as::<_, FlagStatsRow>(
			r#"
			SELECT fs.flag_id, f.key as flag_key, fs.last_evaluated_at,
				   fs.evaluation_count_24h, fs.evaluation_count_7d, fs.evaluation_count_30d
			FROM flag_stats fs
			JOIN flags f ON f.id = fs.flag_id
			WHERE fs.flag_id = ?
			"#,
		)
		.bind(flag_id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(flag_id = %flag_id, flag_key = %flag_key))]
	async fn record_flag_evaluation(&self, flag_id: FlagId, flag_key: &str) -> Result<()> {
		let now = Utc::now();

		// Use upsert to insert or update stats
		sqlx::query(
			r#"
			INSERT INTO flag_stats (flag_id, last_evaluated_at, evaluation_count_24h, evaluation_count_7d, evaluation_count_30d, updated_at)
			VALUES (?, ?, 1, 1, 1, ?)
			ON CONFLICT(flag_id) DO UPDATE SET
				last_evaluated_at = excluded.last_evaluated_at,
				evaluation_count_24h = flag_stats.evaluation_count_24h + 1,
				evaluation_count_7d = flag_stats.evaluation_count_7d + 1,
				evaluation_count_30d = flag_stats.evaluation_count_30d + 1,
				updated_at = excluded.updated_at
			"#,
		)
		.bind(flag_id.0.to_string())
		.bind(now.to_rfc3339())
		.bind(now.to_rfc3339())
		.execute(&self.pool)
		.await?;

		tracing::debug!(%flag_id, %flag_key, "Recorded flag evaluation");
		Ok(())
	}

	#[instrument(skip(self), fields(org_id = ?org_id, stale_threshold_days = stale_threshold_days))]
	async fn list_stale_flags(
		&self,
		org_id: Option<OrgId>,
		stale_threshold_days: u32,
	) -> Result<Vec<(Flag, Option<chrono::DateTime<Utc>>)>> {
		// Calculate the threshold timestamp
		let threshold = Utc::now() - chrono::Duration::days(stale_threshold_days as i64);
		let threshold_str = threshold.to_rfc3339();

		let rows = match org_id {
			Some(org) => {
				sqlx::query_as::<_, FlagWithStatsRow>(
					r#"
					SELECT f.id, f.org_id, f.key, f.name, f.description, f.tags, f.maintainer_user_id,
						   f.variants, f.default_variant, f.exposure_tracking_enabled,
						   f.created_at, f.updated_at, f.archived_at,
						   fs.last_evaluated_at
					FROM flags f
					LEFT JOIN flag_stats fs ON f.id = fs.flag_id
					WHERE f.org_id = ?
					  AND f.archived_at IS NULL
					  AND (fs.last_evaluated_at IS NULL OR fs.last_evaluated_at < ?)
					ORDER BY fs.last_evaluated_at ASC NULLS FIRST, f.key ASC
					"#,
				)
				.bind(org.0.to_string())
				.bind(&threshold_str)
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as::<_, FlagWithStatsRow>(
					r#"
					SELECT f.id, f.org_id, f.key, f.name, f.description, f.tags, f.maintainer_user_id,
						   f.variants, f.default_variant, f.exposure_tracking_enabled,
						   f.created_at, f.updated_at, f.archived_at,
						   fs.last_evaluated_at
					FROM flags f
					LEFT JOIN flag_stats fs ON f.id = fs.flag_id
					WHERE f.org_id IS NULL
					  AND f.archived_at IS NULL
					  AND (fs.last_evaluated_at IS NULL OR fs.last_evaluated_at < ?)
					ORDER BY fs.last_evaluated_at ASC NULLS FIRST, f.key ASC
					"#,
				)
				.bind(&threshold_str)
				.fetch_all(&self.pool)
				.await?
			}
		};

		let mut results = Vec::with_capacity(rows.len());
		for row in rows {
			let flag_id: FlagId = row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID in database".to_string()))?;
			let prerequisites = self.get_flag_prerequisites(flag_id).await?;
			// Extract last_evaluated_at before consuming row
			let last_evaluated_at = row.last_evaluated_at.as_ref().and_then(|s| {
				chrono::DateTime::parse_from_rfc3339(s)
					.map(|dt| dt.with_timezone(&chrono::Utc))
					.ok()
			});
			let flag = row.into_flag(prerequisites)?;
			results.push((flag, last_evaluated_at));
		}

		Ok(results)
	}
}

impl SqliteFlagsRepository {
	async fn get_flag_prerequisites(&self, flag_id: FlagId) -> Result<Vec<FlagPrerequisite>> {
		let rows = sqlx::query_as::<_, PrerequisiteRow>(
			r#"
			SELECT prerequisite_flag_key, required_variant
			FROM flag_prerequisites
			WHERE flag_id = ?
			"#,
		)
		.bind(flag_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		Ok(
			rows
				.into_iter()
				.map(|r| FlagPrerequisite {
					flag_key: r.prerequisite_flag_key,
					required_variant: r.required_variant,
				})
				.collect(),
		)
	}
}

// Database row types for sqlx

#[derive(sqlx::FromRow)]
struct EnvironmentRow {
	id: String,
	org_id: String,
	name: String,
	color: Option<String>,
	created_at: String,
}

impl TryFrom<EnvironmentRow> for Environment {
	type Error = FlagsServerError;

	fn try_from(row: EnvironmentRow) -> Result<Self> {
		Ok(Environment {
			id: row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid environment ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid org ID".to_string()))?,
			name: row.name,
			color: row.color,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct FlagRow {
	id: String,
	org_id: Option<String>,
	key: String,
	name: String,
	description: Option<String>,
	tags: String,
	maintainer_user_id: Option<String>,
	variants: String,
	default_variant: String,
	exposure_tracking_enabled: bool,
	created_at: String,
	updated_at: String,
	archived_at: Option<String>,
}

impl FlagRow {
	fn into_flag(self, prerequisites: Vec<FlagPrerequisite>) -> Result<Flag> {
		let tags: Vec<String> = serde_json::from_str(&self.tags)?;
		let variants: Vec<Variant> = serde_json::from_str(&self.variants)?;

		Ok(Flag {
			id: self
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID".to_string()))?,
			org_id: self
				.org_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid org ID".to_string()))
				})
				.transpose()?,
			key: self.key,
			name: self.name,
			description: self.description,
			tags,
			maintainer_user_id: self
				.maintainer_user_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid user ID".to_string()))
				})
				.transpose()?,
			variants,
			default_variant: self.default_variant,
			prerequisites,
			exposure_tracking_enabled: self.exposure_tracking_enabled,
			created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
				.map_err(|_| FlagsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
			archived_at: self
				.archived_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid archived_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
		})
	}
}

#[derive(sqlx::FromRow)]
struct PrerequisiteRow {
	prerequisite_flag_key: String,
	required_variant: String,
}

#[derive(sqlx::FromRow)]
struct FlagConfigRow {
	id: String,
	flag_id: String,
	environment_id: String,
	enabled: bool,
	strategy_id: Option<String>,
	created_at: String,
	updated_at: String,
}

impl TryFrom<FlagConfigRow> for FlagConfig {
	type Error = FlagsServerError;

	fn try_from(row: FlagConfigRow) -> Result<Self> {
		Ok(FlagConfig {
			id: row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid config ID".to_string()))?,
			flag_id: row
				.flag_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID".to_string()))?,
			environment_id: row
				.environment_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid environment ID".to_string()))?,
			enabled: row.enabled,
			strategy_id: row
				.strategy_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid strategy ID".to_string()))
				})
				.transpose()?,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
				.map_err(|_| FlagsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct StrategyRow {
	id: String,
	org_id: Option<String>,
	name: String,
	description: Option<String>,
	conditions: String,
	percentage: Option<i32>,
	percentage_key: String,
	schedule: Option<String>,
	created_at: String,
	updated_at: String,
}

impl TryFrom<StrategyRow> for Strategy {
	type Error = FlagsServerError;

	fn try_from(row: StrategyRow) -> Result<Self> {
		use loom_flags_core::{Condition, PercentageKey, Schedule};

		let conditions: Vec<Condition> = serde_json::from_str(&row.conditions)?;
		let percentage_key: PercentageKey = serde_json::from_str(&row.percentage_key)?;
		let schedule: Option<Schedule> = row.schedule.map(|s| serde_json::from_str(&s)).transpose()?;

		Ok(Strategy {
			id: row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid strategy ID".to_string()))?,
			org_id: row
				.org_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid org ID".to_string()))
				})
				.transpose()?,
			name: row.name,
			description: row.description,
			conditions,
			percentage: row.percentage.map(|p| p as u32),
			percentage_key,
			schedule,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
				.map_err(|_| FlagsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct KillSwitchRow {
	id: String,
	org_id: Option<String>,
	key: String,
	name: String,
	description: Option<String>,
	linked_flag_keys: String,
	is_active: bool,
	activated_at: Option<String>,
	activated_by: Option<String>,
	activation_reason: Option<String>,
	created_at: String,
	updated_at: String,
}

impl TryFrom<KillSwitchRow> for KillSwitch {
	type Error = FlagsServerError;

	fn try_from(row: KillSwitchRow) -> Result<Self> {
		let linked_flag_keys: Vec<String> = serde_json::from_str(&row.linked_flag_keys)?;

		Ok(KillSwitch {
			id: row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid kill switch ID".to_string()))?,
			org_id: row
				.org_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid org ID".to_string()))
				})
				.transpose()?,
			key: row.key,
			name: row.name,
			description: row.description,
			linked_flag_keys,
			is_active: row.is_active,
			activated_at: row
				.activated_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid activated_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			activated_by: row
				.activated_by
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid user ID".to_string()))
				})
				.transpose()?,
			activation_reason: row.activation_reason,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
				.map_err(|_| FlagsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct SdkKeyRow {
	id: String,
	environment_id: String,
	key_type: String,
	name: String,
	key_hash: String,
	created_by: String,
	created_at: String,
	last_used_at: Option<String>,
	revoked_at: Option<String>,
}

impl TryFrom<SdkKeyRow> for SdkKey {
	type Error = FlagsServerError;

	fn try_from(row: SdkKeyRow) -> Result<Self> {
		let key_type: SdkKeyType = row
			.key_type
			.parse()
			.map_err(|_| FlagsServerError::Internal("Invalid key type".to_string()))?;

		Ok(SdkKey {
			id: row
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid SDK key ID".to_string()))?,
			environment_id: row
				.environment_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid environment ID".to_string()))?,
			key_type,
			name: row.name,
			key_hash: row.key_hash,
			created_by: row
				.created_by
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid user ID".to_string()))?,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			last_used_at: row
				.last_used_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid last_used_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			revoked_at: row
				.revoked_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid revoked_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
		})
	}
}

#[derive(sqlx::FromRow)]
struct ExposureLogRow {
	id: String,
	flag_id: String,
	flag_key: String,
	environment_id: String,
	user_id: Option<String>,
	org_id: Option<String>,
	variant: String,
	reason: String,
	context_hash: String,
	timestamp: String,
}

impl TryFrom<ExposureLogRow> for ExposureLog {
	type Error = FlagsServerError;

	fn try_from(row: ExposureLogRow) -> Result<Self> {
		use loom_flags_core::EvaluationReason;

		let reason: EvaluationReason = serde_json::from_str(&row.reason)
			.map_err(|_| FlagsServerError::Internal("Invalid reason JSON".to_string()))?;

		Ok(ExposureLog {
			id: ExposureLogId(
				row
					.id
					.parse()
					.map_err(|_| FlagsServerError::Internal("Invalid exposure log ID".to_string()))?,
			),
			flag_id: row
				.flag_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID".to_string()))?,
			flag_key: row.flag_key,
			environment_id: row
				.environment_id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid environment ID".to_string()))?,
			user_id: row.user_id,
			org_id: row.org_id,
			variant: row.variant,
			reason,
			context_hash: row.context_hash,
			timestamp: chrono::DateTime::parse_from_rfc3339(&row.timestamp)
				.map_err(|_| FlagsServerError::Internal("Invalid timestamp".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct FlagStatsRow {
	#[allow(dead_code)]
	flag_id: String,
	flag_key: String,
	last_evaluated_at: Option<String>,
	evaluation_count_24h: i64,
	evaluation_count_7d: i64,
	evaluation_count_30d: i64,
}

impl TryFrom<FlagStatsRow> for loom_flags_core::FlagStats {
	type Error = FlagsServerError;

	fn try_from(row: FlagStatsRow) -> Result<Self> {
		Ok(loom_flags_core::FlagStats {
			flag_key: row.flag_key,
			last_evaluated_at: row
				.last_evaluated_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid last_evaluated_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			evaluation_count_24h: row.evaluation_count_24h as u64,
			evaluation_count_7d: row.evaluation_count_7d as u64,
			evaluation_count_30d: row.evaluation_count_30d as u64,
		})
	}
}

#[derive(sqlx::FromRow)]
struct FlagWithStatsRow {
	id: String,
	org_id: Option<String>,
	key: String,
	name: String,
	description: Option<String>,
	tags: String,
	maintainer_user_id: Option<String>,
	variants: String,
	default_variant: String,
	exposure_tracking_enabled: bool,
	created_at: String,
	updated_at: String,
	archived_at: Option<String>,
	last_evaluated_at: Option<String>,
}

impl FlagWithStatsRow {
	fn into_flag(self, prerequisites: Vec<FlagPrerequisite>) -> Result<Flag> {
		let tags: Vec<String> = serde_json::from_str(&self.tags)?;
		let variants: Vec<Variant> = serde_json::from_str(&self.variants)?;

		Ok(Flag {
			id: self
				.id
				.parse()
				.map_err(|_| FlagsServerError::Internal("Invalid flag ID".to_string()))?,
			org_id: self
				.org_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid org ID".to_string()))
				})
				.transpose()?,
			key: self.key,
			name: self.name,
			description: self.description,
			tags,
			maintainer_user_id: self
				.maintainer_user_id
				.map(|s| {
					s.parse()
						.map_err(|_| FlagsServerError::Internal("Invalid user ID".to_string()))
				})
				.transpose()?,
			variants,
			default_variant: self.default_variant,
			prerequisites,
			exposure_tracking_enabled: self.exposure_tracking_enabled,
			created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
				.map_err(|_| FlagsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
				.map_err(|_| FlagsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
			archived_at: self
				.archived_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| FlagsServerError::Internal("Invalid archived_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
		})
	}
}
