// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use sqlx::sqlite::SqlitePool;

pub async fn create_test_pool() -> SqlitePool {
	SqlitePool::connect(":memory:").await.unwrap()
}

pub async fn create_users_table(pool: &SqlitePool) {
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
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_identities_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS identities (
			id TEXT PRIMARY KEY,
			user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			provider TEXT NOT NULL,
			provider_user_id TEXT NOT NULL,
			email TEXT NOT NULL,
			email_verified INTEGER DEFAULT 0,
			access_token TEXT,
			refresh_token TEXT,
			token_expires_at TEXT,
			created_at TEXT NOT NULL,
			UNIQUE(provider, provider_user_id)
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_job_definitions_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS job_definitions (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			description TEXT,
			job_type TEXT NOT NULL,
			interval_secs INTEGER,
			enabled INTEGER NOT NULL DEFAULT 1,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_job_runs_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS job_runs (
			id TEXT PRIMARY KEY,
			job_id TEXT NOT NULL REFERENCES job_definitions(id),
			status TEXT NOT NULL,
			started_at TEXT NOT NULL,
			completed_at TEXT,
			duration_ms INTEGER,
			error_message TEXT,
			retry_count INTEGER NOT NULL DEFAULT 0,
			triggered_by TEXT NOT NULL,
			metadata TEXT
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_user_test_pool() -> SqlitePool {
	let pool = create_test_pool().await;
	create_users_table(&pool).await;
	create_identities_table(&pool).await;
	pool
}

pub async fn create_job_test_pool() -> SqlitePool {
	let pool = create_test_pool().await;
	create_job_definitions_table(&pool).await;
	create_job_runs_table(&pool).await;
	pool
}

pub async fn create_sessions_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS sessions (
			id TEXT PRIMARY KEY,
			user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
			session_type TEXT NOT NULL,
			token_hash TEXT,
			created_at TEXT NOT NULL,
			last_used_at TEXT NOT NULL,
			expires_at TEXT NOT NULL,
			ip_address TEXT,
			user_agent TEXT,
			geo_city TEXT,
			geo_country TEXT
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();

	sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash)")
		.execute(pool)
		.await
		.unwrap();
}

pub async fn create_session_test_pool() -> SqlitePool {
	let pool = create_test_pool().await;
	create_users_table(&pool).await;
	create_sessions_table(&pool).await;
	pool
}

pub async fn create_repos_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS repos (
			id TEXT PRIMARY KEY NOT NULL,
			owner_type TEXT NOT NULL CHECK (owner_type IN ('user', 'org')),
			owner_id TEXT NOT NULL,
			name TEXT NOT NULL,
			visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'public')),
			default_branch TEXT NOT NULL DEFAULT 'cannon',
			deleted_at TEXT,
			created_at TEXT NOT NULL,
			updated_at TEXT NOT NULL,
			UNIQUE (owner_type, owner_id, name)
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_repo_team_access_table(pool: &SqlitePool) {
	sqlx::query(
		r#"
		CREATE TABLE IF NOT EXISTS repo_team_access (
			repo_id TEXT NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
			team_id TEXT NOT NULL,
			role TEXT NOT NULL CHECK (role IN ('read', 'write', 'admin')),
			PRIMARY KEY (repo_id, team_id)
		)
		"#,
	)
	.execute(pool)
	.await
	.unwrap();
}

pub async fn create_scm_test_pool() -> SqlitePool {
	let pool = create_test_pool().await;
	create_repos_table(&pool).await;
	create_repo_team_access_table(&pool).await;
	pool
}
