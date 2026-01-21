// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Partial configuration layer for merging from multiple sources.

use loom_common_config::SecretString;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Partial configuration layer - all fields are Option for merging.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigLayer {
	#[serde(default)]
	pub global: Option<GlobalLayer>,
	#[serde(default)]
	pub providers: Option<ProvidersLayer>,
	#[serde(default)]
	pub tools: Option<ToolsLayer>,
	#[serde(default)]
	pub logging: Option<LoggingLayer>,
	#[serde(default)]
	pub retry: Option<RetryLayer>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GlobalLayer {
	#[serde(default)]
	pub default_provider: Option<String>,
	#[serde(default)]
	pub model_preferences: Option<ModelPreferencesLayer>,
	#[serde(default)]
	pub workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelPreferencesLayer {
	#[serde(default)]
	pub default: Option<String>,
	#[serde(default)]
	pub code: Option<String>,
	#[serde(default)]
	pub chat: Option<String>,
	#[serde(default)]
	pub small: Option<String>,
	#[serde(default)]
	pub large: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProvidersLayer {
	#[serde(flatten)]
	pub entries: HashMap<String, ProviderLayer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderLayer {
	OpenAi(OpenAiLayer),
	Anthropic(AnthropicLayer),
	Ollama(OllamaLayer),
	Custom(GenericProviderLayer),
}

#[derive(Clone, Default, Deserialize)]
pub struct OpenAiLayer {
	#[serde(default)]
	pub api_key: Option<SecretString>,
	#[serde(default)]
	pub base_url: Option<String>,
	#[serde(default)]
	pub default_model: Option<String>,
	#[serde(default)]
	pub organization: Option<String>,
}

impl std::fmt::Debug for OpenAiLayer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("OpenAiLayer")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.field("organization", &self.organization)
			.finish()
	}
}

#[derive(Clone, Default, Deserialize)]
pub struct AnthropicLayer {
	#[serde(default)]
	pub api_key: Option<SecretString>,
	#[serde(default)]
	pub base_url: Option<String>,
	#[serde(default)]
	pub default_model: Option<String>,
}

impl std::fmt::Debug for AnthropicLayer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AnthropicLayer")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.finish()
	}
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OllamaLayer {
	#[serde(default)]
	pub host: Option<String>,
	#[serde(default)]
	pub default_model: Option<String>,
}

#[derive(Clone, Default, Deserialize)]
pub struct GenericProviderLayer {
	#[serde(default)]
	pub api_key: Option<SecretString>,
	#[serde(default)]
	pub base_url: Option<String>,
	#[serde(default)]
	pub default_model: Option<String>,
	#[serde(default)]
	pub extra: HashMap<String, String>,
}

impl std::fmt::Debug for GenericProviderLayer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GenericProviderLayer")
			.field("api_key", &self.api_key)
			.field("base_url", &self.base_url)
			.field("default_model", &self.default_model)
			.field("extra", &self.extra)
			.finish()
	}
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolsLayer {
	#[serde(default)]
	pub max_file_size_bytes: Option<u64>,
	#[serde(default)]
	pub command_timeout_secs: Option<u64>,
	#[serde(default)]
	pub allow_shell: Option<bool>,
	#[serde(default)]
	pub workspace: Option<WorkspaceLayer>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkspaceLayer {
	#[serde(default)]
	pub root: Option<PathBuf>,
	#[serde(default)]
	pub allow_outside_workspace: Option<bool>,
	#[serde(default)]
	pub allowed_paths: Option<Vec<PathBuf>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LoggingLayer {
	#[serde(default)]
	pub level: Option<String>,
	#[serde(default)]
	pub file: Option<PathBuf>,
	#[serde(default)]
	pub format: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RetryLayer {
	#[serde(default)]
	pub max_attempts: Option<u32>,
	#[serde(default)]
	pub base_delay_ms: Option<u64>,
	#[serde(default)]
	pub max_delay_ms: Option<u64>,
	#[serde(default)]
	pub backoff_factor: Option<f64>,
	#[serde(default)]
	pub jitter: Option<bool>,
}

impl ConfigLayer {
	/// Merge another layer into this one. Other layer takes precedence.
	pub fn merge(&mut self, other: ConfigLayer) {
		merge_option(&mut self.global, other.global, GlobalLayer::merge);
		merge_option(&mut self.providers, other.providers, ProvidersLayer::merge);
		merge_option(&mut self.tools, other.tools, ToolsLayer::merge);
		merge_option(&mut self.logging, other.logging, LoggingLayer::merge);
		merge_option(&mut self.retry, other.retry, RetryLayer::merge);
	}
}

fn merge_option<T, F>(target: &mut Option<T>, source: Option<T>, merge_fn: F)
where
	F: FnOnce(&mut T, T),
{
	match (target.as_mut(), source) {
		(Some(t), Some(s)) => merge_fn(t, s),
		(None, Some(s)) => *target = Some(s),
		_ => {}
	}
}

impl GlobalLayer {
	fn merge(&mut self, other: GlobalLayer) {
		if other.default_provider.is_some() {
			self.default_provider = other.default_provider;
		}
		if other.workspace_root.is_some() {
			self.workspace_root = other.workspace_root;
		}
		merge_option(
			&mut self.model_preferences,
			other.model_preferences,
			|t, s| {
				if s.default.is_some() {
					t.default = s.default;
				}
				if s.code.is_some() {
					t.code = s.code;
				}
				if s.chat.is_some() {
					t.chat = s.chat;
				}
				if s.small.is_some() {
					t.small = s.small;
				}
				if s.large.is_some() {
					t.large = s.large;
				}
			},
		);
	}
}

impl ProvidersLayer {
	fn merge(&mut self, other: ProvidersLayer) {
		for (name, provider) in other.entries {
			self.entries.insert(name, provider);
		}
	}
}

impl ToolsLayer {
	fn merge(&mut self, other: ToolsLayer) {
		if other.max_file_size_bytes.is_some() {
			self.max_file_size_bytes = other.max_file_size_bytes;
		}
		if other.command_timeout_secs.is_some() {
			self.command_timeout_secs = other.command_timeout_secs;
		}
		if other.allow_shell.is_some() {
			self.allow_shell = other.allow_shell;
		}
		merge_option(&mut self.workspace, other.workspace, |t, s| {
			if s.root.is_some() {
				t.root = s.root;
			}
			if s.allow_outside_workspace.is_some() {
				t.allow_outside_workspace = s.allow_outside_workspace;
			}
			if s.allowed_paths.is_some() {
				t.allowed_paths = s.allowed_paths;
			}
		});
	}
}

impl LoggingLayer {
	fn merge(&mut self, other: LoggingLayer) {
		if other.level.is_some() {
			self.level = other.level;
		}
		if other.file.is_some() {
			self.file = other.file;
		}
		if other.format.is_some() {
			self.format = other.format;
		}
	}
}

impl RetryLayer {
	fn merge(&mut self, other: RetryLayer) {
		if other.max_attempts.is_some() {
			self.max_attempts = other.max_attempts;
		}
		if other.base_delay_ms.is_some() {
			self.base_delay_ms = other.base_delay_ms;
		}
		if other.max_delay_ms.is_some() {
			self.max_delay_ms = other.max_delay_ms;
		}
		if other.backoff_factor.is_some() {
			self.backoff_factor = other.backoff_factor;
		}
		if other.jitter.is_some() {
			self.jitter = other.jitter;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests that when merging two ConfigLayers, fields from the second layer
	/// (source) take precedence over fields from the first layer (target).
	/// This property is essential for layered config where later sources
	/// (e.g., CLI args) override earlier ones (e.g., config files).
	#[test]
	fn test_merge_precedence_overwrites_existing_values() {
		let mut base = ConfigLayer {
			global: Some(GlobalLayer {
				default_provider: Some("openai".to_string()),
				workspace_root: Some(PathBuf::from("/base")),
				model_preferences: None,
			}),
			..Default::default()
		};

		let overlay = ConfigLayer {
			global: Some(GlobalLayer {
				default_provider: Some("anthropic".to_string()),
				workspace_root: None,
				model_preferences: None,
			}),
			..Default::default()
		};

		base.merge(overlay);

		let global = base.global.unwrap();
		assert_eq!(global.default_provider, Some("anthropic".to_string()));
		assert_eq!(global.workspace_root, Some(PathBuf::from("/base")));
	}

	/// Tests that merging preserves values from the base layer when the
	/// overlay layer has None for those fields. This ensures partial
	/// configs don't accidentally clear existing settings.
	#[test]
	fn test_merge_preserves_base_when_overlay_is_none() {
		let mut base = ConfigLayer {
			logging: Some(LoggingLayer {
				level: Some("debug".to_string()),
				file: Some(PathBuf::from("/var/log/app.log")),
				format: Some("json".to_string()),
			}),
			..Default::default()
		};

		let overlay = ConfigLayer {
			logging: Some(LoggingLayer {
				level: Some("info".to_string()),
				file: None,
				format: None,
			}),
			..Default::default()
		};

		base.merge(overlay);

		let logging = base.logging.unwrap();
		assert_eq!(logging.level, Some("info".to_string()));
		assert_eq!(logging.file, Some(PathBuf::from("/var/log/app.log")));
		assert_eq!(logging.format, Some("json".to_string()));
	}

	/// Tests that merging an empty layer into a populated layer preserves
	/// all existing values. This is critical for ensuring default/empty
	/// config sources don't wipe out valid configuration.
	#[test]
	fn test_merge_empty_layer_preserves_all() {
		let mut base = ConfigLayer {
			retry: Some(RetryLayer {
				max_attempts: Some(5),
				base_delay_ms: Some(100),
				max_delay_ms: Some(5000),
				backoff_factor: Some(2.0),
				jitter: Some(true),
			}),
			..Default::default()
		};

		let empty = ConfigLayer::default();
		base.merge(empty);

		let retry = base.retry.unwrap();
		assert_eq!(retry.max_attempts, Some(5));
		assert_eq!(retry.base_delay_ms, Some(100));
		assert_eq!(retry.max_delay_ms, Some(5000));
		assert_eq!(retry.backoff_factor, Some(2.0));
		assert_eq!(retry.jitter, Some(true));
	}

	/// Tests that when the base layer is empty/default, merging an overlay
	/// populates it correctly. This verifies the "first source sets values"
	/// behavior in the config loading chain.
	#[test]
	fn test_merge_into_empty_base() {
		let mut base = ConfigLayer::default();

		let overlay = ConfigLayer {
			tools: Some(ToolsLayer {
				max_file_size_bytes: Some(1024 * 1024),
				command_timeout_secs: Some(30),
				allow_shell: Some(false),
				workspace: None,
			}),
			..Default::default()
		};

		base.merge(overlay);

		let tools = base.tools.unwrap();
		assert_eq!(tools.max_file_size_bytes, Some(1024 * 1024));
		assert_eq!(tools.command_timeout_secs, Some(30));
		assert_eq!(tools.allow_shell, Some(false));
	}

	/// Tests nested struct merging for model preferences. Ensures that
	/// individual preference fields can be overridden independently without
	/// affecting sibling fields.
	#[test]
	fn test_merge_nested_model_preferences() {
		let mut base = ConfigLayer {
			global: Some(GlobalLayer {
				default_provider: None,
				workspace_root: None,
				model_preferences: Some(ModelPreferencesLayer {
					default: Some("gpt-4".to_string()),
					code: Some("gpt-4".to_string()),
					chat: Some("gpt-3.5-turbo".to_string()),
					small: None,
					large: None,
				}),
			}),
			..Default::default()
		};

		let overlay = ConfigLayer {
			global: Some(GlobalLayer {
				default_provider: None,
				workspace_root: None,
				model_preferences: Some(ModelPreferencesLayer {
					default: None,
					code: Some("claude-3-opus".to_string()),
					chat: None,
					small: Some("claude-3-haiku".to_string()),
					large: None,
				}),
			}),
			..Default::default()
		};

		base.merge(overlay);

		let prefs = base.global.unwrap().model_preferences.unwrap();
		assert_eq!(prefs.default, Some("gpt-4".to_string()));
		assert_eq!(prefs.code, Some("claude-3-opus".to_string()));
		assert_eq!(prefs.chat, Some("gpt-3.5-turbo".to_string()));
		assert_eq!(prefs.small, Some("claude-3-haiku".to_string()));
		assert_eq!(prefs.large, None);
	}

	/// Tests that provider entries are replaced entirely when the same
	/// provider name exists in both layers. This is the expected behavior
	/// for HashMap-based merging where keys collide.
	#[test]
	fn test_merge_providers_replaces_by_name() {
		use loom_common_config::Secret;

		let mut base = ConfigLayer {
			providers: Some(ProvidersLayer {
				entries: HashMap::from([(
					"main".to_string(),
					ProviderLayer::OpenAi(OpenAiLayer {
						api_key: Some(Secret::new("key1".to_string())),
						base_url: None,
						default_model: Some("gpt-4".to_string()),
						organization: None,
					}),
				)]),
			}),
			..Default::default()
		};

		let overlay = ConfigLayer {
			providers: Some(ProvidersLayer {
				entries: HashMap::from([(
					"main".to_string(),
					ProviderLayer::Anthropic(AnthropicLayer {
						api_key: Some(Secret::new("key2".to_string())),
						base_url: None,
						default_model: Some("claude-3-opus".to_string()),
					}),
				)]),
			}),
			..Default::default()
		};

		base.merge(overlay);

		let providers = base.providers.unwrap();
		assert!(matches!(
			providers.entries.get("main"),
			Some(ProviderLayer::Anthropic(_))
		));
	}

	/// Tests multi-layer merging simulating real-world config loading:
	/// defaults -> config file -> environment -> CLI args.
	/// Each successive layer should override previous values.
	#[test]
	fn test_multi_layer_merge_chain() {
		let mut config = ConfigLayer::default();

		let defaults = ConfigLayer {
			retry: Some(RetryLayer {
				max_attempts: Some(3),
				base_delay_ms: Some(100),
				max_delay_ms: Some(10000),
				backoff_factor: Some(2.0),
				jitter: Some(true),
			}),
			logging: Some(LoggingLayer {
				level: Some("info".to_string()),
				file: None,
				format: Some("text".to_string()),
			}),
			..Default::default()
		};

		let config_file = ConfigLayer {
			retry: Some(RetryLayer {
				max_attempts: Some(5),
				base_delay_ms: None,
				max_delay_ms: None,
				backoff_factor: None,
				jitter: None,
			}),
			logging: Some(LoggingLayer {
				level: Some("debug".to_string()),
				file: Some(PathBuf::from("/var/log/app.log")),
				format: None,
			}),
			..Default::default()
		};

		let cli_args = ConfigLayer {
			logging: Some(LoggingLayer {
				level: Some("trace".to_string()),
				file: None,
				format: None,
			}),
			..Default::default()
		};

		config.merge(defaults);
		config.merge(config_file);
		config.merge(cli_args);

		let retry = config.retry.unwrap();
		assert_eq!(retry.max_attempts, Some(5));
		assert_eq!(retry.base_delay_ms, Some(100));
		assert_eq!(retry.backoff_factor, Some(2.0));

		let logging = config.logging.unwrap();
		assert_eq!(logging.level, Some("trace".to_string()));
		assert_eq!(logging.file, Some(PathBuf::from("/var/log/app.log")));
		assert_eq!(logging.format, Some("text".to_string()));
	}

	/// Tests workspace nested struct merging to ensure allowed_paths and
	/// other workspace settings merge correctly at the nested level.
	#[test]
	fn test_merge_nested_workspace_settings() {
		let mut base = ConfigLayer {
			tools: Some(ToolsLayer {
				max_file_size_bytes: None,
				command_timeout_secs: None,
				allow_shell: Some(true),
				workspace: Some(WorkspaceLayer {
					root: Some(PathBuf::from("/project")),
					allow_outside_workspace: Some(false),
					allowed_paths: Some(vec![PathBuf::from("/tmp")]),
				}),
			}),
			..Default::default()
		};

		let overlay = ConfigLayer {
			tools: Some(ToolsLayer {
				max_file_size_bytes: Some(2048),
				command_timeout_secs: None,
				allow_shell: None,
				workspace: Some(WorkspaceLayer {
					root: None,
					allow_outside_workspace: Some(true),
					allowed_paths: None,
				}),
			}),
			..Default::default()
		};

		base.merge(overlay);

		let tools = base.tools.unwrap();
		assert_eq!(tools.max_file_size_bytes, Some(2048));
		assert_eq!(tools.allow_shell, Some(true));

		let workspace = tools.workspace.unwrap();
		assert_eq!(workspace.root, Some(PathBuf::from("/project")));
		assert_eq!(workspace.allow_outside_workspace, Some(true));
		assert_eq!(workspace.allowed_paths, Some(vec![PathBuf::from("/tmp")]));
	}
}
