// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;
use std::sync::Arc;

use jj_lib::settings::UserSettings;

use crate::error::{Result, SpoolError};

/// Spool configuration settings, wrapping jj-lib's UserSettings.
#[derive(Clone)]
pub struct SpoolSettings {
	inner: Arc<UserSettings>,
}

impl SpoolSettings {
	/// Load settings from default configuration paths.
	///
	/// Searches for configuration in:
	/// 1. `$XDG_CONFIG_HOME/loom/spool.toml` (or `~/.config/loom/spool.toml`)
	/// 2. Environment variables with `LOOM_SPOOL_` prefix
	pub fn load() -> Result<Self> {
		let config = Self::build_config()?;
		let inner = UserSettings::from_config(config)
			.map_err(|e| SpoolError::Config(format!("Failed to create settings: {e}")))?;
		Ok(Self {
			inner: Arc::new(inner),
		})
	}

	/// Create settings with default configuration.
	pub fn default_settings() -> Result<Self> {
		// Start with jj-lib's defaults (includes signing.backend = "none", etc.)
		let mut config = jj_lib::config::StackedConfig::with_defaults();

		// Add spool-specific configuration
		let hostname = hostname::get()
			.map(|h| h.to_string_lossy().to_string())
			.unwrap_or_else(|_| "localhost".to_string());

		let username = std::env::var("USER")
			.or_else(|_| std::env::var("USERNAME"))
			.unwrap_or_else(|_| "user".to_string());

		let spool_config = format!(
			r#"
[user]
name = "{username}"
email = "{username}@{hostname}"

[operation]
hostname = "{hostname}"
username = "{username}"

[signing]
behavior = "drop"

[ui]
pager = "less"
"#
		);

		if let Ok(layer) =
			jj_lib::config::ConfigLayer::parse(jj_lib::config::ConfigSource::User, &spool_config)
		{
			config.add_layer(layer);
		}

		let inner = UserSettings::from_config(config)
			.map_err(|e| SpoolError::Config(format!("Failed to create settings: {e}")))?;
		Ok(Self {
			inner: Arc::new(inner),
		})
	}

	/// Get the inner jj-lib UserSettings.
	pub fn inner(&self) -> &UserSettings {
		&self.inner
	}

	/// Get the configured user name.
	pub fn user_name(&self) -> String {
		self.inner.user_name().to_string()
	}

	/// Get the configured user email.
	pub fn user_email(&self) -> String {
		self.inner.user_email().to_string()
	}

	/// Get the configuration directory path.
	pub fn config_dir() -> Option<PathBuf> {
		dirs::config_dir().map(|p| p.join("loom"))
	}

	/// Get the data directory path.
	pub fn data_dir() -> Option<PathBuf> {
		dirs::data_dir().map(|p| p.join("loom"))
	}

	fn build_config() -> Result<jj_lib::config::StackedConfig> {
		// Start with jj-lib's defaults
		let mut config = jj_lib::config::StackedConfig::with_defaults();

		// Load from XDG config path
		if let Some(config_dir) = Self::config_dir() {
			let config_path = config_dir.join("spool.toml");
			if config_path.exists() {
				if let Ok(contents) = std::fs::read_to_string(&config_path) {
					if let Ok(layer) =
						jj_lib::config::ConfigLayer::parse(jj_lib::config::ConfigSource::User, &contents)
					{
						config.add_layer(layer);
					}
				}
			}
		}

		// Note: Environment variable support via LOOM_SPOOL_* prefix
		// is planned but not yet implemented. For now, use the config file.

		Ok(config)
	}
}

impl Default for SpoolSettings {
	fn default() -> Self {
		Self::default_settings().expect("default settings should always work")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_settings() {
		let settings = SpoolSettings::default_settings();
		assert!(settings.is_ok());
	}

	#[test]
	fn test_config_dir() {
		let dir = SpoolSettings::config_dir();
		assert!(dir.is_some());
		assert!(dir.unwrap().ends_with("loom"));
	}
}
