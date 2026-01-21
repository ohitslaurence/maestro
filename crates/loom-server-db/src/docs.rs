// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Documentation search repository for FTS5 operations.

use async_trait::async_trait;
use sqlx::{sqlite::SqlitePool, FromRow};

use crate::error::DbError;

#[derive(Debug, Clone, FromRow, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct DocSearchHit {
	pub path: String,
	pub title: String,
	pub summary: String,
	pub diataxis: String,
	pub tags: String,
	pub snippet: String,
	pub score: f64,
}

#[derive(Debug, Clone)]
pub struct DocSearchParams {
	pub query: String,
	pub diataxis: Option<String>,
	pub limit: u32,
	pub offset: u32,
}

impl Default for DocSearchParams {
	fn default() -> Self {
		Self {
			query: String::new(),
			diataxis: None,
			limit: 20,
			offset: 0,
		}
	}
}

#[derive(Debug, Clone)]
pub struct DocIndexEntry {
	pub doc_id: String,
	pub path: String,
	pub title: String,
	pub summary: String,
	pub body: String,
	pub diataxis: String,
	pub tags: String,
	pub updated_at: String,
}

#[derive(Clone)]
pub struct DocsRepository {
	pool: SqlitePool,
}

impl DocsRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	#[tracing::instrument(skip(self))]
	pub async fn clear_docs(&self) -> Result<(), DbError> {
		sqlx::query("DELETE FROM docs_fts")
			.execute(&self.pool)
			.await?;
		tracing::debug!("cleared docs_fts table");
		Ok(())
	}

	#[tracing::instrument(skip(self, entries), fields(count = entries.len()))]
	pub async fn insert_docs(&self, entries: &[DocIndexEntry]) -> Result<(), DbError> {
		let mut tx = self.pool.begin().await?;

		sqlx::query("DELETE FROM docs_fts")
			.execute(&mut *tx)
			.await?;

		for entry in entries {
			sqlx::query(
				r#"
				INSERT INTO docs_fts (doc_id, path, title, summary, body, diataxis, tags, updated_at)
				VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
				"#,
			)
			.bind(&entry.doc_id)
			.bind(&entry.path)
			.bind(&entry.title)
			.bind(&entry.summary)
			.bind(&entry.body)
			.bind(&entry.diataxis)
			.bind(&entry.tags)
			.bind(&entry.updated_at)
			.execute(&mut *tx)
			.await?;
		}

		tx.commit().await?;
		tracing::info!(count = entries.len(), "inserted docs into search index");
		Ok(())
	}

	#[tracing::instrument(skip(self), fields(query = %params.query))]
	pub async fn search(&self, params: &DocSearchParams) -> Result<Vec<DocSearchHit>, DbError> {
		let query = params.query.trim();
		if query.is_empty() {
			return Ok(vec![]);
		}

		let escaped_query = format!("\"{}\"", query.replace('"', "\"\""));

		let hits: Vec<DocSearchHit> = if let Some(ref diataxis) = params.diataxis {
			sqlx::query_as(
				r#"
				SELECT
					path,
					title,
					COALESCE(summary, '') as summary,
					diataxis,
					COALESCE(tags, '') as tags,
					snippet(docs_fts, 4, '<mark>', '</mark>', '…', 24) AS snippet,
					bm25(docs_fts) AS score
				FROM docs_fts
				WHERE docs_fts MATCH ?1 AND diataxis = ?2
				ORDER BY score
				LIMIT ?3 OFFSET ?4
				"#,
			)
			.bind(&escaped_query)
			.bind(diataxis)
			.bind(params.limit)
			.bind(params.offset)
			.fetch_all(&self.pool)
			.await?
		} else {
			sqlx::query_as(
				r#"
				SELECT
					path,
					title,
					COALESCE(summary, '') as summary,
					diataxis,
					COALESCE(tags, '') as tags,
					snippet(docs_fts, 4, '<mark>', '</mark>', '…', 24) AS snippet,
					bm25(docs_fts) AS score
				FROM docs_fts
				WHERE docs_fts MATCH ?1
				ORDER BY score
				LIMIT ?2 OFFSET ?3
				"#,
			)
			.bind(&escaped_query)
			.bind(params.limit)
			.bind(params.offset)
			.fetch_all(&self.pool)
			.await?
		};

		tracing::debug!(count = hits.len(), "docs search completed");
		Ok(hits)
	}
}

#[async_trait]
pub trait DocsStore: Send + Sync {
	async fn clear_docs(&self) -> Result<(), DbError>;
	async fn insert_docs(&self, entries: &[DocIndexEntry]) -> Result<(), DbError>;
	async fn search(&self, params: &DocSearchParams) -> Result<Vec<DocSearchHit>, DbError>;
}

