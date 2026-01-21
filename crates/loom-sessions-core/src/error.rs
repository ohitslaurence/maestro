// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the sessions system.

use thiserror::Error;

/// Errors that can occur in the sessions system.
#[derive(Debug, Error)]
pub enum SessionsError {
	/// Invalid session status string
	#[error("invalid session status: {0}")]
	InvalidStatus(String),

	/// Invalid platform string
	#[error("invalid platform: {0}")]
	InvalidPlatform(String),

	/// Session not found
	#[error("session not found: {0}")]
	NotFound(String),

	/// Project not found
	#[error("project not found: {0}")]
	ProjectNotFound(String),

	/// Invalid session ID
	#[error("invalid session ID: {0}")]
	InvalidSessionId(String),

	/// Database error
	#[error("database error: {0}")]
	Database(String),

	/// Serialization error
	#[error("serialization error: {0}")]
	Serialization(String),
}
