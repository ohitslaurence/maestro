// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! SQLite database operations for thread persistence.
//!
//! This module re-exports repositories from loom-db and provides
//! server-specific migrations.

pub mod audit;
pub mod cse;

use sqlx::sqlite::SqlitePool;

pub use audit::AuditRepository;

use crate::error::ServerError;

pub use loom_server_db::{
	create_pool, ApiKeyRepository, DbError, GithubInstallation, GithubInstallationInfo, GithubRepo,
	OrgRepository, SessionRepository, ShareRepository, TeamRepository, ThreadRepository,
	ThreadSearchHit, UserRepository,
};

/// Run all database migrations (001-035).
///
/// # Arguments
/// * `pool` - SQLite connection pool
///
/// # Errors
/// Returns `ServerError::Database` if migrations fail.
///
/// # Note
/// Migrations are idempotent - safe to run multiple times.
#[tracing::instrument(skip(pool))]
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), ServerError> {
	let m1 = include_str!("../../migrations/001_create_threads.sql");
	sqlx::query(m1).execute(pool).await?;

	let m2 = include_str!("../../migrations/002_add_visibility.sql");
	if let Err(e) = sqlx::query(m2).execute(pool).await {
		let msg = e.to_string();
		if !msg.contains("duplicate column name: visibility")
			&& !msg.contains("duplicate column")
			&& !msg.contains("already exists")
		{
			return Err(e.into());
		}
	}

	let m3 = include_str!("../../migrations/003_add_git_metadata.sql");
	if let Err(e) = sqlx::query(m3).execute(pool).await {
		let msg = e.to_string();
		if !msg.contains("duplicate column") && !msg.contains("already exists") {
			return Err(e.into());
		}
	}

	let m4 = include_str!("../../migrations/004_git_repos_and_commits.sql");
	for stmt in m4.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("duplicate column")
				&& !msg.contains("already exists")
				&& !msg.contains("table thread_repos already exists")
				&& !msg.contains("table thread_commits already exists")
			{
				return Err(e.into());
			}
		}
	}

	let m5 = include_str!("../../migrations/005_thread_fts.sql");

	if let Some(vt_end) = m5.find(");") {
		let create_vt = &m5[..vt_end + 2];
		if let Err(e) = sqlx::query(create_vt.trim()).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("table thread_fts already exists") {
				tracing::warn!(error = %e, "FTS CREATE VIRTUAL TABLE failed");
			}
		}

		let remaining = &m5[vt_end + 2..];
		for trigger_block in remaining.split("END;") {
			let trigger = trigger_block.trim();
			if trigger.is_empty() || !trigger.contains("CREATE TRIGGER") {
				continue;
			}
			let full_trigger = format!("{trigger} END;");
			if let Err(e) = sqlx::query(&full_trigger).execute(pool).await {
				let msg = e.to_string();
				if !msg.contains("already exists") && !msg.contains("trigger") {
					tracing::warn!(error = %e, stmt = %full_trigger.chars().take(80).collect::<String>(), "FTS trigger creation failed");
				}
			}
		}
	}

	let m6 = include_str!("../../migrations/006_cse_cache.sql");
	for stmt in m6.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m7 = include_str!("../../migrations/007_github_app.sql");
	for stmt in m7.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists")
				&& !msg.contains("duplicate column")
				&& !msg.contains("table github_installations already exists")
				&& !msg.contains("table github_installation_repos already exists")
			{
				return Err(e.into());
			}
		}
	}

	let m8 = include_str!("../../migrations/008_auth_users.sql");
	for stmt in m8.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m9 = include_str!("../../migrations/009_auth_sessions.sql");
	for stmt in m9.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m10 = include_str!("../../migrations/010_auth_orgs.sql");
	for stmt in m10.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m11 = include_str!("../../migrations/011_auth_teams.sql");
	for stmt in m11.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m12 = include_str!("../../migrations/012_auth_api_keys.sql");
	for stmt in m12.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m13 = include_str!("../../migrations/013_auth_threads_ext.sql");
	for stmt in m13.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m14 = include_str!("../../migrations/014_auth_audit.sql");
	for stmt in m14.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m15 = include_str!("../../migrations/015_impersonation_sessions.sql");
	for stmt in m15.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m16 = include_str!("../../migrations/016_user_locale.sql");
	for stmt in m16.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m17 = include_str!("../../migrations/017_fix_sessions_token_hash.sql");
	for stmt in m17.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m18 = include_str!("../../migrations/018_scm_maintenance.sql");
	for stmt in m18.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m19 = include_str!("../../migrations/019_jobs.sql");
	for stmt in m19.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m20 = include_str!("../../migrations/020_scm_repos.sql");
	for stmt in m20.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m21 = include_str!("../../migrations/021_scm_webhooks.sql");
	for stmt in m21.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m22 = include_str!("../../migrations/022_scm_mirrors.sql");
	for stmt in m22.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m23 = include_str!("../../migrations/023_add_username.sql");
	for stmt in m23.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m24 = include_str!("../../migrations/024_ws_tokens.sql");
	for stmt in m24.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m25 = include_str!("../../migrations/025_weaver_secrets.sql");
	for stmt in m25.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m26 = include_str!("../../migrations/026_audit_enrichment.sql");
	for stmt in m26.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m27 = include_str!("../../migrations/027_docs_fts.sql");
	if let Err(e) = sqlx::query(m27).execute(pool).await {
		let msg = e.to_string();
		if !msg.contains("already exists") && !msg.contains("table docs_fts already exists") {
			tracing::warn!(error = %e, "docs_fts table creation failed");
		}
	}

	let m28 = include_str!("../../migrations/028_wgtunnel.sql");
	for stmt in m28.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m29 = include_str!("../../migrations/029_scim_support.sql");
	for stmt in m29.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m30 = include_str!("../../migrations/030_feature_flags.sql");
	for stmt in m30.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m31 = include_str!("../../migrations/031_exposure_tracking.sql");
	for stmt in m31.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m32 = include_str!("../../migrations/032_analytics.sql");
	for stmt in m32.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m33 = include_str!("../../migrations/033_crash_analytics.sql");
	for stmt in m33.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m34 = include_str!("../../migrations/034_cron_monitoring.sql");
	for stmt in m34.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	let m35 = include_str!("../../migrations/035_sessions.sql");
	for stmt in m35.split(';').filter(|s| !s.trim().is_empty()) {
		if let Err(e) = sqlx::query(stmt).execute(pool).await {
			let msg = e.to_string();
			if !msg.contains("already exists") && !msg.contains("duplicate column") {
				return Err(e.into());
			}
		}
	}

	tracing::debug!("database migrations complete");
	Ok(())
}
