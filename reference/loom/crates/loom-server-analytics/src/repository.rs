// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Database repository for analytics data.
//!
//! This module provides the [`AnalyticsRepository`] trait and its SQLite implementation
//! for persisting persons, events, identities, merges, and API keys.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tracing::instrument;

use loom_analytics_core::{
	AnalyticsApiKey, AnalyticsApiKeyId, AnalyticsKeyType, Event, EventId, IdentityType, MergeReason,
	OrgId, Person, PersonId, PersonIdentity, PersonIdentityId, PersonMerge, PersonMergeId,
	PersonWithIdentities,
};

use crate::error::{AnalyticsServerError, Result};

/// Repository trait for analytics data operations.
///
/// This trait defines all CRUD operations for the analytics system's data entities:
/// - Persons and their properties
/// - Person identities (distinct_id mappings)
/// - Events
/// - Person merges (audit trail)
/// - API keys
#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait AnalyticsRepository: Send + Sync {
	// Person operations
	async fn create_person(&self, person: &Person) -> Result<()>;
	async fn get_person_by_id(&self, id: PersonId) -> Result<Option<Person>>;
	async fn get_person_with_identities(&self, id: PersonId) -> Result<Option<PersonWithIdentities>>;
	async fn list_persons(&self, org_id: OrgId, limit: u32, offset: u32) -> Result<Vec<Person>>;
	async fn update_person(&self, person: &Person) -> Result<()>;
	async fn count_persons(&self, org_id: OrgId) -> Result<u64>;

	// Person identity operations
	async fn create_identity(&self, identity: &PersonIdentity) -> Result<()>;
	async fn get_identity_by_distinct_id(
		&self,
		org_id: OrgId,
		distinct_id: &str,
	) -> Result<Option<PersonIdentity>>;
	async fn list_identities_for_person(&self, person_id: PersonId) -> Result<Vec<PersonIdentity>>;
	async fn transfer_identities(
		&self,
		from_person_id: PersonId,
		to_person_id: PersonId,
	) -> Result<u64>;

	// Event operations
	async fn insert_event(&self, event: &Event) -> Result<()>;
	async fn insert_events(&self, events: &[Event]) -> Result<u64>;
	async fn get_event_by_id(&self, id: EventId) -> Result<Option<Event>>;
	async fn list_events(
		&self,
		org_id: OrgId,
		distinct_id: Option<&str>,
		event_name: Option<&str>,
		start_time: Option<DateTime<Utc>>,
		end_time: Option<DateTime<Utc>>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<Event>>;
	async fn count_events(
		&self,
		org_id: OrgId,
		distinct_id: Option<&str>,
		event_name: Option<&str>,
		start_time: Option<DateTime<Utc>>,
		end_time: Option<DateTime<Utc>>,
	) -> Result<u64>;
	async fn reassign_events(&self, from_person_id: PersonId, to_person_id: PersonId) -> Result<u64>;

	// Person merge operations
	async fn create_merge(&self, merge: &PersonMerge) -> Result<()>;
	async fn list_merges_for_person(&self, person_id: PersonId) -> Result<Vec<PersonMerge>>;

	// API key operations
	async fn create_api_key(&self, key: &AnalyticsApiKey) -> Result<()>;
	async fn get_api_key_by_id(&self, id: AnalyticsApiKeyId) -> Result<Option<AnalyticsApiKey>>;
	async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<AnalyticsApiKey>>;
	async fn list_api_keys(&self, org_id: OrgId) -> Result<Vec<AnalyticsApiKey>>;
	async fn revoke_api_key(&self, id: AnalyticsApiKeyId) -> Result<bool>;
	async fn update_api_key_last_used(&self, id: AnalyticsApiKeyId) -> Result<()>;
	async fn find_api_key_by_verification(
		&self,
		raw_key: &str,
		org_id: OrgId,
	) -> Result<Option<AnalyticsApiKey>>;

	/// Finds an API key by verifying the raw key against all stored hashes.
	/// This is used when the org_id is not known (e.g., during initial authentication).
	async fn find_api_key_by_raw(&self, raw_key: &str) -> Result<Option<AnalyticsApiKey>>;
}

