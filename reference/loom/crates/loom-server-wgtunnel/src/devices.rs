// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{Result, WgError};
use base64::prelude::*;
use chrono::{DateTime, Utc};
use loom_server_db::WgTunnelRepository;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
	pub id: Uuid,
	pub user_id: Uuid,
	pub public_key: [u8; 32],
	pub name: Option<String>,
	pub created_at: DateTime<Utc>,
	pub last_seen_at: Option<DateTime<Utc>>,
	pub revoked_at: Option<DateTime<Utc>>,
}

impl Device {
	pub fn public_key_base64(&self) -> String {
		BASE64_STANDARD.encode(self.public_key)
	}
}

#[derive(Debug, Clone)]
struct DeviceRow {
	id: String,
	user_id: String,
	public_key: Vec<u8>,
	name: Option<String>,
	created_at: String,
	last_seen_at: Option<String>,
	revoked_at: Option<String>,
}

impl TryFrom<DeviceRow> for Device {
	type Error = WgError;

	fn try_from(row: DeviceRow) -> Result<Self> {
		let public_key: [u8; 32] = row
			.public_key
			.try_into()
			.map_err(|_| WgError::InvalidPublicKey("invalid key length".to_string()))?;

		Ok(Device {
			id: row
				.id
				.parse()
				.map_err(|_| WgError::Internal("invalid device id".to_string()))?,
			user_id: row
				.user_id
				.parse()
				.map_err(|_| WgError::Internal("invalid user id".to_string()))?,
			public_key,
			name: row.name,
			created_at: parse_datetime(&row.created_at)?,
			last_seen_at: row
				.last_seen_at
				.as_ref()
				.map(|s| parse_datetime(s))
				.transpose()?,
			revoked_at: row
				.revoked_at
				.as_ref()
				.map(|s| parse_datetime(s))
				.transpose()?,
		})
	}
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
	DateTime::parse_from_rfc3339(s)
		.map(|dt| dt.with_timezone(&Utc))
		.or_else(|_| {
			chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
				.map(|ndt| ndt.and_utc())
				.map_err(|_| WgError::Internal(format!("invalid datetime: {s}")))
		})
}

#[derive(Clone)]
pub struct DeviceService {
	repo: WgTunnelRepository,
}

impl DeviceService {
	pub fn new(repo: WgTunnelRepository) -> Self {
		Self { repo }
	}

	#[instrument(skip(self, public_key), fields(%user_id, name = ?name))]
	pub async fn register(
		&self,
		user_id: Uuid,
		public_key: [u8; 32],
		name: Option<String>,
	) -> Result<Device> {
		let existing = self.get_by_public_key(&public_key).await?;
		if existing.is_some() {
			return Err(WgError::DeviceAlreadyExists);
		}

		let id = Uuid::new_v4();
		let now = Utc::now();

		self
			.repo
			.insert_device(id, user_id, public_key.as_slice(), name.as_deref())
			.await?;

		Ok(Device {
			id,
			user_id,
			public_key,
			name,
			created_at: now,
			last_seen_at: None,
			revoked_at: None,
		})
	}

	#[instrument(skip(self), fields(%user_id))]
	pub async fn list(&self, user_id: Uuid) -> Result<Vec<Device>> {
		let rows = self.repo.list_devices_for_user(user_id).await?;

		rows
			.into_iter()
			.map(
				|(id, user_id, public_key, name, created_at, last_seen_at, revoked_at)| {
					DeviceRow {
						id,
						user_id,
						public_key,
						name,
						created_at,
						last_seen_at,
						revoked_at,
					}
					.try_into()
				},
			)
			.collect()
	}

	#[instrument(skip(self), fields(%id))]
	pub async fn get(&self, id: Uuid) -> Result<Option<Device>> {
		let row = self.repo.get_device(id).await?;

		match row {
			Some((id, user_id, public_key, name, created_at, last_seen_at, revoked_at)) => {
				let device = DeviceRow {
					id,
					user_id,
					public_key,
					name,
					created_at,
					last_seen_at,
					revoked_at,
				}
				.try_into()?;
				Ok(Some(device))
			}
			None => Ok(None),
		}
	}

	#[instrument(skip(self, public_key))]
	pub async fn get_by_public_key(&self, public_key: &[u8; 32]) -> Result<Option<Device>> {
		let row = self
			.repo
			.get_device_by_public_key(public_key.as_slice())
			.await?;

		match row {
			Some((id, user_id, public_key, name, created_at, last_seen_at, revoked_at)) => {
				let device = DeviceRow {
					id,
					user_id,
					public_key,
					name,
					created_at,
					last_seen_at,
					revoked_at,
				}
				.try_into()?;
				Ok(Some(device))
			}
			None => Ok(None),
		}
	}

	#[instrument(skip(self), fields(%id, %user_id))]
	pub async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<()> {
		let rows_affected = self.repo.revoke_device(id, user_id).await?;

		if rows_affected == 0 {
			return Err(WgError::DeviceNotFound);
		}

		Ok(())
	}

	#[instrument(skip(self), fields(%id))]
	pub async fn update_last_seen(&self, id: Uuid) -> Result<()> {
		self.repo.update_device_last_seen(id).await?;

		Ok(())
	}
}
