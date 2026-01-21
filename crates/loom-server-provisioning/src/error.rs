// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_server_db::DbError;

/// Errors that can occur during user provisioning.
#[derive(Debug, thiserror::Error)]
pub enum ProvisioningError {
	#[error("database error: {0}")]
	Database(#[from] DbError),

	#[error("user not found: {0}")]
	UserNotFound(String),

	#[error("invalid request: {0}")]
	InvalidRequest(String),

	#[error("signups are disabled and user does not exist")]
	SignupsDisabled,
}
