// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! CSE (Custom Search Engine) cache repository for database operations.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::sqlite::SqlitePool;

use crate::error::DbError;

/// Repository for CSE cache database operations.
#[derive(Clone)]
pub struct CseRepository {
	pool: SqlitePool,
}

impl CseRepository {
	/// Create a new CSE repository with the given pool.
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	/// Get the underlying pool reference.
	pub fn pool(&self) -> &SqlitePool {
		&self.pool
	}

	/// Get cached CSE results for a query.
	///
	/// Returns None if no valid cache entry exists or if the entry is expired (>24h).
	#[tracing::instrument(skip(self), fields(query = %query, max_results = max_results))]
	pub async fn get_cached_results(
		&self,
		query: &str,
		max_results: u32,
	) -> Result<Option<String>, DbError> {
		let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();
		let normalized_query = normalize_cache_query(query);

		let row: Option<(String,)> = sqlx::query_as(
			r#"
			SELECT response_json
			FROM cse_cache
			WHERE query = ?1
			  AND max_results = ?2
			  AND created_at >= ?3
			LIMIT 1
			"#,
		)
		.bind(&normalized_query)
		.bind(max_results as i64)
		.bind(&cutoff)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some((json,)) => {
				tracing::debug!(query = %query, max_results = max_results, "cse_cache: hit");
				Ok(Some(json))
			}
			None => {
				tracing::debug!(query = %query, max_results = max_results, "cse_cache: miss");
				Ok(None)
			}
		}
	}

	/// Cache CSE results for a query.
	///
	/// Uses UPSERT to update existing cache entries.
	#[tracing::instrument(skip(self, response_json), fields(query = %query, max_results = max_results))]
	pub async fn cache_results(
		&self,
		query: &str,
		max_results: u32,
		response_json: &str,
	) -> Result<(), DbError> {
		let now = Utc::now().to_rfc3339();
		let normalized_query = normalize_cache_query(query);

		sqlx::query(
			r#"
			INSERT INTO cse_cache (query, max_results, response_json, created_at)
			VALUES (?1, ?2, ?3, ?4)
			ON CONFLICT(query, max_results) DO UPDATE SET
				response_json = excluded.response_json,
				created_at    = excluded.created_at
			"#,
		)
		.bind(&normalized_query)
		.bind(max_results as i64)
		.bind(response_json)
		.bind(&now)
		.execute(&self.pool)
		.await?;

		tracing::debug!(query = %query, max_results = max_results, "cse_cache: stored");
		Ok(())
	}

	/// Delete expired cache entries (older than 24 hours).
	///
	/// Returns the number of entries deleted.
	#[tracing::instrument(skip(self))]
	pub async fn cleanup_expired(&self) -> Result<u64, DbError> {
		let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();

		let result = sqlx::query(
			r#"
			DELETE FROM cse_cache
			WHERE created_at < ?1
			"#,
		)
		.bind(&cutoff)
		.execute(&self.pool)
		.await?;

		let deleted = result.rows_affected();
		if deleted > 0 {
			tracing::debug!(deleted = deleted, "cse_cache: cleaned up expired entries");
		}

		Ok(deleted)
	}
}

/// Normalizes a query string for cache key purposes.
///
/// - Collapses multiple whitespace into single spaces
/// - Converts to lowercase
/// - Trims leading/trailing whitespace
pub fn normalize_cache_query(query: &str) -> String {
	query
		.split_whitespace()
		.collect::<Vec<_>>()
		.join(" ")
		.to_lowercase()
}

#[async_trait]
pub trait CseStore: Send + Sync {
	async fn get_cached_results(
		&self,
		query: &str,
		max_results: u32,
	) -> Result<Option<String>, DbError>;
	async fn cache_results(
		&self,
		query: &str,
		max_results: u32,
		response_json: &str,
	) -> Result<(), DbError>;
	async fn cleanup_expired(&self) -> Result<u64, DbError>;
}

#[async_trait]
impl CseStore for CseRepository {
	async fn get_cached_results(
		&self,
		query: &str,
		max_results: u32,
	) -> Result<Option<String>, DbError> {
		self.get_cached_results(query, max_results).await
	}

	async fn cache_results(
		&self,
		query: &str,
		max_results: u32,
		response_json: &str,
	) -> Result<(), DbError> {
		self.cache_results(query, max_results, response_json).await
	}

	async fn cleanup_expired(&self) -> Result<u64, DbError> {
		self.cleanup_expired().await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::str::FromStr;

	#[test]
	fn test_normalize_cache_query() {
		assert_eq!(normalize_cache_query("Hello World"), "hello world");
		assert_eq!(
			normalize_cache_query("  multiple   spaces  "),
			"multiple spaces"
		);
		assert_eq!(normalize_cache_query("UPPERCASE"), "uppercase");
		assert_eq!(normalize_cache_query("  trim  me  "), "trim me");
		assert_eq!(
			normalize_cache_query("already normalized"),
			"already normalized"
		);
	}

	async fn create_cse_test_pool() -> SqlitePool {
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
			CREATE TABLE IF NOT EXISTS cse_cache (
				query TEXT NOT NULL,
				max_results INTEGER NOT NULL,
				response_json TEXT NOT NULL,
				created_at TEXT NOT NULL,
				PRIMARY KEY (query, max_results)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> CseRepository {
		let pool = create_cse_test_pool().await;
		CseRepository::new(pool)
	}

	#[tokio::test]
	async fn test_cache_and_get_results() {
		let repo = make_repo().await;
		let query = "rust programming";
		let max_results = 10;
		let response_json = r#"{"results": [{"title": "Rust Lang"}]}"#;

		repo
			.cache_results(query, max_results, response_json)
			.await
			.unwrap();

		let cached = repo.get_cached_results(query, max_results).await.unwrap();
		assert!(cached.is_some());
		assert_eq!(cached.unwrap(), response_json);

		let cached_normalized = repo
			.get_cached_results("  Rust   Programming  ", max_results)
			.await
			.unwrap();
		assert!(cached_normalized.is_some());
		assert_eq!(cached_normalized.unwrap(), response_json);

		let different_max = repo.get_cached_results(query, 5).await.unwrap();
		assert!(different_max.is_none());
	}

	#[tokio::test]
	async fn test_get_cache_miss() {
		let repo = make_repo().await;
		let result = repo
			.get_cached_results("nonexistent query", 10)
			.await
			.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_cleanup_expired() {
		let repo = make_repo().await;

		let old_time = (Utc::now() - Duration::hours(48)).to_rfc3339();
		sqlx::query(
			r#"
			INSERT INTO cse_cache (query, max_results, response_json, created_at)
			VALUES (?1, ?2, ?3, ?4)
			"#,
		)
		.bind("old query")
		.bind(10_i64)
		.bind(r#"{"results": []}"#)
		.bind(&old_time)
		.execute(repo.pool())
		.await
		.unwrap();

		repo
			.cache_results("new query", 10, r#"{"results": []}"#)
			.await
			.unwrap();

		let deleted = repo.cleanup_expired().await.unwrap();
		assert_eq!(deleted, 1);

		let old_cached = repo.get_cached_results("old query", 10).await.unwrap();
		assert!(old_cached.is_none());

		let new_cached = repo.get_cached_results("new query", 10).await.unwrap();
		assert!(new_cached.is_some());
	}
}
