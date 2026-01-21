// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Configuration sources: files, environment, CLI, defaults.

use std::path::PathBuf;

use loom_common_config::load_secret_env;
use tracing::{debug, trace};

use crate::layer::*;
use crate::paths::PathsConfig;
use crate::ConfigError;

/// Source precedence levels (higher = overrides lower).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
	Defaults = 10,
	SystemFile = 20,
	UserFile = 30,
	WorkspaceFile = 40,
	Environment = 50,
	Cli = 60,
}

/// Trait for configuration sources.
pub trait ConfigSource: Send + Sync {
	/// Name for logging
	fn name(&self) -> &'static str;

	/// Precedence level
	fn precedence(&self) -> Precedence;

	/// Load configuration layer from this source
	fn load(&self) -> Result<ConfigLayer, ConfigError>;
}

/// Built-in defaults source.
pub struct DefaultsSource;

impl ConfigSource for DefaultsSource {
	fn name(&self) -> &'static str {
		"defaults"
	}
	fn precedence(&self) -> Precedence {
		Precedence::Defaults
	}

	fn load(&self) -> Result<ConfigLayer, ConfigError> {
		debug!("loading defaults");
		// Return empty layer - defaults applied during finalization
		Ok(ConfigLayer::default())
	}
}

/// File-based configuration source (TOML).
pub struct FileSource {
	path: PathBuf,
	precedence: Precedence,
	name: &'static str,
}

impl FileSource {
	/// System config: /etc/loom/config.toml
	pub fn system() -> Self {
		Self {
			path: PathBuf::from("/etc/loom/config.toml"),
			precedence: Precedence::SystemFile,
			name: "system-config",
		}
	}

	/// User config: ~/.config/loom/config.toml
	pub fn user(paths: &PathsConfig) -> Self {
		Self {
			path: paths.user_config_file.clone(),
			precedence: Precedence::UserFile,
			name: "user-config",
		}
	}

	/// Workspace config: .loom/config.toml
	pub fn workspace() -> Result<Self, ConfigError> {
		let cwd = std::env::current_dir()?;
		Ok(Self {
			path: cwd.join(".loom/config.toml"),
			precedence: Precedence::WorkspaceFile,
			name: "workspace-config",
		})
	}

	/// Custom file path with specified precedence
	pub fn custom(path: PathBuf, precedence: Precedence, name: &'static str) -> Self {
		Self {
			path,
			precedence,
			name,
		}
	}
}

impl ConfigSource for FileSource {
	fn name(&self) -> &'static str {
		self.name
	}
	fn precedence(&self) -> Precedence {
		self.precedence
	}

	fn load(&self) -> Result<ConfigLayer, ConfigError> {
		if !self.path.exists() {
			debug!(path = %self.path.display(), source = self.name, "config file not found, skipping");
			return Ok(ConfigLayer::default());
		}

		debug!(path = %self.path.display(), source = self.name, "loading config file");

		let content = std::fs::read_to_string(&self.path)?;
		let layer: ConfigLayer = toml::from_str(&content).map_err(|e| ConfigError::TomlParse {
			path: self.path.clone(),
			source: e,
		})?;

		trace!(source = self.name, "parsed config layer");
		Ok(layer)
	}
}

/// Environment variable source.
///
/// Convention: LOOM_<SECTION>__<FIELD> (double underscore for nesting)
pub struct EnvSource;

impl ConfigSource for EnvSource {
	fn name(&self) -> &'static str {
		"environment"
	}
	fn precedence(&self) -> Precedence {
		Precedence::Environment
	}

	fn load(&self) -> Result<ConfigLayer, ConfigError> {
		debug!("loading environment variables");
		let mut layer = ConfigLayer::default();

		// Load API keys using load_secret_env (supports VAR and VAR_FILE)
		// Try LOOM_ prefixed first, then fall back to standard env vars
		if let Some(secret) = load_secret_env("LOOM_OPENAI_API_KEY")
			.ok()
			.flatten()
			.or_else(|| load_secret_env("OPENAI_API_KEY").ok().flatten())
		{
			trace!("loaded OpenAI API key from environment");
			ensure_openai_provider(&mut layer).api_key = Some(secret);
		}

		if let Some(secret) = load_secret_env("LOOM_ANTHROPIC_API_KEY")
			.ok()
			.flatten()
			.or_else(|| load_secret_env("ANTHROPIC_API_KEY").ok().flatten())
		{
			trace!("loaded Anthropic API key from environment");
			ensure_anthropic_provider(&mut layer).api_key = Some(secret);
		}

		// Load non-secret env vars
		for (key, value) in std::env::vars() {
			if !key.starts_with("LOOM_") {
				continue;
			}

			let value = value.trim().to_string();
			if value.is_empty() {
				continue;
			}

			trace!(key = %key, "processing env var");

			match key.as_str() {
				// Global settings
				"LOOM_DEFAULT_PROVIDER" => {
					layer
						.global
						.get_or_insert_with(GlobalLayer::default)
						.default_provider = Some(value);
				}
				"LOOM_WORKSPACE_ROOT" => {
					layer
						.global
						.get_or_insert_with(GlobalLayer::default)
						.workspace_root = Some(PathBuf::from(value));
				}

				// OpenAI provider (non-secret fields)
				"LOOM_OPENAI_BASE_URL" => {
					ensure_openai_provider(&mut layer).base_url = Some(value);
				}
				"LOOM_OPENAI_MODEL" => {
					ensure_openai_provider(&mut layer).default_model = Some(value);
				}

				// Anthropic provider (non-secret fields)
				"LOOM_ANTHROPIC_BASE_URL" => {
					ensure_anthropic_provider(&mut layer).base_url = Some(value);
				}
				"LOOM_ANTHROPIC_MODEL" => {
					ensure_anthropic_provider(&mut layer).default_model = Some(value);
				}

				// Logging
				"LOOM_LOG_LEVEL" => {
					layer
						.logging
						.get_or_insert_with(LoggingLayer::default)
						.level = Some(value);
				}
				"LOOM_LOG_FORMAT" => {
					layer
						.logging
						.get_or_insert_with(LoggingLayer::default)
						.format = Some(value);
				}

				// Retry
				"LOOM_RETRY_MAX_ATTEMPTS" => {
					if let Ok(v) = value.parse() {
						layer
							.retry
							.get_or_insert_with(RetryLayer::default)
							.max_attempts = Some(v);
					}
				}

				_ => {
					// Unknown LOOM_ variable, ignore
				}
			}
		}

		Ok(layer)
	}
}

