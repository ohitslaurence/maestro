// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! WireGuard tunnel repository for database operations.
//!
//! This module provides database access for WireGuard tunnel components:
//! - Weaver registration and management
//! - Device registration and management
//! - Session tracking
//! - IP allocation

use async_trait::async_trait;
use sqlx::sqlite::SqlitePool;
use std::net::Ipv6Addr;
use uuid::Uuid;

use crate::error::DbError;

pub type WeaverRowTuple = (
	String,
	Vec<u8>,
	String,
	Option<i64>,
	Option<String>,
	String,
	Option<String>,
);

pub type DeviceRowTuple = (
	String,
	String,
	Vec<u8>,
	Option<String>,
	String,
	Option<String>,
	Option<String>,
);

pub type SessionRowTuple = (String, String, String, String, String, Option<String>);

pub type IpAllocationRow = (String,);

/// Repository for WireGuard tunnel database operations.
#[derive(Clone)]
pub struct WgTunnelRepository {
	pool: SqlitePool,
}

impl WgTunnelRepository {
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	// =========================================================================
	// Weaver Operations
	// =========================================================================

	#[tracing::instrument(skip(self, public_key), fields(%weaver_id))]
	pub async fn insert_weaver(
		&self,
		weaver_id: Uuid,
		public_key: &[u8],
		assigned_ip: Ipv6Addr,
		derp_region: Option<u16>,
	) -> Result<(), DbError> {
		sqlx::query(
			"INSERT INTO wg_weavers (weaver_id, public_key, assigned_ip, derp_home_region, registered_at)
			 VALUES (?, ?, ?, ?, datetime('now'))",
		)
		.bind(weaver_id.to_string())
		.bind(public_key)
		.bind(assigned_ip.to_string())
		.bind(derp_region.map(|r| r as i64))
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(%weaver_id))]
	pub async fn get_weaver(&self, weaver_id: Uuid) -> Result<Option<WeaverRowTuple>, DbError> {
		let row: Option<WeaverRowTuple> = sqlx::query_as(
			"SELECT weaver_id, public_key, assigned_ip, derp_home_region, endpoint, registered_at, last_seen_at
			 FROM wg_weavers WHERE weaver_id = ?",
		)
		.bind(weaver_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self), fields(%weaver_id))]
	pub async fn delete_weaver(&self, weaver_id: Uuid) -> Result<u64, DbError> {
		let result = sqlx::query("DELETE FROM wg_weavers WHERE weaver_id = ?")
			.bind(weaver_id.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected())
	}

	#[tracing::instrument(skip(self), fields(%weaver_id, %endpoint))]
	pub async fn update_weaver_endpoint(
		&self,
		weaver_id: Uuid,
		endpoint: &str,
	) -> Result<u64, DbError> {
		let result = sqlx::query("UPDATE wg_weavers SET endpoint = ? WHERE weaver_id = ?")
			.bind(endpoint)
			.bind(weaver_id.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected())
	}

	#[tracing::instrument(skip(self), fields(%weaver_id))]
	pub async fn update_weaver_last_seen(&self, weaver_id: Uuid) -> Result<u64, DbError> {
		let result =
			sqlx::query("UPDATE wg_weavers SET last_seen_at = datetime('now') WHERE weaver_id = ?")
				.bind(weaver_id.to_string())
				.execute(&self.pool)
				.await?;

		Ok(result.rows_affected())
	}

	// =========================================================================
	// Device Operations
	// =========================================================================

	#[tracing::instrument(skip(self, public_key), fields(%id, %user_id))]
	pub async fn insert_device(
		&self,
		id: Uuid,
		user_id: Uuid,
		public_key: &[u8],
		name: Option<&str>,
	) -> Result<(), DbError> {
		sqlx::query(
			"INSERT INTO wg_devices (id, user_id, public_key, name, created_at)
			 VALUES (?, ?, ?, ?, datetime('now'))",
		)
		.bind(id.to_string())
		.bind(user_id.to_string())
		.bind(public_key)
		.bind(name)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(%user_id))]
	pub async fn list_devices_for_user(&self, user_id: Uuid) -> Result<Vec<DeviceRowTuple>, DbError> {
		let rows: Vec<DeviceRowTuple> = sqlx::query_as(
			"SELECT id, user_id, public_key, name, created_at, last_seen_at, revoked_at
			 FROM wg_devices WHERE user_id = ? AND revoked_at IS NULL
			 ORDER BY created_at DESC",
		)
		.bind(user_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		Ok(rows)
	}

	#[tracing::instrument(skip(self), fields(%id))]
	pub async fn get_device(&self, id: Uuid) -> Result<Option<DeviceRowTuple>, DbError> {
		let row: Option<DeviceRowTuple> = sqlx::query_as(
			"SELECT id, user_id, public_key, name, created_at, last_seen_at, revoked_at
			 FROM wg_devices WHERE id = ?",
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self, public_key))]
	pub async fn get_device_by_public_key(
		&self,
		public_key: &[u8],
	) -> Result<Option<DeviceRowTuple>, DbError> {
		let row: Option<DeviceRowTuple> = sqlx::query_as(
			"SELECT id, user_id, public_key, name, created_at, last_seen_at, revoked_at
			 FROM wg_devices WHERE public_key = ?",
		)
		.bind(public_key)
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self), fields(%id, %user_id))]
	pub async fn revoke_device(&self, id: Uuid, user_id: Uuid) -> Result<u64, DbError> {
		let result = sqlx::query(
			"UPDATE wg_devices SET revoked_at = datetime('now')
			 WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
		)
		.bind(id.to_string())
		.bind(user_id.to_string())
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected())
	}

	#[tracing::instrument(skip(self), fields(%id))]
	pub async fn update_device_last_seen(&self, id: Uuid) -> Result<u64, DbError> {
		let result = sqlx::query("UPDATE wg_devices SET last_seen_at = datetime('now') WHERE id = ?")
			.bind(id.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected())
	}

	// =========================================================================
	// Session Operations
	// =========================================================================

	#[tracing::instrument(skip(self), fields(%id, %device_id, %weaver_id))]
	pub async fn insert_session(
		&self,
		id: Uuid,
		device_id: Uuid,
		weaver_id: Uuid,
		client_ip: Ipv6Addr,
	) -> Result<(), DbError> {
		sqlx::query(
			"INSERT INTO wg_sessions (id, device_id, weaver_id, client_ip, created_at)
			 VALUES (?, ?, ?, ?, datetime('now'))",
		)
		.bind(id.to_string())
		.bind(device_id.to_string())
		.bind(weaver_id.to_string())
		.bind(client_ip.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(%device_id, %weaver_id))]
	pub async fn get_session_by_device_weaver(
		&self,
		device_id: Uuid,
		weaver_id: Uuid,
	) -> Result<Option<SessionRowTuple>, DbError> {
		let row: Option<SessionRowTuple> = sqlx::query_as(
			"SELECT id, device_id, weaver_id, client_ip, created_at, last_handshake_at
			 FROM wg_sessions WHERE device_id = ? AND weaver_id = ?",
		)
		.bind(device_id.to_string())
		.bind(weaver_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self), fields(%device_id))]
	pub async fn list_sessions_for_device(
		&self,
		device_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError> {
		let rows: Vec<SessionRowTuple> = sqlx::query_as(
			"SELECT id, device_id, weaver_id, client_ip, created_at, last_handshake_at
			 FROM wg_sessions WHERE device_id = ?
			 ORDER BY created_at DESC",
		)
		.bind(device_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		Ok(rows)
	}

	#[tracing::instrument(skip(self), fields(%weaver_id))]
	pub async fn list_sessions_for_weaver(
		&self,
		weaver_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError> {
		let rows: Vec<SessionRowTuple> = sqlx::query_as(
			"SELECT id, device_id, weaver_id, client_ip, created_at, last_handshake_at
			 FROM wg_sessions WHERE weaver_id = ?
			 ORDER BY created_at DESC",
		)
		.bind(weaver_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		Ok(rows)
	}

	#[tracing::instrument(skip(self), fields(%id))]
	pub async fn get_session(&self, id: Uuid) -> Result<Option<SessionRowTuple>, DbError> {
		let row: Option<SessionRowTuple> = sqlx::query_as(
			"SELECT id, device_id, weaver_id, client_ip, created_at, last_handshake_at
			 FROM wg_sessions WHERE id = ?",
		)
		.bind(id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self), fields(%id))]
	pub async fn delete_session(&self, id: Uuid) -> Result<u64, DbError> {
		let result = sqlx::query("DELETE FROM wg_sessions WHERE id = ?")
			.bind(id.to_string())
			.execute(&self.pool)
			.await?;

		Ok(result.rows_affected())
	}

	#[tracing::instrument(skip(self), fields(%id))]
	pub async fn update_session_handshake(&self, id: Uuid) -> Result<u64, DbError> {
		let result =
			sqlx::query("UPDATE wg_sessions SET last_handshake_at = datetime('now') WHERE id = ?")
				.bind(id.to_string())
				.execute(&self.pool)
				.await?;

		Ok(result.rows_affected())
	}

	// =========================================================================
	// IP Allocation Operations
	// =========================================================================

	#[tracing::instrument(skip(self))]
	pub async fn get_allocated_ips_by_type(
		&self,
		allocation_type: &str,
	) -> Result<Vec<IpAllocationRow>, DbError> {
		let rows: Vec<IpAllocationRow> = sqlx::query_as(
			"SELECT ip FROM wg_ip_allocations WHERE allocation_type = ? AND released_at IS NULL",
		)
		.bind(allocation_type)
		.fetch_all(&self.pool)
		.await?;

		Ok(rows)
	}

	#[tracing::instrument(skip(self), fields(%entity_id))]
	pub async fn get_allocation_for_entity(
		&self,
		entity_id: Uuid,
	) -> Result<Option<IpAllocationRow>, DbError> {
		let row: Option<IpAllocationRow> = sqlx::query_as(
			"SELECT ip FROM wg_ip_allocations WHERE entity_id = ? AND released_at IS NULL",
		)
		.bind(entity_id.to_string())
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	#[tracing::instrument(skip(self), fields(%ip, %entity_id))]
	pub async fn insert_ip_allocation(
		&self,
		ip: &str,
		allocation_type: &str,
		entity_id: Uuid,
	) -> Result<(), DbError> {
		sqlx::query(
			"INSERT INTO wg_ip_allocations (ip, allocation_type, entity_id, allocated_at)
			 VALUES (?, ?, ?, datetime('now'))",
		)
		.bind(ip)
		.bind(allocation_type)
		.bind(entity_id.to_string())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	#[tracing::instrument(skip(self), fields(%ip))]
	pub async fn release_ip(&self, ip: Ipv6Addr) -> Result<u64, DbError> {
		let result =
			sqlx::query("UPDATE wg_ip_allocations SET released_at = datetime('now') WHERE ip = ?")
				.bind(ip.to_string())
				.execute(&self.pool)
				.await?;

		Ok(result.rows_affected())
	}
}

#[async_trait]
pub trait WgTunnelStore: Send + Sync {
	async fn insert_weaver(
		&self,
		weaver_id: Uuid,
		public_key: &[u8],
		assigned_ip: Ipv6Addr,
		derp_region: Option<u16>,
	) -> Result<(), DbError>;
	async fn get_weaver(&self, weaver_id: Uuid) -> Result<Option<WeaverRowTuple>, DbError>;
	async fn delete_weaver(&self, weaver_id: Uuid) -> Result<u64, DbError>;
	async fn update_weaver_endpoint(&self, weaver_id: Uuid, endpoint: &str) -> Result<u64, DbError>;
	async fn update_weaver_last_seen(&self, weaver_id: Uuid) -> Result<u64, DbError>;
	async fn insert_device(
		&self,
		id: Uuid,
		user_id: Uuid,
		public_key: &[u8],
		name: Option<&str>,
	) -> Result<(), DbError>;
	async fn list_devices_for_user(&self, user_id: Uuid) -> Result<Vec<DeviceRowTuple>, DbError>;
	async fn get_device(&self, id: Uuid) -> Result<Option<DeviceRowTuple>, DbError>;
	async fn get_device_by_public_key(
		&self,
		public_key: &[u8],
	) -> Result<Option<DeviceRowTuple>, DbError>;
	async fn revoke_device(&self, id: Uuid, user_id: Uuid) -> Result<u64, DbError>;
	async fn update_device_last_seen(&self, id: Uuid) -> Result<u64, DbError>;
	async fn insert_session(
		&self,
		id: Uuid,
		device_id: Uuid,
		weaver_id: Uuid,
		client_ip: Ipv6Addr,
	) -> Result<(), DbError>;
	async fn get_session_by_device_weaver(
		&self,
		device_id: Uuid,
		weaver_id: Uuid,
	) -> Result<Option<SessionRowTuple>, DbError>;
	async fn list_sessions_for_device(
		&self,
		device_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError>;
	async fn list_sessions_for_weaver(
		&self,
		weaver_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError>;
	async fn get_session(&self, id: Uuid) -> Result<Option<SessionRowTuple>, DbError>;
	async fn delete_session(&self, id: Uuid) -> Result<u64, DbError>;
	async fn update_session_handshake(&self, id: Uuid) -> Result<u64, DbError>;
	async fn get_allocated_ips_by_type(
		&self,
		allocation_type: &str,
	) -> Result<Vec<IpAllocationRow>, DbError>;
	async fn get_allocation_for_entity(
		&self,
		entity_id: Uuid,
	) -> Result<Option<IpAllocationRow>, DbError>;
	async fn insert_ip_allocation(
		&self,
		ip: &str,
		allocation_type: &str,
		entity_id: Uuid,
	) -> Result<(), DbError>;
	async fn release_ip(&self, ip: Ipv6Addr) -> Result<u64, DbError>;
}

#[async_trait]
impl WgTunnelStore for WgTunnelRepository {
	async fn insert_weaver(
		&self,
		weaver_id: Uuid,
		public_key: &[u8],
		assigned_ip: Ipv6Addr,
		derp_region: Option<u16>,
	) -> Result<(), DbError> {
		self
			.insert_weaver(weaver_id, public_key, assigned_ip, derp_region)
			.await
	}

	async fn get_weaver(&self, weaver_id: Uuid) -> Result<Option<WeaverRowTuple>, DbError> {
		self.get_weaver(weaver_id).await
	}

	async fn delete_weaver(&self, weaver_id: Uuid) -> Result<u64, DbError> {
		self.delete_weaver(weaver_id).await
	}

	async fn update_weaver_endpoint(&self, weaver_id: Uuid, endpoint: &str) -> Result<u64, DbError> {
		self.update_weaver_endpoint(weaver_id, endpoint).await
	}

	async fn update_weaver_last_seen(&self, weaver_id: Uuid) -> Result<u64, DbError> {
		self.update_weaver_last_seen(weaver_id).await
	}

	async fn insert_device(
		&self,
		id: Uuid,
		user_id: Uuid,
		public_key: &[u8],
		name: Option<&str>,
	) -> Result<(), DbError> {
		self.insert_device(id, user_id, public_key, name).await
	}

	async fn list_devices_for_user(&self, user_id: Uuid) -> Result<Vec<DeviceRowTuple>, DbError> {
		self.list_devices_for_user(user_id).await
	}

	async fn get_device(&self, id: Uuid) -> Result<Option<DeviceRowTuple>, DbError> {
		self.get_device(id).await
	}

	async fn get_device_by_public_key(
		&self,
		public_key: &[u8],
	) -> Result<Option<DeviceRowTuple>, DbError> {
		self.get_device_by_public_key(public_key).await
	}

	async fn revoke_device(&self, id: Uuid, user_id: Uuid) -> Result<u64, DbError> {
		self.revoke_device(id, user_id).await
	}

	async fn update_device_last_seen(&self, id: Uuid) -> Result<u64, DbError> {
		self.update_device_last_seen(id).await
	}

	async fn insert_session(
		&self,
		id: Uuid,
		device_id: Uuid,
		weaver_id: Uuid,
		client_ip: Ipv6Addr,
	) -> Result<(), DbError> {
		self
			.insert_session(id, device_id, weaver_id, client_ip)
			.await
	}

	async fn get_session_by_device_weaver(
		&self,
		device_id: Uuid,
		weaver_id: Uuid,
	) -> Result<Option<SessionRowTuple>, DbError> {
		self
			.get_session_by_device_weaver(device_id, weaver_id)
			.await
	}

	async fn list_sessions_for_device(
		&self,
		device_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError> {
		self.list_sessions_for_device(device_id).await
	}

	async fn list_sessions_for_weaver(
		&self,
		weaver_id: Uuid,
	) -> Result<Vec<SessionRowTuple>, DbError> {
		self.list_sessions_for_weaver(weaver_id).await
	}

	async fn get_session(&self, id: Uuid) -> Result<Option<SessionRowTuple>, DbError> {
		self.get_session(id).await
	}

	async fn delete_session(&self, id: Uuid) -> Result<u64, DbError> {
		self.delete_session(id).await
	}

	async fn update_session_handshake(&self, id: Uuid) -> Result<u64, DbError> {
		self.update_session_handshake(id).await
	}

	async fn get_allocated_ips_by_type(
		&self,
		allocation_type: &str,
	) -> Result<Vec<IpAllocationRow>, DbError> {
		self.get_allocated_ips_by_type(allocation_type).await
	}

	async fn get_allocation_for_entity(
		&self,
		entity_id: Uuid,
	) -> Result<Option<IpAllocationRow>, DbError> {
		self.get_allocation_for_entity(entity_id).await
	}

	async fn insert_ip_allocation(
		&self,
		ip: &str,
		allocation_type: &str,
		entity_id: Uuid,
	) -> Result<(), DbError> {
		self
			.insert_ip_allocation(ip, allocation_type, entity_id)
			.await
	}

	async fn release_ip(&self, ip: Ipv6Addr) -> Result<u64, DbError> {
		self.release_ip(ip).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::str::FromStr;

	async fn create_wgtunnel_test_pool() -> SqlitePool {
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
			CREATE TABLE IF NOT EXISTS wg_weavers (
				weaver_id TEXT PRIMARY KEY,
				public_key BLOB NOT NULL,
				assigned_ip TEXT NOT NULL,
				derp_home_region INTEGER,
				endpoint TEXT,
				registered_at TEXT NOT NULL,
				last_seen_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS wg_devices (
				id TEXT PRIMARY KEY,
				user_id TEXT NOT NULL,
				public_key BLOB NOT NULL,
				name TEXT,
				created_at TEXT NOT NULL,
				last_seen_at TEXT,
				revoked_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS wg_sessions (
				id TEXT PRIMARY KEY,
				device_id TEXT NOT NULL,
				weaver_id TEXT NOT NULL,
				client_ip TEXT NOT NULL,
				created_at TEXT NOT NULL,
				last_handshake_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS wg_ip_allocations (
				ip TEXT PRIMARY KEY,
				allocation_type TEXT NOT NULL,
				entity_id TEXT NOT NULL,
				allocated_at TEXT NOT NULL,
				released_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> WgTunnelRepository {
		let pool = create_wgtunnel_test_pool().await;
		WgTunnelRepository::new(pool)
	}

	#[tokio::test]
	async fn test_create_and_get_weaver() {
		let repo = make_repo().await;
		let weaver_id = Uuid::new_v4();
		let public_key = vec![0x01, 0x02, 0x03, 0x04, 0x05];
		let assigned_ip: Ipv6Addr = "fd00::1".parse().unwrap();
		let derp_region = Some(1u16);

		repo
			.insert_weaver(weaver_id, &public_key, assigned_ip, derp_region)
			.await
			.unwrap();

		let weaver = repo.get_weaver(weaver_id).await.unwrap();
		assert!(weaver.is_some());
		let (id, pk, ip, derp, _endpoint, _registered_at, _last_seen) = weaver.unwrap();
		assert_eq!(id, weaver_id.to_string());
		assert_eq!(pk, public_key);
		assert_eq!(ip, assigned_ip.to_string());
		assert_eq!(derp, Some(1i64));
	}

	#[tokio::test]
	async fn test_get_weaver_not_found() {
		let repo = make_repo().await;
		let nonexistent_id = Uuid::new_v4();

		let result = repo.get_weaver(nonexistent_id).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_create_and_get_device() {
		let repo = make_repo().await;
		let device_id = Uuid::new_v4();
		let user_id = Uuid::new_v4();
		let public_key = vec![0xaa, 0xbb, 0xcc, 0xdd];
		let device_name = Some("test-device");

		repo
			.insert_device(device_id, user_id, &public_key, device_name)
			.await
			.unwrap();

		let device = repo.get_device(device_id).await.unwrap();
		assert!(device.is_some());
		let (id, uid, pk, name, _created_at, _last_seen, _revoked_at) = device.unwrap();
		assert_eq!(id, device_id.to_string());
		assert_eq!(uid, user_id.to_string());
		assert_eq!(pk, public_key);
		assert_eq!(name, Some("test-device".to_string()));
	}

	#[tokio::test]
	async fn test_allocate_ip() {
		let repo = make_repo().await;
		let entity_id = Uuid::new_v4();
		let ip = "fd00::100";
		let allocation_type = "weaver";

		repo
			.insert_ip_allocation(ip, allocation_type, entity_id)
			.await
			.unwrap();

		let allocation = repo.get_allocation_for_entity(entity_id).await.unwrap();
		assert!(allocation.is_some());
		let (allocated_ip,) = allocation.unwrap();
		assert_eq!(allocated_ip, ip);

		let all_ips = repo
			.get_allocated_ips_by_type(allocation_type)
			.await
			.unwrap();
		assert_eq!(all_ips.len(), 1);
		assert_eq!(all_ips[0].0, ip);
	}
}
