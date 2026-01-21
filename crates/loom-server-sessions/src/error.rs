// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the sessions server.

use thiserror::Error;

/// Errors that can occur in the sessions server.
#[derive(Debug, Error)]
pub enum SessionsServerError {
	/// Database error
	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	/// Session not found
	#[error("session not found: {0}")]
	SessionNotFound(String),

	/// Project not found
	#[error("project not found: {0}")]
	ProjectNotFound(String),

	/// Invalid session data
	#[error("invalid session data: {0}")]
	InvalidData(String),

	/// JSON serialization error
	#[error("json error: {0}")]
	Json(#[from] serde_json::Error),

	/// Core error
	#[error("sessions core error: {0}")]
	Core(#[from] loom_sessions_core::SessionsError),
}

/// Result type for sessions server operations.
pub type Result<T> = std::result::Result<T, SessionsServerError>;
