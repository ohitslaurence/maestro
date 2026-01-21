// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration registry - manages sources and merges layers.

use tracing::{debug, info};

use crate::layer::ConfigLayer;
use crate::paths::PathsConfig;
use crate::runtime::LoomConfig;
use crate::sources::ConfigSource;
use crate::validation::validate_config;
use crate::ConfigError;

/// Registry that manages configuration sources and merges them.
pub struct ConfigRegistry {
	sources: Vec<Box<dyn ConfigSource>>,
}

impl ConfigRegistry {
	/// Create a new empty registry.
	pub fn new() -> Self {
		Self {
			sources: Vec::new(),
		}
	}

	/// Register a configuration source.
	pub fn register(&mut self, source: Box<dyn ConfigSource>) {
		debug!(source = source.name(), precedence = ?source.precedence(), "registering config source");
		self.sources.push(source);
	}

	/// Load configuration from all sources, merge, and validate.
	///
	/// Sources are sorted by precedence (lowest first) and merged
	/// so higher precedence sources override lower ones.
	pub fn load(&self, paths: PathsConfig) -> Result<LoomConfig, ConfigError> {
		// Sort sources by precedence (lowest first)
		let mut sorted_sources: Vec<_> = self.sources.iter().collect();
		sorted_sources.sort_by_key(|s| s.precedence());

		info!(
			source_count = sorted_sources.len(),
			"loading configuration from sources"
		);

		// Merge all layers
		let mut merged = ConfigLayer::default();
		for source in &sorted_sources {
			match source.load() {
				Ok(layer) => {
					debug!(source = source.name(), "merging config layer");
					merged.merge(layer);
				}
				Err(e) => {
					// Log error but continue - missing files are OK
					debug!(source = source.name(), error = %e, "failed to load source, skipping");
				}
			}
		}

		// Build runtime config
		let config = LoomConfig::from_layer(merged, paths)?;

		// Validate
		validate_config(&config)?;

		info!(
				default_provider = %config.global.default_provider,
				provider_count = config.providers.len(),
				log_level = ?config.logging.level,
				"configuration loaded successfully"
		);

		Ok(config)
	}

	/// Get the number of registered sources.
	pub fn source_count(&self) -> usize {
		self.sources.len()
	}
}

impl Default for ConfigRegistry {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sources::DefaultsSource;

	#[test]
	fn test_registry_registers_sources() {
		let mut registry = ConfigRegistry::new();
		assert_eq!(registry.source_count(), 0);

		registry.register(Box::new(DefaultsSource));
		assert_eq!(registry.source_count(), 1);
	}

	#[test]
	fn test_registry_loads_with_defaults() {
		let mut registry = ConfigRegistry::new();
		registry.register(Box::new(DefaultsSource));

		let paths = PathsConfig {
			user_config_file: "/tmp/test/config.toml".into(),
			system_config_file: "/etc/loom/config.toml".into(),
			data_dir: "/tmp/test/data".into(),
			cache_dir: "/tmp/test/cache".into(),
			state_dir: "/tmp/test/state".into(),
		};

		let config = registry.load(paths);
		assert!(config.is_ok());

		let config = config.unwrap();
		assert_eq!(config.global.default_provider, "anthropic");
	}

	/// Test that sources are merged in precedence order.
	#[test]
	fn test_precedence_merge_order() {
		use crate::sources::Precedence;

		struct MockSource {
			name: &'static str,
			precedence: Precedence,
			provider_name: String,
		}

		impl ConfigSource for MockSource {
			fn name(&self) -> &'static str {
				self.name
			}
			fn precedence(&self) -> Precedence {
				self.precedence
			}

			fn load(&self) -> Result<ConfigLayer, ConfigError> {
				let mut layer = ConfigLayer::default();
				layer.global = Some(crate::layer::GlobalLayer {
					default_provider: Some(self.provider_name.clone()),
					..Default::default()
				});
				Ok(layer)
			}
		}

		let mut registry = ConfigRegistry::new();

		// Add in wrong order - registry should sort
		registry.register(Box::new(MockSource {
			name: "cli",
			precedence: Precedence::Cli,
			provider_name: "cli-provider".to_string(),
		}));
		registry.register(Box::new(MockSource {
			name: "user",
			precedence: Precedence::UserFile,
			provider_name: "user-provider".to_string(),
		}));

		let paths = PathsConfig {
			user_config_file: "/tmp/test/config.toml".into(),
			system_config_file: "/etc/loom/config.toml".into(),
			data_dir: "/tmp/test/data".into(),
			cache_dir: "/tmp/test/cache".into(),
			state_dir: "/tmp/test/state".into(),
		};

		let config = registry.load(paths).unwrap();

		// CLI has higher precedence, so its value wins
		assert_eq!(config.global.default_provider, "cli-provider");
	}
}
