// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#[derive(Debug, thiserror::Error)]
pub enum DbError {
	#[error("Database error: {0}")]
	Sqlx(#[from] sqlx::Error),

	#[error("Not found: {0}")]
	NotFound(String),

	#[error("Conflict: {0}")]
	Conflict(String),

	#[error("Internal: {0}")]
	Internal(String),

	#[error("Serialization error: {0}")]
	Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;
