// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Default configuration file generation.

use std::fs;
use std::path::Path;

use tracing::{debug, info};

use crate::ConfigError;

/// Default configuration file template.
///
/// This template is written to ~/.config/loom/config.toml when no user config exists.
pub const DEFAULT_CONFIG_TEMPLATE: &str = r#"#
# Loom Configuration File
# Location: ~/.config/loom/config.toml
#
# This file was auto-generated with sensible defaults.
# Customize as needed. See: https://github.com/ghuntley/loom/blob/main/specs/configuration-system.md
#

# =============================================================================
# Global Settings
# =============================================================================

[global]
# The default LLM provider to use when none is specified.
# Must match a key in [providers.*] section.
default_provider = "anthropic"

# =============================================================================
# Provider Configurations
# =============================================================================

# Anthropic (Claude) - default provider
[providers.anthropic]
type = "anthropic"
# API key - prefer environment variable ANTHROPIC_API_KEY or LOOM_ANTHROPIC_API_KEY
# api_key = "sk-ant-..."
default_model = "claude-sonnet-4-20250514"

# OpenAI (GPT)
# [providers.openai]
# type = "openai"
# API key - prefer environment variable OPENAI_API_KEY or LOOM_OPENAI_API_KEY
# api_key = "sk-..."
# default_model = "gpt-4o"

# Ollama (local models)
# [providers.ollama]
# type = "ollama"
# host = "http://localhost:11434"
# default_model = "llama3"

# =============================================================================
# Tool Settings
# =============================================================================

[tools]
# Maximum file size to read (in bytes)
max_file_size_bytes = 1048576  # 1 MB

# Timeout for shell command execution (in seconds)
command_timeout_secs = 300

# Allow shell command execution
allow_shell = true

[tools.workspace]
# Allow tool operations outside the workspace directory
allow_outside_workspace = false

# =============================================================================
# Logging Configuration
# =============================================================================

[logging]
# Log level: error, warn, info, debug, trace
level = "info"

# Log format: pretty, json, compact
format = "pretty"

# =============================================================================
# Retry Configuration
# =============================================================================

[retry]
# Maximum retry attempts for transient failures
max_attempts = 3

# Initial delay before first retry (in milliseconds)
base_delay_ms = 500

# Maximum delay cap (in milliseconds)
max_delay_ms = 30000

# Exponential backoff multiplier
backoff_factor = 2.0

# Add randomization to delays to prevent thundering herd
jitter = true
"#;

/// Ensure the config directory exists and create a default config file if none exists.
///
/// Returns `true` if a new config file was created, `false` if one already existed.
pub fn ensure_default_config(config_file_path: &Path) -> Result<bool, ConfigError> {
	if config_file_path.exists() {
		debug!(path = %config_file_path.display(), "config file already exists");
		return Ok(false);
	}

	if let Some(parent) = config_file_path.parent() {
		if !parent.exists() {
			debug!(path = %parent.display(), "creating config directory");
			fs::create_dir_all(parent)?;
		}
	}

	info!(path = %config_file_path.display(), "creating default config file");
	fs::write(config_file_path, DEFAULT_CONFIG_TEMPLATE)?;

	Ok(true)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[test]
	fn test_default_config_template_is_valid_toml() {
		let result: Result<crate::layer::ConfigLayer, _> = toml::from_str(DEFAULT_CONFIG_TEMPLATE);
		assert!(
			result.is_ok(),
			"Default config template should be valid TOML: {:?}",
			result.err()
		);
	}

	#[test]
	fn test_ensure_default_config_creates_file() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join("loom/config.toml");

		assert!(!config_path.exists());

		let created = ensure_default_config(&config_path).unwrap();
		assert!(created);
		assert!(config_path.exists());

		let contents = fs::read_to_string(&config_path).unwrap();
		assert!(contents.contains("[global]"));
		assert!(contents.contains("default_provider"));
	}

	#[test]
	fn test_ensure_default_config_does_not_overwrite() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join("config.toml");

		fs::write(&config_path, "# existing config\n").unwrap();

		let created = ensure_default_config(&config_path).unwrap();
		assert!(!created);

		let contents = fs::read_to_string(&config_path).unwrap();
		assert_eq!(contents, "# existing config\n");
	}

	#[test]
	fn test_ensure_default_config_creates_parent_dirs() {
		let dir = tempdir().unwrap();
		let config_path = dir.path().join("nested/deep/path/config.toml");

		assert!(!config_path.parent().unwrap().exists());

		let created = ensure_default_config(&config_path).unwrap();
		assert!(created);
		assert!(config_path.exists());
	}
}
