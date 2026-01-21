// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

use crate::error::DbError;

#[derive(Debug, Clone)]
pub struct BranchProtectionRuleRecord {
	pub id: Uuid,
	pub repo_id: Uuid,
	pub pattern: String,
	pub block_direct_push: bool,
	pub block_force_push: bool,
	pub block_deletion: bool,
	pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait ProtectionStore: Send + Sync {
	async fn create(
		&self,
		rule: &BranchProtectionRuleRecord,
	) -> Result<BranchProtectionRuleRecord, DbError>;
	async fn list_by_repo(&self, repo_id: Uuid) -> Result<Vec<BranchProtectionRuleRecord>, DbError>;
	async fn get_by_id(&self, id: Uuid) -> Result<Option<BranchProtectionRuleRecord>, DbError>;
	async fn delete(&self, id: Uuid) -> Result<(), DbError>;
}

#[derive(Clone)]
pub struct ProtectionRepository {
	pool: SqlitePool,
}

impl ProtectionRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}
}

#[async_trait]
impl ProtectionStore for ProtectionRepository {
	#[tracing::instrument(skip(self, rule), fields(rule_id = %rule.id, repo_id = %rule.repo_id))]
	async fn create(
		&self,
		rule: &BranchProtectionRuleRecord,
	) -> Result<BranchProtectionRuleRecord, DbError> {
		sqlx::query(
			r#"
			INSERT INTO branch_protection_rules (id, repo_id, pattern, block_direct_push, block_force_push, block_deletion, created_at)
			VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(rule.id.to_string())
		.bind(rule.repo_id.to_string())
		.bind(&rule.pattern)
		.bind(rule.block_direct_push as i32)
		.bind(rule.block_force_push as i32)
		.bind(rule.block_deletion as i32)
		.bind(rule.created_at.to_rfc3339())
		.execute(&self.pool)
		.await
		.map_err(|e| match e {
			sqlx::Error::Database(ref db_err) if db_err.is_unique_violation() => {
				DbError::Conflict("Branch protection rule already exists".to_string())
			}
			_ => DbError::Sqlx(e),
		})?;

		Ok(rule.clone())
	}

	#[tracing::instrument(skip(self), fields(repo_id = %repo_id))]
	async fn list_by_repo(&self, repo_id: Uuid) -> Result<Vec<BranchProtectionRuleRecord>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, repo_id, pattern, block_direct_push, block_force_push, block_deletion, created_at
			FROM branch_protection_rules
			WHERE repo_id = ?
			ORDER BY created_at ASC
			"#,
		)
		.bind(repo_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.iter().map(row_to_rule).collect()
	}

	#[tracing::instrument(skip(self), fields(rule_id = %id))]
	async fn get_by_id(&self, id: Uuid) -> Result<Option<BranchProtectionRuleRecord>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, repo_id, pattern, block_direct_push, block_force_push, block_deletion, created_at
			FROM branch_protection_rules
			WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(|r| row_to_rule(&r)).transpose()
	}

	#[tracing::instrument(skip(self), fields(rule_id = %id))]
	async fn delete(&self, id: Uuid) -> Result<(), DbError> {
		let result = sqlx::query(
			r#"
			DELETE FROM branch_protection_rules WHERE id = ?
			"#,
		)
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		if result.rows_affected() == 0 {
			return Err(DbError::NotFound(
				"Branch protection rule not found".to_string(),
			));
		}

		Ok(())
	}
}