fn ensure_openai_provider(layer: &mut ConfigLayer) -> &mut OpenAiLayer {
	let providers = layer.providers.get_or_insert_with(ProvidersLayer::default);
	providers
		.entries
		.entry("openai".to_string())
		.or_insert_with(|| ProviderLayer::OpenAi(OpenAiLayer::default()));

	match providers.entries.get_mut("openai") {
		Some(ProviderLayer::OpenAi(ref mut l)) => l,
		_ => unreachable!(),
	}
}

fn ensure_anthropic_provider(layer: &mut ConfigLayer) -> &mut AnthropicLayer {
	let providers = layer.providers.get_or_insert_with(ProvidersLayer::default);
	providers
		.entries
		.entry("anthropic".to_string())
		.or_insert_with(|| ProviderLayer::Anthropic(AnthropicLayer::default()));

	match providers.entries.get_mut("anthropic") {
		Some(ProviderLayer::Anthropic(ref mut l)) => l,
		_ => unreachable!(),
	}
}

/// CLI override source.
pub struct CliSource {
	overrides: CliOverrides,
}

/// CLI argument overrides.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
	pub provider: Option<String>,
	pub model: Option<String>,
	pub workspace: Option<PathBuf>,
	pub log_level: Option<String>,
	pub log_format: Option<String>,
	pub config_file: Option<PathBuf>,
}

impl CliSource {
	pub fn new(overrides: CliOverrides) -> Self {
		Self { overrides }
	}
}

impl ConfigSource for CliSource {
	fn name(&self) -> &'static str {
		"cli"
	}
	fn precedence(&self) -> Precedence {
		Precedence::Cli
	}

	fn load(&self) -> Result<ConfigLayer, ConfigError> {
		debug!("loading CLI overrides");
		let mut layer = ConfigLayer::default();

		if let Some(ref provider) = self.overrides.provider {
			layer
				.global
				.get_or_insert_with(GlobalLayer::default)
				.default_provider = Some(provider.clone());
		}

		if let Some(ref workspace) = self.overrides.workspace {
			layer
				.global
				.get_or_insert_with(GlobalLayer::default)
				.workspace_root = Some(workspace.clone());
		}

		if let Some(ref level) = self.overrides.log_level {
			layer
				.logging
				.get_or_insert_with(LoggingLayer::default)
				.level = Some(level.clone());
		}

		if let Some(ref format) = self.overrides.log_format {
			layer
				.logging
				.get_or_insert_with(LoggingLayer::default)
				.format = Some(format.clone());
		}

		Ok(layer)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_precedence_ordering() {
		assert!(Precedence::Cli > Precedence::Environment);
		assert!(Precedence::Environment > Precedence::WorkspaceFile);
		assert!(Precedence::WorkspaceFile > Precedence::UserFile);
		assert!(Precedence::UserFile > Precedence::SystemFile);
		assert!(Precedence::SystemFile > Precedence::Defaults);
	}

	#[test]
	fn test_defaults_source_returns_empty_layer() {
		let source = DefaultsSource;
		let layer = source.load().unwrap();
		assert!(layer.global.is_none());
		assert!(layer.providers.is_none());
	}

	#[test]
	fn test_file_source_missing_file_returns_empty() {
		let source = FileSource {
			path: PathBuf::from("/nonexistent/config.toml"),
			precedence: Precedence::UserFile,
			name: "test",
		};
		let layer = source.load().unwrap();
		assert!(layer.global.is_none());
	}
}
