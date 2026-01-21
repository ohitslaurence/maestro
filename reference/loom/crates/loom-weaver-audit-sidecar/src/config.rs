// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
	#[error("missing required environment variable: {0}")]
	MissingEnvVar(String),

	#[error("invalid value for {name}: {message}")]
	InvalidValue { name: String, message: String },
}

pub type Result<T> = std::result::Result<T, ConfigError>;

#[derive(Debug, Clone)]
pub struct Config {
	pub weaver_id: String,
	pub org_id: String,
	pub owner_user_id: String,
	pub pod_name: String,
	pub pod_namespace: String,
	pub server_url: String,
	pub batch_interval: Duration,
	pub buffer_max_bytes: u64,
	pub metrics_port: u16,
	pub health_port: u16,
	pub buffer_path: PathBuf,
	pub sa_token_path: String,
	pub allow_no_auth: bool,
	#[allow(dead_code)] // Used in from_env() validation, kept for runtime access
	pub allow_insecure_http: bool,
}

impl Config {
	pub fn from_env() -> Result<Self> {
		let weaver_id = require_env("LOOM_WEAVER_ID")?;
		let org_id = require_env("LOOM_ORG_ID")?;
		let owner_user_id = require_env("LOOM_OWNER_USER_ID")?;
		let pod_name = require_env("LOOM_POD_NAME")?;
		let pod_namespace = require_env("LOOM_POD_NAMESPACE")?;
		let server_url = require_env("LOOM_SERVER_URL")?;

		let batch_interval_ms: u64 = optional_env_parse("LOOM_AUDIT_BATCH_INTERVAL_MS", 100)?;
		let buffer_max_bytes: u64 =
			optional_env_parse("LOOM_AUDIT_BUFFER_MAX_BYTES", 256 * 1024 * 1024)?;
		let metrics_port: u16 = optional_env_parse("LOOM_AUDIT_METRICS_PORT", 9090)?;
		let health_port: u16 = optional_env_parse("LOOM_AUDIT_HEALTH_PORT", 9091)?;
		let buffer_path_raw = optional_env(
			"LOOM_AUDIT_BUFFER_PATH",
			"/tmp/audit-buffer.jsonl".to_string(),
		);
		let buffer_path = validate_buffer_path(&buffer_path_raw)?;
		let sa_token_path = optional_env(
			"LOOM_SA_TOKEN_PATH",
			"/var/run/secrets/kubernetes.io/serviceaccount/token".to_string(),
		);

		let allow_no_auth = std::env::var("LOOM_AUDIT_ALLOW_NO_AUTH")
			.map(|v| v == "true" || v == "1")
			.unwrap_or(false);

		let allow_insecure_http = std::env::var("LOOM_AUDIT_ALLOW_INSECURE_HTTP")
			.map(|v| v == "true" || v == "1")
			.unwrap_or(false);

		if !allow_insecure_http && !server_url.starts_with("https://") {
			return Err(ConfigError::InvalidValue {
				name: "LOOM_SERVER_URL".into(),
				message: "must start with https:// (set LOOM_AUDIT_ALLOW_INSECURE_HTTP=true to allow HTTP)"
					.into(),
			});
		}

		Ok(Config {
			weaver_id,
			org_id,
			owner_user_id,
			pod_name,
			pod_namespace,
			server_url,
			batch_interval: Duration::from_millis(batch_interval_ms),
			buffer_max_bytes,
			metrics_port,
			health_port,
			buffer_path,
			sa_token_path,
			allow_no_auth,
			allow_insecure_http,
		})
	}
}

fn require_env(name: &str) -> Result<String> {
	std::env::var(name).map_err(|_| ConfigError::MissingEnvVar(name.to_string()))
}

fn optional_env(name: &str, default: String) -> String {
	std::env::var(name).unwrap_or(default)
}

fn optional_env_parse<T: std::str::FromStr>(name: &str, default: T) -> Result<T>
where
	T::Err: std::fmt::Display,
{
	match std::env::var(name) {
		Ok(val) => val.parse().map_err(|e: T::Err| ConfigError::InvalidValue {
			name: name.to_string(),
			message: e.to_string(),
		}),
		Err(_) => Ok(default),
	}
}

fn validate_buffer_path(path: &str) -> Result<PathBuf> {
	let path = PathBuf::from(path);

	// Canonicalize if the path exists, otherwise use the path as-is
	// (it will be created later)
	let check_path = if path.exists() {
		path.canonicalize().unwrap_or_else(|_| path.clone())
	} else {
		// Check parent directory exists and is safe
		if let Some(parent) = path.parent() {
			if parent.exists() {
				parent
					.canonicalize()
					.map(|p| p.join(path.file_name().unwrap_or_default()))
					.unwrap_or_else(|_| path.clone())
			} else {
				path.clone()
			}
		} else {
			path.clone()
		}
	};

	let allowed_prefixes = [
		Path::new("/var/lib/loom/audit"),
		Path::new("/tmp/loom-audit"),
		Path::new("/tmp"), // Allow /tmp for backward compatibility
	];

	let is_safe = allowed_prefixes
		.iter()
		.any(|prefix| check_path.starts_with(prefix));

	if !is_safe {
		return Err(ConfigError::InvalidValue {
			name: "LOOM_AUDIT_BUFFER_PATH".into(),
			message: format!(
				"must be under one of: {:?}",
				allowed_prefixes
					.iter()
					.map(|p| p.display())
					.collect::<Vec<_>>()
			),
		});
	}

	Ok(path)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_optional_env_parse_uses_default() {
		let result: u64 = optional_env_parse("NONEXISTENT_VAR_12345", 42).unwrap();
		assert_eq!(result, 42);
	}
}
