// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration validation rules.

use tracing::warn;

use crate::runtime::{LoomConfig, ProviderConfig};
use crate::ConfigError;

/// Validate the configuration.
///
/// Returns Ok(()) if valid, or ConfigError::Validation with details.
pub fn validate_config(config: &LoomConfig) -> Result<(), ConfigError> {
	validate_global(config)?;
	validate_providers(config)?;
	validate_retry(config)?;
	validate_tools(config)?;

	Ok(())
}

fn validate_global(config: &LoomConfig) -> Result<(), ConfigError> {
	// If providers are configured, default_provider should exist
	if !config.providers.is_empty()
		&& !config
			.providers
			.contains_key(&config.global.default_provider)
	{
		// Warn but don't fail - provider might be coming from env var at runtime
		warn!(
				default_provider = %config.global.default_provider,
				available = ?config.providers.keys().collect::<Vec<_>>(),
				"default_provider not found in configured providers"
		);
	}

	Ok(())
}

fn validate_providers(config: &LoomConfig) -> Result<(), ConfigError> {
	for (name, provider) in &config.providers {
		match provider {
			ProviderConfig::OpenAi(cfg) => {
				if cfg.api_key.is_none() {
					warn!(provider = %name, "OpenAI provider has no api_key configured");
				}
				if cfg.base_url.is_empty() {
					return Err(ConfigError::invalid_value(
						format!("providers.{name}.base_url"),
						"base_url cannot be empty",
					));
				}
			}
			ProviderConfig::Anthropic(cfg) => {
				if cfg.api_key.is_none() {
					warn!(provider = %name, "Anthropic provider has no api_key configured");
				}
				if cfg.base_url.is_empty() {
					return Err(ConfigError::invalid_value(
						format!("providers.{name}.base_url"),
						"base_url cannot be empty",
					));
				}
			}
			ProviderConfig::Ollama(cfg) => {
				if cfg.host.is_empty() {
					return Err(ConfigError::invalid_value(
						format!("providers.{name}.host"),
						"host cannot be empty",
					));
				}
			}
			ProviderConfig::Custom(cfg) => {
				if cfg.base_url.is_empty() {
					return Err(ConfigError::invalid_value(
						format!("providers.{name}.base_url"),
						"base_url cannot be empty for custom provider",
					));
				}
			}
		}
	}

	Ok(())
}

fn validate_retry(config: &LoomConfig) -> Result<(), ConfigError> {
	let retry = &config.retry;

	if retry.max_attempts == 0 {
		return Err(ConfigError::invalid_value(
			"retry.max_attempts",
			"must be at least 1",
		));
	}

	if retry.max_attempts > 20 {
		return Err(ConfigError::invalid_value(
			"retry.max_attempts",
			"must be at most 20 (unreasonably high)",
		));
	}

	if retry.backoff_factor < 1.0 {
		return Err(ConfigError::invalid_value(
			"retry.backoff_factor",
			"must be at least 1.0",
		));
	}

	if retry.backoff_factor > 10.0 {
		return Err(ConfigError::invalid_value(
			"retry.backoff_factor",
			"must be at most 10.0",
		));
	}

	if retry.base_delay > retry.max_delay {
		return Err(ConfigError::invalid_value(
			"retry.base_delay",
			"cannot be greater than max_delay",
		));
	}

	Ok(())
}

fn validate_tools(config: &LoomConfig) -> Result<(), ConfigError> {
	let tools = &config.tools;

	if tools.max_file_size_bytes == 0 {
		return Err(ConfigError::invalid_value(
			"tools.max_file_size_bytes",
			"must be greater than 0",
		));
	}

	if tools.command_timeout.as_secs() == 0 {
		return Err(ConfigError::invalid_value(
			"tools.command_timeout_secs",
			"must be greater than 0",
		));
	}

	// Validate allowed_paths are relative or absolute
	for path in &tools.workspace.allowed_paths {
		if path.to_string_lossy().contains("..") {
			warn!(
					path = %path.display(),
					"allowed_path contains '..' which may be a security risk"
			);
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::paths::PathsConfig;
	use std::collections::HashMap;
	use std::time::Duration;

	fn minimal_config() -> LoomConfig {
		LoomConfig {
			global: crate::runtime::GlobalConfig::default(),
			providers: HashMap::new(),
			tools: crate::runtime::ToolsConfig::default(),
			logging: crate::runtime::LoggingConfig::default(),
			retry: crate::runtime::RetryConfig::default(),
			paths: PathsConfig {
				user_config_file: "/tmp/config.toml".into(),
				system_config_file: "/etc/loom/config.toml".into(),
				data_dir: "/tmp/data".into(),
				cache_dir: "/tmp/cache".into(),
				state_dir: "/tmp/state".into(),
			},
		}
	}

	/// Test that a minimal valid config passes validation.
	/// This ensures the default values are valid.
	#[test]
	fn test_minimal_config_is_valid() {
		let config = minimal_config();
		assert!(validate_config(&config).is_ok());
	}

	/// Test that zero max_attempts fails validation.
	#[test]
	fn test_zero_max_attempts_fails() {
		let mut config = minimal_config();
		config.retry.max_attempts = 0;

		let result = validate_config(&config);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("max_attempts"));
	}

	/// Test that excessive max_attempts fails validation.
	#[test]
	fn test_excessive_max_attempts_fails() {
		let mut config = minimal_config();
		config.retry.max_attempts = 100;

		let result = validate_config(&config);
		assert!(result.is_err());
	}

	/// Test that base_delay > max_delay fails validation.
	#[test]
	fn test_base_delay_greater_than_max_fails() {
		let mut config = minimal_config();
		config.retry.base_delay = Duration::from_secs(60);
		config.retry.max_delay = Duration::from_secs(30);

		let result = validate_config(&config);
		assert!(result.is_err());
	}

	/// Test that empty base_url for provider fails.
	#[test]
	fn test_empty_provider_base_url_fails() {
		let mut config = minimal_config();
		config.providers.insert(
			"test".to_string(),
			ProviderConfig::OpenAi(crate::runtime::OpenAiConfig {
				api_key: Some(loom_common_secret::SecretString::new("key".to_string())),
				base_url: "".to_string(),
				default_model: "model".to_string(),
				organization: None,
			}),
		);

		let result = validate_config(&config);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("base_url"));
	}
}
