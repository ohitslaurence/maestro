// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for crons server operations.

use thiserror::Error;

/// Result type for crons server operations.
pub type Result<T> = std::result::Result<T, CronsServerError>;

/// Errors that can occur in crons server operations.
#[derive(Debug, Error)]
pub enum CronsServerError {
	#[error("monitor not found")]
	MonitorNotFound,

	#[error("check-in not found")]
	CheckInNotFound,

	#[error("invalid slug: {0}")]
	InvalidSlug(String),

	#[error("invalid cron expression: {0}")]
	InvalidCronExpression(String),

	#[error("invalid timezone: {0}")]
	InvalidTimezone(String),

	#[error("duplicate monitor slug")]
	DuplicateSlug,

	#[error("invalid ping key")]
	InvalidPingKey,

	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("internal error: {0}")]
	Internal(String),
}
