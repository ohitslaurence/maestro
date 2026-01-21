// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

#[derive(Error, Debug)]
pub enum WgError {
	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("device not found")]
	DeviceNotFound,

	#[error("weaver not found")]
	WeaverNotFound,

	#[error("session not found")]
	SessionNotFound,

	#[error("device already exists")]
	DeviceAlreadyExists,

	#[error("device revoked")]
	DeviceRevoked,

	#[error("weaver already registered")]
	WeaverAlreadyRegistered,

	#[error("session already exists for device-weaver pair")]
	SessionAlreadyExists,

	#[error("invalid public key: {0}")]
	InvalidPublicKey(String),

	#[error("IP allocation failed: {0}")]
	IpAllocation(String),

	#[error("configuration error: {0}")]
	Config(String),

	#[error("DERP map error: {0}")]
	DerpMap(String),

	#[error("unauthorized: {0}")]
	Unauthorized(String),

	#[error("internal error: {0}")]
	Internal(String),
}

pub type Result<T> = std::result::Result<T, WgError>;

impl From<loom_wgtunnel_common::KeyError> for WgError {
	fn from(e: loom_wgtunnel_common::KeyError) -> Self {
		WgError::InvalidPublicKey(e.to_string())
	}
}

impl From<loom_wgtunnel_common::ip::IpError> for WgError {
	fn from(e: loom_wgtunnel_common::ip::IpError) -> Self {
		WgError::IpAllocation(e.to_string())
	}
}

impl From<loom_server_db::DbError> for WgError {
	fn from(e: loom_server_db::DbError) -> Self {
		match e {
			loom_server_db::DbError::Sqlx(sqlx_err) => WgError::Database(sqlx_err),
			loom_server_db::DbError::NotFound(msg) => WgError::Internal(format!("not found: {msg}")),
			loom_server_db::DbError::Conflict(msg) => WgError::Internal(format!("conflict: {msg}")),
			loom_server_db::DbError::Internal(msg) => WgError::Internal(msg),
			loom_server_db::DbError::Serialization(err) => {
				WgError::Internal(format!("serialization: {err}"))
			}
		}
	}
}
