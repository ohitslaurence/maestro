// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqliteSynchronous};
use std::str::FromStr;

use crate::error::DbError;

/// Create a SqlitePool with WAL mode and common settings.
///
/// # Arguments
/// * `database_url` - SQLite connection string (e.g., "sqlite:./loom.db")
///
/// # Errors
/// Returns `DbError::Internal` if the URL is invalid or connection fails.
#[tracing::instrument(skip(database_url))]
pub async fn create_pool(database_url: &str) -> Result<SqlitePool, DbError> {
	let options = SqliteConnectOptions::from_str(database_url)
		.map_err(|e| DbError::Internal(format!("Invalid database URL: {e}")))?
		.journal_mode(SqliteJournalMode::Wal)
		.synchronous(SqliteSynchronous::Normal)
		.create_if_missing(true);

	let pool = SqlitePool::connect_with(options).await?;

	tracing::debug!("database pool created");
	Ok(pool)
}
