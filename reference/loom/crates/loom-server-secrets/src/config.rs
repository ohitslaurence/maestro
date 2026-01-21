// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration for the secrets system.
//!
//! This module handles loading and validating secrets configuration including:
//! - Master key for envelope encryption (KEK)
//! - SVID signing key for weaver identity
//! - TTL settings for SVIDs
//! - Pod verification options

use std::path::PathBuf;
use std::time::Duration;

use loom_common_secret::SecretString;
use tracing::instrument;

use crate::error::{SecretsError, SecretsResult};

/// Default SVID TTL (15 minutes).
const DEFAULT_SVID_TTL_SECS: u64 = 900;

/// Minimum allowed SVID TTL (1 minute).
const MIN_SVID_TTL_SECS: u64 = 60;

/// Maximum allowed SVID TTL (1 hour).
const MAX_SVID_TTL_SECS: u64 = 3600;

/// Configuration for the secrets system.
#[derive(Clone)]
pub struct SecretsConfig {
	/// Master key for envelope encryption (Key Encryption Key).
	/// Must be exactly 32 bytes (256 bits) for AES-256-GCM.
	master_key: SecretString,

	/// Ed25519 private key for signing SVIDs.
	/// PEM-encoded PKCS#8 format.
	svid_signing_key: SecretString,

	/// Time-to-live for issued SVIDs.
	svid_ttl: Duration,

	/// Whether to verify weaver pods via K8s API before issuing SVIDs.
	verify_pods: bool,

	/// Expected issuer claim for SVID validation.
	svid_issuer: String,
}

impl SecretsConfig {
	/// Create a new secrets configuration.
	///
	/// # Arguments
	/// * `master_key` - 32-byte master key (hex or base64 encoded)
	/// * `svid_signing_key` - PEM-encoded Ed25519 private key
	/// * `svid_ttl` - Optional TTL for SVIDs (defaults to 15 minutes)
	/// * `verify_pods` - Whether to verify weaver pods via K8s API
	/// * `svid_issuer` - Issuer claim for SVIDs
	pub fn new(
		master_key: SecretString,
		svid_signing_key: SecretString,
		svid_ttl: Option<Duration>,
		verify_pods: bool,
		svid_issuer: String,
	) -> SecretsResult<Self> {
		let ttl = svid_ttl.unwrap_or(Duration::from_secs(DEFAULT_SVID_TTL_SECS));

		if ttl.as_secs() < MIN_SVID_TTL_SECS {
			return Err(SecretsError::Configuration(format!(
				"SVID TTL must be at least {} seconds",
				MIN_SVID_TTL_SECS
			)));
		}

		if ttl.as_secs() > MAX_SVID_TTL_SECS {
			return Err(SecretsError::Configuration(format!(
				"SVID TTL must be at most {} seconds",
				MAX_SVID_TTL_SECS
			)));
		}

		Ok(Self {
			master_key,
			svid_signing_key,
			svid_ttl: ttl,
			verify_pods,
			svid_issuer,
		})
	}

	/// Load configuration from environment variables.
	///
	/// Environment variables:
	/// - `LOOM_SECRETS_MASTER_KEY` - Master key (hex or base64)
	/// - `LOOM_SECRETS_MASTER_KEY_FILE` - Path to file containing master key
	/// - `LOOM_SECRETS_SVID_KEY` - SVID signing key (PEM)
	/// - `LOOM_SECRETS_SVID_KEY_FILE` - Path to file containing SVID key
	/// - `LOOM_SECRETS_SVID_TTL_SECS` - SVID TTL in seconds (default: 900)
	/// - `LOOM_SECRETS_VERIFY_PODS` - Enable pod verification (default: true)
	/// - `LOOM_SECRETS_SVID_ISSUER` - SVID issuer claim (default: "loom")
	#[instrument(skip_all)]
	pub fn from_env() -> SecretsResult<Self> {
		let master_key = load_secret_from_env("LOOM_SECRETS_MASTER_KEY")?
			.ok_or(SecretsError::MasterKeyNotConfigured)?;

		let svid_signing_key = load_secret_from_env("LOOM_SECRETS_SVID_KEY")?
			.ok_or(SecretsError::SvidSigningKeyNotConfigured)?;

		let svid_ttl_secs: u64 = std::env::var("LOOM_SECRETS_SVID_TTL_SECS")
			.ok()
			.and_then(|s| s.parse().ok())
			.unwrap_or(DEFAULT_SVID_TTL_SECS);

		let verify_pods = std::env::var("LOOM_SECRETS_VERIFY_PODS")
			.map(|v| v != "0" && v.to_lowercase() != "false")
			.unwrap_or(true);

		let svid_issuer =
			std::env::var("LOOM_SECRETS_SVID_ISSUER").unwrap_or_else(|_| "loom".to_string());

		Self::new(
			master_key,
			svid_signing_key,
			Some(Duration::from_secs(svid_ttl_secs)),
			verify_pods,
			svid_issuer,
		)
	}