fn row_to_rule(row: &sqlx::sqlite::SqliteRow) -> Result<BranchProtectionRuleRecord, DbError> {
	let id_str: String = row.get("id");
	let repo_id_str: String = row.get("repo_id");
	let created_at_str: String = row.get("created_at");
	let block_direct_push: i32 = row.get("block_direct_push");
	let block_force_push: i32 = row.get("block_force_push");
	let block_deletion: i32 = row.get("block_deletion");

	Ok(BranchProtectionRuleRecord {
		id: Uuid::parse_str(&id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		repo_id: Uuid::parse_str(&repo_id_str).map_err(|e| DbError::Internal(e.to_string()))?,
		pattern: row.get("pattern"),
		block_direct_push: block_direct_push != 0,
		block_force_push: block_force_push != 0,
		block_deletion: block_deletion != 0,
		created_at: DateTime::parse_from_rfc3339(&created_at_str)
			.map(|d| d.with_timezone(&Utc))
			.map_err(|e| DbError::Internal(e.to_string()))?,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::str::FromStr;

	async fn create_protection_test_pool() -> SqlitePool {
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
			CREATE TABLE IF NOT EXISTS branch_protection_rules (
				id TEXT PRIMARY KEY,
				repo_id TEXT NOT NULL,
				pattern TEXT NOT NULL,
				block_direct_push INTEGER NOT NULL DEFAULT 0,
				block_force_push INTEGER NOT NULL DEFAULT 0,
				block_deletion INTEGER NOT NULL DEFAULT 0,
				created_at TEXT NOT NULL,
				UNIQUE(repo_id, pattern)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> ProtectionRepository {
		let pool = create_protection_test_pool().await;
		ProtectionRepository::new(pool)
	}

	fn make_rule(repo_id: Uuid, pattern: &str) -> BranchProtectionRuleRecord {
		BranchProtectionRuleRecord {
			id: Uuid::new_v4(),
			repo_id,
			pattern: pattern.to_string(),
			block_direct_push: true,
			block_force_push: true,
			block_deletion: false,
			created_at: Utc::now(),
		}
	}

	#[tokio::test]
	async fn test_create_and_get_rule() {
		let repo = make_repo().await;
		let repo_id = Uuid::new_v4();
		let rule = make_rule(repo_id, "main");

		let created = repo.create(&rule).await.unwrap();
		assert_eq!(created.id, rule.id);
		assert_eq!(created.pattern, "main");
		assert!(created.block_direct_push);
		assert!(created.block_force_push);
		assert!(!created.block_deletion);

		let fetched = repo.get_by_id(rule.id).await.unwrap();
		assert!(fetched.is_some());
		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, rule.id);
		assert_eq!(fetched.repo_id, repo_id);
		assert_eq!(fetched.pattern, "main");
	}

	#[tokio::test]
	async fn test_get_rule_not_found() {
		let repo = make_repo().await;
		let result = repo.get_by_id(Uuid::new_v4()).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_list_rules_for_repo() {
		let repo = make_repo().await;
		let repo_id = Uuid::new_v4();
		let other_repo_id = Uuid::new_v4();

		let rule1 = make_rule(repo_id, "main");
		let rule2 = make_rule(repo_id, "develop");
		let rule3 = make_rule(other_repo_id, "main");

		repo.create(&rule1).await.unwrap();
		repo.create(&rule2).await.unwrap();
		repo.create(&rule3).await.unwrap();

		let rules = repo.list_by_repo(repo_id).await.unwrap();
		assert_eq!(rules.len(), 2);
		let patterns: Vec<_> = rules.iter().map(|r| r.pattern.as_str()).collect();
		assert!(patterns.contains(&"main"));
		assert!(patterns.contains(&"develop"));

		let other_rules = repo.list_by_repo(other_repo_id).await.unwrap();
		assert_eq!(other_rules.len(), 1);
		assert_eq!(other_rules[0].pattern, "main");
	}

	#[tokio::test]
	async fn test_delete_rule() {
		let repo = make_repo().await;
		let repo_id = Uuid::new_v4();
		let rule = make_rule(repo_id, "main");

		repo.create(&rule).await.unwrap();
		let fetched = repo.get_by_id(rule.id).await.unwrap();
		assert!(fetched.is_some());

		repo.delete(rule.id).await.unwrap();

		let fetched_after = repo.get_by_id(rule.id).await.unwrap();
		assert!(fetched_after.is_none());

		let delete_again = repo.delete(rule.id).await;
		assert!(matches!(delete_again, Err(DbError::NotFound(_))));
	}
}