/// SQLite implementation of [`AnalyticsRepository`].
#[derive(Clone)]
pub struct SqliteAnalyticsRepository {
	pool: SqlitePool,
}

impl SqliteAnalyticsRepository {
	/// Creates a new repository using the given SQLite connection pool.
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}
}

#[async_trait]
impl AnalyticsRepository for SqliteAnalyticsRepository {
	// Person operations

	#[instrument(skip(self, person), fields(person_id = %person.id, org_id = %person.org_id))]
	async fn create_person(&self, person: &Person) -> Result<()> {
		let properties_json = serde_json::to_string(&person.properties)?;

		sqlx::query(
			r#"
			INSERT INTO analytics_persons (id, org_id, properties, created_at, updated_at, merged_into_id, merged_at)
			VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(person.id.0.to_string())
		.bind(person.org_id.0.to_string())
		.bind(properties_json)
		.bind(person.created_at.to_rfc3339())
		.bind(person.updated_at.to_rfc3339())
		.bind(person.merged_into_id.map(|id| id.0.to_string()))
		.bind(person.merged_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(person_id = %id))]
	async fn get_person_by_id(&self, id: PersonId) -> Result<Option<Person>> {
		let row = sqlx::query_as::<_, PersonRow>(
			r#"
			SELECT id, org_id, properties, created_at, updated_at, merged_into_id, merged_at
			FROM analytics_persons
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(person_id = %id))]
	async fn get_person_with_identities(&self, id: PersonId) -> Result<Option<PersonWithIdentities>> {
		let person = match self.get_person_by_id(id).await? {
			Some(p) => p,
			None => return Ok(None),
		};

		let identities = self.list_identities_for_person(id).await?;
		Ok(Some(PersonWithIdentities::new(person, identities)))
	}

	#[instrument(skip(self), fields(org_id = %org_id))]
	async fn list_persons(&self, org_id: OrgId, limit: u32, offset: u32) -> Result<Vec<Person>> {
		let rows = sqlx::query_as::<_, PersonRow>(
			r#"
			SELECT id, org_id, properties, created_at, updated_at, merged_into_id, merged_at
			FROM analytics_persons
			WHERE org_id = ? AND merged_into_id IS NULL
			ORDER BY created_at DESC
			LIMIT ? OFFSET ?
			"#,
		)
		.bind(org_id.0.to_string())
		.bind(limit as i64)
		.bind(offset as i64)
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self, person), fields(person_id = %person.id))]
	async fn update_person(&self, person: &Person) -> Result<()> {
		let properties_json = serde_json::to_string(&person.properties)?;

		sqlx::query(
			r#"
			UPDATE analytics_persons
			SET properties = ?, updated_at = ?, merged_into_id = ?, merged_at = ?
			WHERE id = ?
			"#,
		)
		.bind(properties_json)
		.bind(person.updated_at.to_rfc3339())
		.bind(person.merged_into_id.map(|id| id.0.to_string()))
		.bind(person.merged_at.map(|dt| dt.to_rfc3339()))
		.bind(person.id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(org_id = %org_id))]
	async fn count_persons(&self, org_id: OrgId) -> Result<u64> {
		let (count,): (i64,) = sqlx::query_as(
			r#"
			SELECT COUNT(*) FROM analytics_persons
			WHERE org_id = ? AND merged_into_id IS NULL
			"#,
		)
		.bind(org_id.0.to_string())
		.fetch_one(&self.pool)
		.await?;

		Ok(count as u64)
	}

	// Person identity operations

	#[instrument(skip(self, identity), fields(identity_id = %identity.id, person_id = %identity.person_id))]
	async fn create_identity(&self, identity: &PersonIdentity) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO analytics_person_identities (id, person_id, distinct_id, identity_type, created_at)
			VALUES (?, ?, ?, ?, ?)
			"#,
		)
		.bind(identity.id.0.to_string())
		.bind(identity.person_id.0.to_string())
		.bind(&identity.distinct_id)
		.bind(identity.identity_type.as_str())
		.bind(identity.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(org_id = %org_id, distinct_id = %distinct_id))]
	async fn get_identity_by_distinct_id(
		&self,
		org_id: OrgId,
		distinct_id: &str,
	) -> Result<Option<PersonIdentity>> {
		let row = sqlx::query_as::<_, PersonIdentityRow>(
			r#"
			SELECT pi.id, pi.person_id, pi.distinct_id, pi.identity_type, pi.created_at
			FROM analytics_person_identities pi
			JOIN analytics_persons p ON p.id = pi.person_id
			WHERE p.org_id = ? AND pi.distinct_id = ?
			"#,
		)
		.bind(org_id.0.to_string())
		.bind(distinct_id)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(person_id = %person_id))]
	async fn list_identities_for_person(&self, person_id: PersonId) -> Result<Vec<PersonIdentity>> {
		let rows = sqlx::query_as::<_, PersonIdentityRow>(
			r#"
			SELECT id, person_id, distinct_id, identity_type, created_at
			FROM analytics_person_identities
			WHERE person_id = ?
			ORDER BY created_at ASC
			"#,
		)
		.bind(person_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self), fields(from_person_id = %from_person_id, to_person_id = %to_person_id))]
	async fn transfer_identities(
		&self,
		from_person_id: PersonId,
		to_person_id: PersonId,
	) -> Result<u64> {
		let result = sqlx::query(
			r#"
			UPDATE analytics_person_identities
			SET person_id = ?
			WHERE person_id = ?
			"#,
		)
		.bind(to_person_id.0.to_string())
		.bind(from_person_id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected())
	}

	// Event operations

	#[instrument(skip(self, event), fields(event_id = %event.id, org_id = %event.org_id))]
	async fn insert_event(&self, event: &Event) -> Result<()> {
		let properties_json = serde_json::to_string(&event.properties)?;
		let ip_address = event.ip_address.as_ref().map(|s| s.expose().to_string());

		sqlx::query(
			r#"
			INSERT INTO analytics_events (id, org_id, person_id, distinct_id, event_name, properties,
				timestamp, ip_address, user_agent, lib, lib_version, created_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(event.id.0.to_string())
		.bind(event.org_id.0.to_string())
		.bind(event.person_id.map(|id| id.0.to_string()))
		.bind(&event.distinct_id)
		.bind(&event.event_name)
		.bind(properties_json)
		.bind(event.timestamp.to_rfc3339())
		.bind(ip_address)
		.bind(&event.user_agent)
		.bind(&event.lib)
		.bind(&event.lib_version)
		.bind(event.created_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self, events), fields(count = events.len()))]
	async fn insert_events(&self, events: &[Event]) -> Result<u64> {
		if events.is_empty() {
			return Ok(0);
		}

		let mut count = 0u64;
		for event in events {
			self.insert_event(event).await?;
			count += 1;
		}

		Ok(count)
	}

	#[instrument(skip(self), fields(event_id = %id))]
	async fn get_event_by_id(&self, id: EventId) -> Result<Option<Event>> {
		let row = sqlx::query_as::<_, EventRow>(
			r#"
			SELECT id, org_id, person_id, distinct_id, event_name, properties,
				timestamp, ip_address, user_agent, lib, lib_version, created_at
			FROM analytics_events
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id, distinct_id = ?distinct_id, event_name = ?event_name))]
	async fn list_events(
		&self,
		org_id: OrgId,
		distinct_id: Option<&str>,
		event_name: Option<&str>,
		start_time: Option<DateTime<Utc>>,
		end_time: Option<DateTime<Utc>>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<Event>> {
		let mut query = String::from(
			r#"
			SELECT id, org_id, person_id, distinct_id, event_name, properties,
				timestamp, ip_address, user_agent, lib, lib_version, created_at
			FROM analytics_events
			WHERE org_id = ?
			"#,
		);

		if distinct_id.is_some() {
			query.push_str(" AND distinct_id = ?");
		}
		if event_name.is_some() {
			query.push_str(" AND event_name = ?");
		}
		if start_time.is_some() {
			query.push_str(" AND timestamp >= ?");
		}
		if end_time.is_some() {
			query.push_str(" AND timestamp <= ?");
		}

		query.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

		let mut q = sqlx::query_as::<_, EventRow>(&query);
		q = q.bind(org_id.0.to_string());

		if let Some(did) = distinct_id {
			q = q.bind(did);
		}
		if let Some(name) = event_name {
			q = q.bind(name);
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

	#[instrument(skip(self), fields(org_id = %org_id, distinct_id = ?distinct_id, event_name = ?event_name))]
	async fn count_events(
		&self,
		org_id: OrgId,
		distinct_id: Option<&str>,
		event_name: Option<&str>,
		start_time: Option<DateTime<Utc>>,
		end_time: Option<DateTime<Utc>>,
	) -> Result<u64> {
		let mut query = String::from(
			r#"
			SELECT COUNT(*) FROM analytics_events
			WHERE org_id = ?
			"#,
		);

		if distinct_id.is_some() {
			query.push_str(" AND distinct_id = ?");
		}
		if event_name.is_some() {
			query.push_str(" AND event_name = ?");
		}
		if start_time.is_some() {
			query.push_str(" AND timestamp >= ?");
		}
		if end_time.is_some() {
			query.push_str(" AND timestamp <= ?");
		}

		let mut q = sqlx::query_as::<_, (i64,)>(&query);
		q = q.bind(org_id.0.to_string());

		if let Some(did) = distinct_id {
			q = q.bind(did);
		}
		if let Some(name) = event_name {
			q = q.bind(name);
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

	#[instrument(skip(self), fields(from_person_id = %from_person_id, to_person_id = %to_person_id))]
	async fn reassign_events(&self, from_person_id: PersonId, to_person_id: PersonId) -> Result<u64> {
		let result = sqlx::query(
			r#"
			UPDATE analytics_events
			SET person_id = ?
			WHERE person_id = ?
			"#,
		)
		.bind(to_person_id.0.to_string())
		.bind(from_person_id.0.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected())
	}

	// Person merge operations

	#[instrument(skip(self, merge), fields(merge_id = %merge.id, winner_id = %merge.winner_id, loser_id = %merge.loser_id))]
	async fn create_merge(&self, merge: &PersonMerge) -> Result<()> {
		let reason_json = serde_json::to_string(&merge.reason)?;

		sqlx::query(
			r#"
			INSERT INTO analytics_person_merges (id, winner_id, loser_id, reason, merged_at)
			VALUES (?, ?, ?, ?, ?)
			"#,
		)
		.bind(merge.id.0.to_string())
		.bind(merge.winner_id.0.to_string())
		.bind(merge.loser_id.0.to_string())
		.bind(reason_json)
		.bind(merge.merged_at.to_rfc3339())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(person_id = %person_id))]
	async fn list_merges_for_person(&self, person_id: PersonId) -> Result<Vec<PersonMerge>> {
		let rows = sqlx::query_as::<_, PersonMergeRow>(
			r#"
			SELECT id, winner_id, loser_id, reason, merged_at
			FROM analytics_person_merges
			WHERE winner_id = ? OR loser_id = ?
			ORDER BY merged_at DESC
			"#,
		)
		.bind(person_id.0.to_string())
		.bind(person_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	// API key operations

	#[instrument(skip(self, key), fields(key_id = %key.id, org_id = %key.org_id))]
	async fn create_api_key(&self, key: &AnalyticsApiKey) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO analytics_api_keys (id, org_id, name, key_type, key_hash, created_by,
				created_at, last_used_at, revoked_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(key.id.0.to_string())
		.bind(key.org_id.0.to_string())
		.bind(&key.name)
		.bind(key.key_type.as_str())
		.bind(&key.key_hash)
		.bind(key.created_by.0.to_string())
		.bind(key.created_at.to_rfc3339())
		.bind(key.last_used_at.map(|dt| dt.to_rfc3339()))
		.bind(key.revoked_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[instrument(skip(self), fields(key_id = %id))]
	async fn get_api_key_by_id(&self, id: AnalyticsApiKeyId) -> Result<Option<AnalyticsApiKey>> {
		let row = sqlx::query_as::<_, ApiKeyRow>(
			r#"
			SELECT id, org_id, name, key_type, key_hash, created_by,
				created_at, last_used_at, revoked_at
			FROM analytics_api_keys
			WHERE id = ?
			"#,
		)
		.bind(id.0.to_string())
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self, key_hash))]
	async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<AnalyticsApiKey>> {
		let row = sqlx::query_as::<_, ApiKeyRow>(
			r#"
			SELECT id, org_id, name, key_type, key_hash, created_by,
				created_at, last_used_at, revoked_at
			FROM analytics_api_keys
			WHERE key_hash = ?
			"#,
		)
		.bind(key_hash)
		.fetch_optional(&self.pool)
		.await?;

		row.map(TryInto::try_into).transpose()
	}

	#[instrument(skip(self), fields(org_id = %org_id))]
	async fn list_api_keys(&self, org_id: OrgId) -> Result<Vec<AnalyticsApiKey>> {
		let rows = sqlx::query_as::<_, ApiKeyRow>(
			r#"
			SELECT id, org_id, name, key_type, key_hash, created_by,
				created_at, last_used_at, revoked_at
			FROM analytics_api_keys
			WHERE org_id = ?
			ORDER BY created_at DESC
			"#,
		)
		.bind(org_id.0.to_string())
		.fetch_all(&self.pool)
		.await?;

		rows.into_iter().map(TryInto::try_into).collect()
	}

	#[instrument(skip(self), fields(key_id = %id))]
	async fn revoke_api_key(&self, id: AnalyticsApiKeyId) -> Result<bool> {
		let result = sqlx::query(
			r#"
			UPDATE analytics_api_keys
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

	#[instrument(skip(self), fields(key_id = %id))]
	async fn update_api_key_last_used(&self, id: AnalyticsApiKeyId) -> Result<()> {
		sqlx::query(
			r#"
			UPDATE analytics_api_keys
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

	#[instrument(skip(self, raw_key), fields(org_id = %org_id))]
	async fn find_api_key_by_verification(
		&self,
		raw_key: &str,
		org_id: OrgId,
	) -> Result<Option<AnalyticsApiKey>> {
		let keys = self.list_api_keys(org_id).await?;

		for key in keys {
			if key.revoked_at.is_some() {
				continue;
			}

			match crate::api_key::verify_api_key(raw_key, &key.key_hash) {
				Ok(true) => {
					tracing::debug!(api_key_id = %key.id, "API key verified successfully");
					return Ok(Some(key));
				}
				Ok(false) => continue,
				Err(e) => {
					tracing::warn!(api_key_id = %key.id, error = %e, "Failed to verify API key hash");
					continue;
				}
			}
		}

		Ok(None)
	}

	#[instrument(skip(self, raw_key))]
	async fn find_api_key_by_raw(&self, raw_key: &str) -> Result<Option<AnalyticsApiKey>> {
		// Fetch all non-revoked API keys
		let rows = sqlx::query_as::<_, ApiKeyRow>(
			r#"
			SELECT id, org_id, name, key_type, key_hash, created_by,
				created_at, last_used_at, revoked_at
			FROM analytics_api_keys
			WHERE revoked_at IS NULL
			"#,
		)
		.fetch_all(&self.pool)
		.await?;

		// Try to verify the raw key against each stored hash
		for row in rows {
			let key: AnalyticsApiKey = match row.try_into() {
				Ok(k) => k,
				Err(e) => {
					tracing::warn!(error = %e, "Failed to parse API key row");
					continue;
				}
			};

			match crate::api_key::verify_api_key(raw_key, &key.key_hash) {
				Ok(true) => {
					tracing::debug!(api_key_id = %key.id, org_id = %key.org_id, "API key verified successfully");
					return Ok(Some(key));
				}
				Ok(false) => continue,
				Err(e) => {
					tracing::warn!(api_key_id = %key.id, error = %e, "Failed to verify API key hash");
					continue;
				}
			}
		}

		Ok(None)
	}
}

// Database row types

#[derive(sqlx::FromRow)]
struct PersonRow {
	id: String,
	org_id: String,
	properties: String,
	created_at: String,
	updated_at: String,
	merged_into_id: Option<String>,
	merged_at: Option<String>,
}

impl TryFrom<PersonRow> for Person {
	type Error = AnalyticsServerError;

	fn try_from(row: PersonRow) -> Result<Self> {
		let properties: serde_json::Value = serde_json::from_str(&row.properties)?;

		Ok(Person {
			id: row
				.id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid person ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid org ID".to_string()))?,
			properties,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid updated_at".to_string()))?
				.with_timezone(&chrono::Utc),
			merged_into_id: row
				.merged_into_id
				.map(|s| {
					s.parse()
						.map_err(|_| AnalyticsServerError::Internal("Invalid merged_into_id".to_string()))
				})
				.transpose()?,
			merged_at: row
				.merged_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| AnalyticsServerError::Internal("Invalid merged_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
		})
	}
}

#[derive(sqlx::FromRow)]
struct PersonIdentityRow {
	id: String,
	person_id: String,
	distinct_id: String,
	identity_type: String,
	created_at: String,
}

impl TryFrom<PersonIdentityRow> for PersonIdentity {
	type Error = AnalyticsServerError;

	fn try_from(row: PersonIdentityRow) -> Result<Self> {
		let identity_type: IdentityType = row
			.identity_type
			.parse()
			.map_err(|_| AnalyticsServerError::Internal("Invalid identity type".to_string()))?;

		Ok(PersonIdentity {
			id: PersonIdentityId(
				row
					.id
					.parse()
					.map_err(|_| AnalyticsServerError::Internal("Invalid identity ID".to_string()))?,
			),
			person_id: row
				.person_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid person ID".to_string()))?,
			distinct_id: row.distinct_id,
			identity_type,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct EventRow {
	id: String,
	org_id: String,
	person_id: Option<String>,
	distinct_id: String,
	event_name: String,
	properties: String,
	timestamp: String,
	ip_address: Option<String>,
	user_agent: Option<String>,
	lib: Option<String>,
	lib_version: Option<String>,
	created_at: String,
}

impl TryFrom<EventRow> for Event {
	type Error = AnalyticsServerError;

	fn try_from(row: EventRow) -> Result<Self> {
		use loom_common_secret::SecretString;

		let properties: serde_json::Value = serde_json::from_str(&row.properties)?;

		Ok(Event {
			id: row
				.id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid event ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid org ID".to_string()))?,
			person_id: row
				.person_id
				.map(|s| {
					s.parse()
						.map_err(|_| AnalyticsServerError::Internal("Invalid person ID".to_string()))
				})
				.transpose()?,
			distinct_id: row.distinct_id,
			event_name: row.event_name,
			properties,
			timestamp: chrono::DateTime::parse_from_rfc3339(&row.timestamp)
				.map_err(|_| AnalyticsServerError::Internal("Invalid timestamp".to_string()))?
				.with_timezone(&chrono::Utc),
			ip_address: row.ip_address.map(SecretString::new),
			user_agent: row.user_agent,
			lib: row.lib,
			lib_version: row.lib_version,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct PersonMergeRow {
	id: String,
	winner_id: String,
	loser_id: String,
	reason: String,
	merged_at: String,
}

impl TryFrom<PersonMergeRow> for PersonMerge {
	type Error = AnalyticsServerError;

	fn try_from(row: PersonMergeRow) -> Result<Self> {
		let reason: MergeReason = serde_json::from_str(&row.reason)?;

		Ok(PersonMerge {
			id: PersonMergeId(
				row
					.id
					.parse()
					.map_err(|_| AnalyticsServerError::Internal("Invalid merge ID".to_string()))?,
			),
			winner_id: row
				.winner_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid winner ID".to_string()))?,
			loser_id: row
				.loser_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid loser ID".to_string()))?,
			reason,
			merged_at: chrono::DateTime::parse_from_rfc3339(&row.merged_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid merged_at".to_string()))?
				.with_timezone(&chrono::Utc),
		})
	}
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
	id: String,
	org_id: String,
	name: String,
	key_type: String,
	key_hash: String,
	created_by: String,
	created_at: String,
	last_used_at: Option<String>,
	revoked_at: Option<String>,
}

impl TryFrom<ApiKeyRow> for AnalyticsApiKey {
	type Error = AnalyticsServerError;

	fn try_from(row: ApiKeyRow) -> Result<Self> {
		let key_type: AnalyticsKeyType = row
			.key_type
			.parse()
			.map_err(|_| AnalyticsServerError::Internal("Invalid key type".to_string()))?;

		Ok(AnalyticsApiKey {
			id: row
				.id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid API key ID".to_string()))?,
			org_id: row
				.org_id
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid org ID".to_string()))?,
			name: row.name,
			key_type,
			key_hash: row.key_hash,
			created_by: row
				.created_by
				.parse()
				.map_err(|_| AnalyticsServerError::Internal("Invalid user ID".to_string()))?,
			created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
				.map_err(|_| AnalyticsServerError::Internal("Invalid created_at".to_string()))?
				.with_timezone(&chrono::Utc),
			last_used_at: row
				.last_used_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| AnalyticsServerError::Internal("Invalid last_used_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
			revoked_at: row
				.revoked_at
				.map(|s| {
					chrono::DateTime::parse_from_rfc3339(&s)
						.map_err(|_| AnalyticsServerError::Internal("Invalid revoked_at".to_string()))
						.map(|dt| dt.with_timezone(&chrono::Utc))
				})
				.transpose()?,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn person_row_conversion_with_null_merge() {
		let row = PersonRow {
			id: uuid::Uuid::new_v4().to_string(),
			org_id: uuid::Uuid::new_v4().to_string(),
			properties: "{}".to_string(),
			created_at: Utc::now().to_rfc3339(),
			updated_at: Utc::now().to_rfc3339(),
			merged_into_id: None,
			merged_at: None,
		};

		let person: Person = row.try_into().unwrap();
		assert!(person.merged_into_id.is_none());
		assert!(person.merged_at.is_none());
	}

	#[test]
	fn person_row_conversion_with_merge() {
		let winner_id = uuid::Uuid::new_v4();
		let row = PersonRow {
			id: uuid::Uuid::new_v4().to_string(),
			org_id: uuid::Uuid::new_v4().to_string(),
			properties: r#"{"name":"Alice"}"#.to_string(),
			created_at: Utc::now().to_rfc3339(),
			updated_at: Utc::now().to_rfc3339(),
			merged_into_id: Some(winner_id.to_string()),
			merged_at: Some(Utc::now().to_rfc3339()),
		};

		let person: Person = row.try_into().unwrap();
		assert!(person.merged_into_id.is_some());
		assert_eq!(person.merged_into_id.unwrap().0, winner_id);
	}

	#[test]
	fn identity_type_parsing() {
		assert_eq!(
			"anonymous".parse::<IdentityType>().unwrap(),
			IdentityType::Anonymous
		);
		assert_eq!(
			"identified".parse::<IdentityType>().unwrap(),
			IdentityType::Identified
		);
	}

	#[test]
	fn api_key_type_parsing() {
		assert_eq!(
			"write".parse::<AnalyticsKeyType>().unwrap(),
			AnalyticsKeyType::Write
		);
		assert_eq!(
			"read_write".parse::<AnalyticsKeyType>().unwrap(),
			AnalyticsKeyType::ReadWrite
		);
	}
}
