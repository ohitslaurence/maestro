// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Environment variable helpers for loading secrets.
//!
//! This module provides utilities for loading secrets from environment
//! variables with support for the `*_FILE` convention used by Docker secrets
//! and Kubernetes.

use std::path::PathBuf;
use std::{env, fs};

use loom_common_secret::Secret;
use thiserror::Error;

/// Errors that can occur when loading secrets from environment variables.
#[derive(Debug, Error)]
pub enum SecretEnvError {
	/// Failed to read the secret file.
	#[error("failed to read secret file at {path}: {source}")]
	Io {
		path: PathBuf,
		#[source]
		source: std::io::Error,
	},

	/// The secret file path was empty.
	#[error("secret file path in {var} is empty")]
	EmptyPath { var: String },
}

/// Load a secret from the environment using the `VAR` / `VAR_FILE` convention.
///
/// This function supports two ways to provide secrets:
///
/// 1. **Direct value**: Set `VAR` to the secret value directly
/// 2. **File reference**: Set `VAR_FILE` to a path containing the secret
///
/// # Precedence
///
/// 1. If `{var}_FILE` is set, read the secret from that file path
/// 2. Otherwise, if `{var}` is set, use its value directly
/// 3. Otherwise, return `Ok(None)`
///
/// # File Format
///
/// When reading from a file:
/// - A single trailing newline is stripped (common in secret files)
/// - All other content is preserved as-is
///
/// # Use Cases
///
/// This convention is commonly used with:
/// - Docker secrets (mounted at `/run/secrets/`)
/// - Kubernetes secrets (mounted as files in pods)
/// - HashiCorp Vault Agent (writes secrets to files)
///
/// # Example
///
/// ```no_run
/// use loom_common_config::load_secret_env;
///
/// // If OPENAI_API_KEY_FILE=/run/secrets/openai_key is set,
/// // reads the secret from that file.
/// // Otherwise, if OPENAI_API_KEY=sk-xxx is set, uses that value.
/// let api_key = load_secret_env("OPENAI_API_KEY")?;
///
/// if let Some(key) = api_key {
///     println!("API key configured: {}", key); // prints "[REDACTED]"
///     // Use key.expose() when you actually need the value
/// }
/// # Ok::<(), loom_common_config::SecretEnvError>(())
/// ```
pub fn load_secret_env(var: &str) -> Result<Option<Secret<String>>, SecretEnvError> {
	let file_var = format!("{var}_FILE");

	if let Ok(path_str) = env::var(&file_var) {
		if path_str.is_empty() {
			return Err(SecretEnvError::EmptyPath { var: file_var });
		}

		let path = PathBuf::from(&path_str);
		let content = fs::read_to_string(&path).map_err(|e| SecretEnvError::Io {
			path: path.clone(),
			source: e,
		})?;

		let secret = content.strip_suffix('\n').unwrap_or(&content).to_string();
		return Ok(Some(Secret::new(secret)));
	}

	if let Ok(value) = env::var(var) {
		return Ok(Some(Secret::new(value)));
	}

	Ok(None)
}

/// Load a required secret from the environment.
///
/// This is a convenience wrapper around [`load_secret_env`] that returns an
/// error if neither `VAR` nor `VAR_FILE` is set.
///
/// # Example
///
/// ```no_run
/// use loom_common_config::env::require_secret_env;
///
/// let api_key = require_secret_env("OPENAI_API_KEY")?;
/// // api_key is guaranteed to be Some here
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn require_secret_env(var: &str) -> Result<Secret<String>, RequiredSecretError> {
	load_secret_env(var)
		.map_err(RequiredSecretError::Load)?
		.ok_or_else(|| RequiredSecretError::Missing {
			var: var.to_string(),
			file_var: format!("{var}_FILE"),
		})
}

/// Error returned when a required secret is not found.
#[derive(Debug, Error)]
pub enum RequiredSecretError {
	#[error("required secret not found: set either {var} or {file_var}")]
	Missing { var: String, file_var: String },

