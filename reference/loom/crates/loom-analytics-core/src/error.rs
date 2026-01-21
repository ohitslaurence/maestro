// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the analytics system.

use thiserror::Error;

/// Errors that can occur in the analytics system.
///
/// These errors cover validation failures, lookup failures, permission issues,
/// and infrastructure errors (database, serialization).
#[derive(Debug, Error)]
pub enum AnalyticsError {
	#[error("person not found: {0}")]
	PersonNotFound(String),

	#[error("event not found: {0}")]
	EventNotFound(String),

	#[error("api key not found")]
	ApiKeyNotFound,

	#[error("api key revoked")]
	ApiKeyRevoked,

	#[error("invalid api key format")]
	InvalidApiKeyFormat,

	#[error("invalid distinct_id: {0}")]
	InvalidDistinctId(String),

	#[error("invalid event name: {0}")]
	InvalidEventName(String),

	#[error("properties too large: {0} bytes (max {1})")]
	PropertiesTooLarge(usize, usize),

	#[error("insufficient permissions: {0}")]
	InsufficientPermissions(String),

	#[error("database error: {0}")]
	Database(String),

	#[error("serialization error: {0}")]
	Serialization(String),

	#[error("internal error: {0}")]
	Internal(String),
}

impl From<serde_json::Error> for AnalyticsError {
	fn from(err: serde_json::Error) -> Self {
		AnalyticsError::Serialization(err.to_string())
	}
}

/// A specialized `Result` type for analytics operations.
pub type Result<T> = std::result::Result<T, AnalyticsError>;
