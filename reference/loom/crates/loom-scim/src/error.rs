// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScimError {
	#[error("invalid filter: {0}")]
	InvalidFilter(String),
	#[error("invalid value: {0}")]
	InvalidValue(String),
	#[error("resource not found: {0}")]
	NotFound(String),
	#[error("uniqueness violation: {0}")]
	Uniqueness(String),
	#[error("mutability violation: {0}")]
	Mutability(String),
	#[error("invalid syntax: {0}")]
	InvalidSyntax(String),
	#[error("invalid path: {0}")]
	InvalidPath(String),
	#[error("too many operations")]
	TooMany,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScimErrorType {
	InvalidFilter,
	TooMany,
	Uniqueness,
	Mutability,
	InvalidSyntax,
	InvalidPath,
	NoTarget,
	InvalidValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimErrorResponse {
	pub schemas: Vec<String>,
	pub status: String,
	pub scim_type: Option<ScimErrorType>,
	pub detail: String,
}

impl ScimErrorResponse {
	pub fn new(status: u16, error_type: Option<ScimErrorType>, detail: impl Into<String>) -> Self {
		Self {
			schemas: vec!["urn:ietf:params:scim:api:messages:2.0:Error".to_string()],
			status: status.to_string(),
			scim_type: error_type,
			detail: detail.into(),
		}
	}
}
