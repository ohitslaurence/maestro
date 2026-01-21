// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Runtime configuration types with resolved defaults.

use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::layer::*;
use crate::paths::PathsConfig;
use crate::ConfigError;

/// The final, validated configuration for Loom.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoomConfig {
	pub global: GlobalConfig,
	pub providers: HashMap<String, ProviderConfig>,
	pub tools: ToolsConfig,
	pub logging: LoggingConfig,
	pub retry: RetryConfig,

	/// Resolved XDG paths (not serialized)
	#[serde(skip)]
	pub paths: PathsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
	pub default_provider: String,
	pub model_preferences: ModelPreferences,
	pub workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPreferences {
	pub default: Option<String>,
	pub code: Option<String>,
	pub chat: Option<String>,
	pub small: Option<String>,
	pub large: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderConfig {
	OpenAi(OpenAiConfig),
	Anthropic(AnthropicConfig),
	Ollama(OllamaConfig),
	Custom(GenericProviderConfig),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
	pub api_key: Option<SecretString>,
	pub base_url: String,
	pub default_model: String,
	pub organization: Option<String>,
}

impl std::fmt::Debug for OpenAiConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("OpenAiConfig")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.field("organization", &self.organization)
			.finish()
	}
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
	pub api_key: Option<SecretString>,
	pub base_url: String,
	pub default_model: String,
}

impl std::fmt::Debug for AnthropicConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AnthropicConfig")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.finish()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
	pub host: String,
	pub default_model: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GenericProviderConfig {
	pub api_key: Option<SecretString>,
	pub base_url: String,
	pub default_model: Option<String>,
	pub extra: HashMap<String, String>,
}

impl std::fmt::Debug for GenericProviderConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GenericProviderConfig")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.field("extra", &self.extra)
			.finish()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
	pub max_file_size_bytes: u64,
	#[serde(with = "humantime_serde")]
	pub command_timeout: Duration,
	pub allow_shell: bool,
	pub workspace: WorkspaceConfig,
}

mod humantime_serde {
	use serde::{self, Deserialize, Deserializer, Serializer};
	use std::time::Duration;

	pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_u64(duration.as_secs())
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
	where
		D: Deserializer<'de>,
	{
		let secs = u64::deserialize(deserializer)?;
		Ok(Duration::from_secs(secs))
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfig {
	pub root: Option<PathBuf>,
	pub allow_outside_workspace: bool,
	pub allowed_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
	pub level: LogLevel,
	pub file: Option<PathBuf>,
	pub format: LogFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
	Error,
	Warn,
	#[default]
	Info,
	Debug,
	Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
	#[default]
	Pretty,
	Json,
	Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
	pub max_attempts: u32,
	#[serde(with = "humantime_serde")]
	pub base_delay: Duration,
	#[serde(with = "humantime_serde")]
	pub max_delay: Duration,
	pub backoff_factor: f64,
	pub jitter: bool,
}

impl Default for RetryConfig {
	fn default() -> Self {
		Self {
			max_attempts: 3,
			base_delay: Duration::from_millis(500),
			max_delay: Duration::from_secs(30),
			backoff_factor: 2.0,
			jitter: true,
		}
	}
}

impl Default for ToolsConfig {
	fn default() -> Self {
		Self {
			max_file_size_bytes: 1024 * 1024, // 1MB
			command_timeout: Duration::from_secs(300),
			allow_shell: true,
			workspace: WorkspaceConfig::default(),
		}
	}
}

impl Default for LoggingConfig {
	fn default() -> Self {
		Self {
			level: LogLevel::Info,
			file: None,
			format: LogFormat::Pretty,
		}
	}
}

impl Default for GlobalConfig {
	fn default() -> Self {
		Self {
			default_provider: "anthropic".to_string(),
			model_preferences: ModelPreferences::default(),
			workspace_root: None,
		}
	}
}

impl LoomConfig {
	/// Build runtime config from a merged layer and paths.
	pub fn from_layer(layer: ConfigLayer, paths: PathsConfig) -> Result<Self, ConfigError> {
		let global = build_global_config(layer.global)?;
		let providers = build_providers_config(layer.providers)?;
		let tools = build_tools_config(layer.tools);
		let logging = build_logging_config(layer.logging);
		let retry = build_retry_config(layer.retry);

		Ok(Self {
			global,
			providers,
			tools,
			logging,
			retry,
			paths,
		})
	}

	/// Get provider config by name.
	pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
		self.providers.get(name)
	}

	/// Get the default provider config.
	pub fn default_provider(&self) -> Option<&ProviderConfig> {
		self.get_provider(&self.global.default_provider)
	}
}

fn build_global_config(layer: Option<GlobalLayer>) -> Result<GlobalConfig, ConfigError> {
	let layer = layer.unwrap_or_default();
	Ok(GlobalConfig {
		default_provider: layer
			.default_provider
			.unwrap_or_else(|| "anthropic".to_string()),
		model_preferences: layer
			.model_preferences
			.map(|mp| ModelPreferences {
				default: mp.default,
				code: mp.code,
				chat: mp.chat,
				small: mp.small,
				large: mp.large,
			})
			.unwrap_or_default(),
		workspace_root: layer.workspace_root,
	})
}

fn build_providers_config(
	layer: Option<ProvidersLayer>,
) -> Result<HashMap<String, ProviderConfig>, ConfigError> {
	let mut providers = HashMap::new();

	if let Some(pl) = layer {
		for (name, provider_layer) in pl.entries {
			let config = match provider_layer {
				ProviderLayer::OpenAi(l) => ProviderConfig::OpenAi(OpenAiConfig {
					api_key: l.api_key,
					base_url: l
						.base_url
						.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
					default_model: l.default_model.unwrap_or_else(|| "gpt-4o".to_string()),
					organization: l.organization,
				}),
				ProviderLayer::Anthropic(l) => ProviderConfig::Anthropic(AnthropicConfig {
					api_key: l.api_key,
					base_url: l
						.base_url
						.unwrap_or_else(|| "https://api.anthropic.com".to_string()),
					default_model: l
						.default_model
						.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
				}),
				ProviderLayer::Ollama(l) => ProviderConfig::Ollama(OllamaConfig {
					host: l
						.host
						.unwrap_or_else(|| "http://localhost:11434".to_string()),
					default_model: l.default_model.unwrap_or_else(|| "llama3".to_string()),
				}),
				ProviderLayer::Custom(l) => ProviderConfig::Custom(GenericProviderConfig {
					api_key: l.api_key,
					base_url: l.base_url.unwrap_or_default(),
					default_model: l.default_model,
					extra: l.extra,
				}),
			};
			providers.insert(name, config);
		}
	}

	Ok(providers)
}

fn build_tools_config(layer: Option<ToolsLayer>) -> ToolsConfig {
	let layer = layer.unwrap_or_default();
	let workspace = layer
		.workspace
		.map(|w| WorkspaceConfig {
			root: w.root,
			allow_outside_workspace: w.allow_outside_workspace.unwrap_or(false),
			allowed_paths: w.allowed_paths.unwrap_or_default(),
		})
		.unwrap_or_default();

	ToolsConfig {
		max_file_size_bytes: layer.max_file_size_bytes.unwrap_or(1024 * 1024),
		command_timeout: Duration::from_secs(layer.command_timeout_secs.unwrap_or(300)),
		allow_shell: layer.allow_shell.unwrap_or(true),
		workspace,
	}
}

fn build_logging_config(layer: Option<LoggingLayer>) -> LoggingConfig {
	let layer = layer.unwrap_or_default();
	LoggingConfig {
		level: parse_log_level(layer.level.as_deref()),
		file: layer.file,
		format: parse_log_format(layer.format.as_deref()),
	}
}

fn parse_log_level(s: Option<&str>) -> LogLevel {
	match s {
		Some("error") => LogLevel::Error,
		Some("warn") => LogLevel::Warn,
		Some("info") => LogLevel::Info,
		Some("debug") => LogLevel::Debug,
		Some("trace") => LogLevel::Trace,
		_ => LogLevel::Info,
	}
}

fn parse_log_format(s: Option<&str>) -> LogFormat {
	match s {
		Some("json") => LogFormat::Json,
		Some("compact") => LogFormat::Compact,
		Some("pretty") => LogFormat::Pretty,
		_ => LogFormat::Pretty,
	}
}

fn build_retry_config(layer: Option<RetryLayer>) -> RetryConfig {
	let layer = layer.unwrap_or_default();
	RetryConfig {
		max_attempts: layer.max_attempts.unwrap_or(3),
		base_delay: Duration::from_millis(layer.base_delay_ms.unwrap_or(500)),
		max_delay: Duration::from_millis(layer.max_delay_ms.unwrap_or(30_000)),
		backoff_factor: layer.backoff_factor.unwrap_or(2.0),
		jitter: layer.jitter.unwrap_or(true),
	}
}
