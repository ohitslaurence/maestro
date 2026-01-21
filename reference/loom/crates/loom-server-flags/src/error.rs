// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use thiserror::Error;

/// Errors specific to the feature flags server.
#[derive(Debug, Error)]
pub enum FlagsServerError {
	#[error(transparent)]
	Core(#[from] loom_flags_core::FlagsError),

	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("sdk key verification failed")]
	SdkKeyVerification,

	#[error("environment mismatch: expected {expected}, got {actual}")]
	EnvironmentMismatch { expected: String, actual: String },

	#[error("unauthorized: {0}")]
	Unauthorized(String),

	#[error("forbidden: {0}")]
	Forbidden(String),

	#[error("internal error: {0}")]
	Internal(String),
}

pub type Result<T> = std::result::Result<T, FlagsServerError>;
