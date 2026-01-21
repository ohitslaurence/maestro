// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the analytics server.

use thiserror::Error;

/// Errors that can occur in the analytics server.
#[derive(Debug, Error)]
pub enum AnalyticsServerError {
	#[error(transparent)]
	Core(#[from] loom_analytics_core::AnalyticsError),

	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("api key verification failed")]
	ApiKeyVerification,

	#[error("unauthorized: {0}")]
	Unauthorized(String),

	#[error("forbidden: {0}")]
	Forbidden(String),

	#[error("not found: {0}")]
	NotFound(String),

	#[error("internal error: {0}")]
	Internal(String),
}

/// A specialized `Result` type for analytics server operations.
pub type Result<T> = std::result::Result<T, AnalyticsServerError>;