	#[error(transparent)]
	Load(#[from] SecretEnvError),
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;
	use tempfile::NamedTempFile;

	mod load_secret_env_tests {
		use super::*;

		/// Verifies that load_secret_env returns None when neither VAR nor VAR_FILE
		/// is set. This is important for optional configuration values.
		#[test]
		fn returns_none_when_not_set() {
			let unique_var = "LOOM_TEST_NONEXISTENT_VAR_12345";
			env::remove_var(unique_var);
			env::remove_var(format!("{unique_var}_FILE"));

			let result = load_secret_env(unique_var).unwrap();
			assert!(result.is_none());
		}

		/// Verifies that load_secret_env reads from VAR when set directly.
		/// This is the simple case for environment-based configuration.
		#[test]
		fn reads_from_direct_env_var() {
			let unique_var = "LOOM_TEST_DIRECT_VAR_12345";
			env::set_var(unique_var, "direct-secret-value");
			env::remove_var(format!("{unique_var}_FILE"));

			let result = load_secret_env(unique_var).unwrap();
			assert!(result.is_some());
			assert_eq!(result.unwrap().expose(), "direct-secret-value");

			env::remove_var(unique_var);
		}

		/// Verifies that load_secret_env reads from a file when VAR_FILE is set.
		/// This supports Docker/Kubernetes secrets.
		#[test]
		fn reads_from_file_when_file_var_set() {
			let unique_var = "LOOM_TEST_FILE_VAR_12345";
			let mut temp_file = NamedTempFile::new().unwrap();
			writeln!(temp_file, "file-secret-value").unwrap();

			env::set_var(
				format!("{unique_var}_FILE"),
				temp_file.path().to_str().unwrap(),
			);
			env::remove_var(unique_var);

			let result = load_secret_env(unique_var).unwrap();
			assert!(result.is_some());
			assert_eq!(result.unwrap().expose(), "file-secret-value");

			env::remove_var(format!("{unique_var}_FILE"));
		}

		/// Verifies that VAR_FILE takes precedence over VAR.
		/// This is important for the precedence rules to be predictable.
		#[test]
		fn file_var_takes_precedence() {
			let unique_var = "LOOM_TEST_PRECEDENCE_VAR_12345";
			let mut temp_file = NamedTempFile::new().unwrap();
			writeln!(temp_file, "file-secret").unwrap();

			env::set_var(unique_var, "direct-secret");
			env::set_var(
				format!("{unique_var}_FILE"),
				temp_file.path().to_str().unwrap(),
			);

			let result = load_secret_env(unique_var).unwrap();
			assert!(result.is_some());
			assert_eq!(result.unwrap().expose(), "file-secret");

			env::remove_var(unique_var);
			env::remove_var(format!("{unique_var}_FILE"));
		}

		/// Verifies that trailing newlines are stripped from file content.
		/// This is important because most text editors add trailing newlines.
		#[test]
		fn strips_single_trailing_newline() {
			let unique_var = "LOOM_TEST_NEWLINE_VAR_12345";
			let mut temp_file = NamedTempFile::new().unwrap();
			writeln!(temp_file, "secret-with-newline").unwrap();

			env::set_var(
				format!("{unique_var}_FILE"),
				temp_file.path().to_str().unwrap(),
			);

			let result = load_secret_env(unique_var).unwrap();
			assert_eq!(result.unwrap().expose(), "secret-with-newline");

			env::remove_var(format!("{unique_var}_FILE"));
		}

		/// Verifies that content without trailing newline is preserved.
		/// This ensures we don't corrupt secrets that don't end with newlines.
		#[test]
		fn preserves_content_without_trailing_newline() {
			let unique_var = "LOOM_TEST_NO_NEWLINE_VAR_12345";
			let mut temp_file = NamedTempFile::new().unwrap();
			write!(temp_file, "secret-no-newline").unwrap();

			env::set_var(
				format!("{unique_var}_FILE"),
				temp_file.path().to_str().unwrap(),
			);

			let result = load_secret_env(unique_var).unwrap();
			assert_eq!(result.unwrap().expose(), "secret-no-newline");

			env::remove_var(format!("{unique_var}_FILE"));
		}

		/// Verifies that an error is returned when the secret file doesn't exist.
		/// This provides clear feedback when configuration is wrong.
		#[test]
		fn returns_error_for_missing_file() {
			let unique_var = "LOOM_TEST_MISSING_FILE_VAR_12345";
			env::set_var(format!("{unique_var}_FILE"), "/nonexistent/path/to/secret");

			let result = load_secret_env(unique_var);
			assert!(result.is_err());
			assert!(matches!(result.unwrap_err(), SecretEnvError::Io { .. }));

			env::remove_var(format!("{unique_var}_FILE"));
		}

		/// Verifies that an error is returned when VAR_FILE is set to empty string.
		/// This catches configuration mistakes.
		#[test]
		fn returns_error_for_empty_file_path() {
			let unique_var = "LOOM_TEST_EMPTY_PATH_VAR_12345";
			env::set_var(format!("{unique_var}_FILE"), "");

			let result = load_secret_env(unique_var);
			assert!(result.is_err());
			assert!(matches!(
				result.unwrap_err(),
				SecretEnvError::EmptyPath { .. }
			));

			env::remove_var(format!("{unique_var}_FILE"));
		}
	}

	mod require_secret_env_tests {
		use super::*;

		/// Verifies that require_secret_env returns the secret when VAR is set.
		#[test]
		fn returns_secret_when_set() {
			let unique_var = "LOOM_TEST_REQUIRE_VAR_12345";
			env::set_var(unique_var, "required-secret");

			let result = require_secret_env(unique_var).unwrap();
			assert_eq!(result.expose(), "required-secret");

			env::remove_var(unique_var);
		}

		/// Verifies that require_secret_env returns error when neither VAR nor
		/// VAR_FILE is set.
		#[test]
		fn returns_error_when_not_set() {
			let unique_var = "LOOM_TEST_REQUIRE_MISSING_VAR_12345";
			env::remove_var(unique_var);
			env::remove_var(format!("{unique_var}_FILE"));

			let result = require_secret_env(unique_var);
			assert!(result.is_err());
			assert!(matches!(
				result.unwrap_err(),
				RequiredSecretError::Missing { .. }
			));
		}
	}
}
