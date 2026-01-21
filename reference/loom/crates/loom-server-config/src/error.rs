// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration error types.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	#[error("Missing required environment variable: {0}")]
	MissingEnvVar(String),

	#[error("Invalid value for {key}: {message}")]
	InvalidValue { key: String, message: String },

	#[error("Failed to parse TOML config at {path}: {source}")]
	TomlParse {
		path: PathBuf,
		#[source]
		source: toml::de::Error,
	},

	#[error("Failed to read config file {path}: {source}")]
	FileRead {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},

	#[error("Validation error: {0}")]
	Validation(String),

	#[error("Secret loading error: {0}")]
	Secret(String),
}

impl From<std::io::Error> for ConfigError {
	fn from(e: std::io::Error) -> Self {
		ConfigError::FileRead {
			path: PathBuf::new(),
			source: e,
		}
	}
}