#[async_trait]
impl DocsStore for DocsRepository {
	async fn clear_docs(&self) -> Result<(), DbError> {
		self.clear_docs().await
	}

	async fn insert_docs(&self, entries: &[DocIndexEntry]) -> Result<(), DbError> {
		self.insert_docs(entries).await
	}

	async fn search(&self, params: &DocSearchParams) -> Result<Vec<DocSearchHit>, DbError> {
		self.search(params).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn create_docs_test_pool() -> SqlitePool {
		let pool = SqlitePool::connect(":memory:").await.unwrap();
		sqlx::query(
			r#"
			CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
				doc_id UNINDEXED,
				path UNINDEXED,
				title,
				summary,
				body,
				diataxis UNINDEXED,
				tags,
				updated_at UNINDEXED,
				tokenize = 'unicode61',
				prefix = '2 3'
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();
		pool
	}

	#[tokio::test]
	async fn test_insert_and_search_docs() {
		let pool = create_docs_test_pool().await;
		let repo = DocsRepository::new(pool);

		let entries = vec![
			DocIndexEntry {
				doc_id: "getting-started".to_string(),
				path: "/docs/getting-started".to_string(),
				title: "Getting Started Guide".to_string(),
				summary: "Learn how to get started with Loom".to_string(),
				body: "This guide will help you install and configure Loom for your project.".to_string(),
				diataxis: "tutorial".to_string(),
				tags: "beginner setup installation".to_string(),
				updated_at: "2025-01-01T00:00:00Z".to_string(),
			},
			DocIndexEntry {
				doc_id: "api-reference".to_string(),
				path: "/docs/api-reference".to_string(),
				title: "API Reference".to_string(),
				summary: "Complete API documentation".to_string(),
				body: "Detailed reference for all API endpoints and methods.".to_string(),
				diataxis: "reference".to_string(),
				tags: "api endpoints methods".to_string(),
				updated_at: "2025-01-01T00:00:00Z".to_string(),
			},
		];

		repo.insert_docs(&entries).await.unwrap();

		let params = DocSearchParams {
			query: "getting started".to_string(),
			diataxis: None,
			limit: 10,
			offset: 0,
		};
		let results = repo.search(&params).await.unwrap();
		assert_eq!(results.len(), 1);
		assert_eq!(results[0].title, "Getting Started Guide");

		let params = DocSearchParams {
			query: "API".to_string(),
			diataxis: None,
			limit: 10,
			offset: 0,
		};
		let results = repo.search(&params).await.unwrap();
		assert_eq!(results.len(), 1);
		assert_eq!(results[0].title, "API Reference");
	}

	#[tokio::test]
	async fn test_search_no_results() {
		let pool = create_docs_test_pool().await;
		let repo = DocsRepository::new(pool);

		let entries = vec![DocIndexEntry {
			doc_id: "getting-started".to_string(),
			path: "/docs/getting-started".to_string(),
			title: "Getting Started Guide".to_string(),
			summary: "Learn how to get started".to_string(),
			body: "This guide will help you.".to_string(),
			diataxis: "tutorial".to_string(),
			tags: "beginner".to_string(),
			updated_at: "2025-01-01T00:00:00Z".to_string(),
		}];

		repo.insert_docs(&entries).await.unwrap();

		let params = DocSearchParams {
			query: "nonexistent".to_string(),
			diataxis: None,
			limit: 10,
			offset: 0,
		};
		let results = repo.search(&params).await.unwrap();
		assert!(results.is_empty());
	}

	#[tokio::test]
	async fn test_clear_docs() {
		let pool = create_docs_test_pool().await;
		let repo = DocsRepository::new(pool);

		let entries = vec![DocIndexEntry {
			doc_id: "test-doc".to_string(),
			path: "/docs/test".to_string(),
			title: "Test Document".to_string(),
			summary: "A test document".to_string(),
			body: "Test content here.".to_string(),
			diataxis: "reference".to_string(),
			tags: "test".to_string(),
			updated_at: "2025-01-01T00:00:00Z".to_string(),
		}];

		repo.insert_docs(&entries).await.unwrap();

		let params = DocSearchParams {
			query: "test".to_string(),
			diataxis: None,
			limit: 10,
			offset: 0,
		};
		let results = repo.search(&params).await.unwrap();
		assert_eq!(results.len(), 1);

		repo.clear_docs().await.unwrap();

		let results = repo.search(&params).await.unwrap();
		assert!(results.is_empty());
	}
}