	/// Get the master key for envelope encryption.
	pub fn master_key(&self) -> &SecretString {
		&self.master_key
	}

	/// Get the SVID signing key.
	pub fn svid_signing_key(&self) -> &SecretString {
		&self.svid_signing_key
	}

	/// Get the SVID TTL.
	pub fn svid_ttl(&self) -> Duration {
		self.svid_ttl
	}

	/// Whether to verify weaver pods via K8s API.
	pub fn verify_pods(&self) -> bool {
		self.verify_pods
	}

	/// Get the expected SVID issuer claim.
	pub fn svid_issuer(&self) -> &str {
		&self.svid_issuer
	}
}

/// Load a secret from environment, with support for _FILE suffix.
///
/// Checks for:
/// 1. `{prefix}` - Direct value
/// 2. `{prefix}_FILE` - Path to file containing value
fn load_secret_from_env(prefix: &str) -> SecretsResult<Option<SecretString>> {
	// Try direct value first
	if let Ok(value) = std::env::var(prefix) {
		if !value.is_empty() {
			return Ok(Some(SecretString::new(value)));
		}
	}

	// Try _FILE variant
	let file_var = format!("{prefix}_FILE");
	if let Ok(path_str) = std::env::var(&file_var) {
		let path = PathBuf::from(&path_str);
		if path.exists() {
			let content = std::fs::read_to_string(&path).map_err(|e| {
				SecretsError::Configuration(format!("failed to read {file_var} from {path_str}: {e}"))
			})?;
			return Ok(Some(SecretString::new(content.trim().to_string())));
		} else {
			return Err(SecretsError::Configuration(format!(
				"file specified in {file_var} does not exist: {path_str}"
			)));
		}
	}

	Ok(None)
}

impl std::fmt::Debug for SecretsConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SecretsConfig")
			.field("master_key", &"[REDACTED]")
			.field("svid_signing_key", &"[REDACTED]")
			.field("svid_ttl", &self.svid_ttl)
			.field("verify_pods", &self.verify_pods)
			.field("svid_issuer", &self.svid_issuer)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn config_validates_svid_ttl_min() {
		let result = SecretsConfig::new(
			SecretString::new("test_master_key".to_string()),
			SecretString::new("test_svid_key".to_string()),
			Some(Duration::from_secs(30)), // Below minimum
			false,
			"loom".to_string(),
		);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("at least"));
	}

	#[test]
	fn config_validates_svid_ttl_max() {
		let result = SecretsConfig::new(
			SecretString::new("test_master_key".to_string()),
			SecretString::new("test_svid_key".to_string()),
			Some(Duration::from_secs(7200)), // Above maximum
			false,
			"loom".to_string(),
		);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("at most"));
	}

	#[test]
	fn config_uses_default_svid_ttl() {
		let config = SecretsConfig::new(
			SecretString::new("test_master_key".to_string()),
			SecretString::new("test_svid_key".to_string()),
			None,
			false,
			"loom".to_string(),
		)
		.unwrap();
		assert_eq!(
			config.svid_ttl(),
			Duration::from_secs(DEFAULT_SVID_TTL_SECS)
		);
	}

	#[test]
	fn config_debug_redacts_secrets() {
		let config = SecretsConfig::new(
			SecretString::new("super_secret_key".to_string()),
			SecretString::new("super_secret_svid".to_string()),
			None,
			true,
			"loom".to_string(),
		)
		.unwrap();
		let debug = format!("{config:?}");
		assert!(!debug.contains("super_secret"));
		assert!(debug.contains("[REDACTED]"));
	}
}
