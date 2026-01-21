// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration error types.

use std::path::PathBuf;

/// Errors that can occur during configuration loading and validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	/// I/O error reading config file
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	/// TOML parsing error
	#[error("TOML parse error in {path}: {source}")]
	TomlParse {
		path: PathBuf,
		#[source]
		source: toml::de::Error,
	},

	/// Environment variable error
	#[error("Environment error: {0}")]
	Env(String),

	/// Validation error
	#[error("Validation error: {0}")]
	Validation(String),

	/// Missing required field
	#[error("Missing required field: {0}")]
	MissingField(String),

	/// Invalid value
	#[error("Invalid value for {field}: {message}")]
	InvalidValue { field: String, message: String },

	/// Provider not found
	#[error("Provider '{0}' not found in configuration")]
	ProviderNotFound(String),

	/// Home directory not found
	#[error("Could not determine home directory")]
	HomeDirNotFound,
}

impl ConfigError {
	/// Create a validation error
	pub fn validation(msg: impl Into<String>) -> Self {
		Self::Validation(msg.into())
	}

	/// Create a missing field error
	pub fn missing_field(field: impl Into<String>) -> Self {
		Self::MissingField(field.into())
	}

	/// Create an invalid value error
	pub fn invalid_value(field: impl Into<String>, message: impl Into<String>) -> Self {
		Self::InvalidValue {
			field: field.into(),
			message: message.into(),
		}
	}
}
