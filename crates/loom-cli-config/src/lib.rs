// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration management for Loom AI agent.
//!
//! This crate provides:
//! - XDG Base Directory compliant path resolution
//! - Layered configuration from multiple sources
//! - TOML configuration file parsing
//! - Environment variable overrides
//! - Configuration validation

pub mod defaults;
pub mod error;
pub mod layer;
pub mod paths;
pub mod registry;
pub mod runtime;
pub mod sources;
pub mod validation;

pub use defaults::{ensure_default_config, DEFAULT_CONFIG_TEMPLATE};
pub use error::ConfigError;
pub use layer::ConfigLayer;
pub use paths::PathsConfig;
pub use registry::ConfigRegistry;
pub use runtime::LoomConfig;
pub use sources::{ConfigSource, Precedence};

/// Load configuration from all sources with default precedence.
///
/// If no user config file exists, a default one is created at
/// `~/.config/loom/config.toml` with sensible defaults.
pub fn load_config() -> Result<LoomConfig, ConfigError> {
	let paths = paths::resolve_xdg_paths()?;

	// Ensure default user config exists
	defaults::ensure_default_config(&paths.user_config_file)?;

	let mut registry = ConfigRegistry::new();

	registry.register(Box::new(sources::DefaultsSource));
	registry.register(Box::new(sources::FileSource::system()));
	registry.register(Box::new(sources::FileSource::user(&paths)));
	if let Ok(ws) = sources::FileSource::workspace() {
		registry.register(Box::new(ws));
	}
	registry.register(Box::new(sources::EnvSource));

	registry.load(paths)
}

/// Load configuration with CLI overrides.
///
/// If no user config file exists, a default one is created at
/// `~/.config/loom/config.toml` with sensible defaults.
pub fn load_config_with_cli(cli: sources::CliOverrides) -> Result<LoomConfig, ConfigError> {
	let paths = paths::resolve_xdg_paths()?;

	// Ensure default user config exists
	defaults::ensure_default_config(&paths.user_config_file)?;

	let mut registry = ConfigRegistry::new();

	registry.register(Box::new(sources::DefaultsSource));
	registry.register(Box::new(sources::FileSource::system()));
	registry.register(Box::new(sources::FileSource::user(&paths)));
	if let Ok(ws) = sources::FileSource::workspace() {
		registry.register(Box::new(ws));
	}
	registry.register(Box::new(sources::EnvSource));
	registry.register(Box::new(sources::CliSource::new(cli)));

	registry.load(paths)
}
