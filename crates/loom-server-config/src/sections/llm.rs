// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! LLM configuration section.

use std::path::PathBuf;

use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Available LLM providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
	#[default]
	Anthropic,
	OpenAi,
	Vertex,
}

impl std::fmt::Display for LlmProvider {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LlmProvider::Anthropic => write!(f, "anthropic"),
			LlmProvider::OpenAi => write!(f, "openai"),
			LlmProvider::Vertex => write!(f, "vertex"),
		}
	}
}

impl std::str::FromStr for LlmProvider {
	type Err = ConfigError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().as_str() {
			"anthropic" => Ok(LlmProvider::Anthropic),
			"openai" => Ok(LlmProvider::OpenAi),
			"vertex" => Ok(LlmProvider::Vertex),
			_ => Err(ConfigError::InvalidValue {
				key: "provider".to_string(),
				message: format!("unknown provider '{s}', expected 'anthropic', 'openai', or 'vertex'"),
			}),
		}
	}
}

/// Anthropic authentication configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicAuthConfig {
	ApiKey(SecretString),
	OAuthPool {
		credential_file: PathBuf,
		cooldown_secs: u64,
	},
}

impl std::fmt::Debug for AnthropicAuthConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AnthropicAuthConfig::ApiKey(key) => f.debug_tuple("ApiKey").field(key).finish(),
			AnthropicAuthConfig::OAuthPool {
				credential_file,
				cooldown_secs,
			} => f
				.debug_struct("OAuthPool")
				.field("credential_file", credential_file)
				.field("cooldown_secs", cooldown_secs)
				.finish(),
		}
	}
}

/// LLM configuration layer (for merging).
///
/// All fields are optional to support layered configuration from
/// multiple sources (defaults, files, environment).
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct LlmConfigLayer {
	pub provider: Option<LlmProvider>,
	pub anthropic_auth: Option<AnthropicAuthConfig>,
	pub anthropic_model: Option<String>,
	pub openai_api_key: Option<SecretString>,
	pub openai_model: Option<String>,
	pub openai_organization: Option<String>,
	pub vertex_project: Option<String>,
	pub vertex_location: Option<String>,
	pub vertex_model: Option<String>,
}

impl std::fmt::Debug for LlmConfigLayer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LlmConfigLayer")
			.field("provider", &self.provider)
			.field("anthropic_auth", &self.anthropic_auth)
			.field("anthropic_model", &self.anthropic_model)
			.field("openai_api_key", &self.openai_api_key)
			.field("openai_model", &self.openai_model)
			.field("openai_organization", &self.openai_organization)
			.field("vertex_project", &self.vertex_project)
			.field("vertex_location", &self.vertex_location)
			.field("vertex_model", &self.vertex_model)
			.finish()
	}
}

impl LlmConfigLayer {
	/// Merges another layer on top of this one.
	/// Values from `other` take precedence when present.
	pub fn merge(&mut self, other: LlmConfigLayer) {
		if other.provider.is_some() {
			self.provider = other.provider;
		}
		if other.anthropic_auth.is_some() {
			self.anthropic_auth = other.anthropic_auth;
		}
		if other.anthropic_model.is_some() {
			self.anthropic_model = other.anthropic_model;
		}
		if other.openai_api_key.is_some() {
			self.openai_api_key = other.openai_api_key;
		}
		if other.openai_model.is_some() {
			self.openai_model = other.openai_model;
		}
		if other.openai_organization.is_some() {
			self.openai_organization = other.openai_organization;
		}
		if other.vertex_project.is_some() {
			self.vertex_project = other.vertex_project;
		}
		if other.vertex_location.is_some() {
			self.vertex_location = other.vertex_location;
		}
		if other.vertex_model.is_some() {
			self.vertex_model = other.vertex_model;
		}
	}

	/// Resolves this layer into a runtime configuration.
	pub fn finalize(self) -> LlmConfig {
		let provider = self.provider.unwrap_or_default();

		let anthropic_auth = self.anthropic_auth.map(|auth| match auth {
			AnthropicAuthConfig::ApiKey(key) => AnthropicAuth::ApiKey(key),
			AnthropicAuthConfig::OAuthPool {
				credential_file,
				cooldown_secs,
			} => AnthropicAuth::OAuthPool(AnthropicOAuthPool {
				credential_file,
				cooldown_secs,
			}),
		});

		let openai = self.openai_api_key.map(|api_key| OpenAiConfig {
			api_key,
			model: self.openai_model.unwrap_or_else(|| "gpt-4".to_string()),
			organization: self.openai_organization,
		});

		let vertex = if self.vertex_project.is_some() && self.vertex_location.is_some() {
			Some(VertexConfig {
				project: self.vertex_project.unwrap(),
				location: self.vertex_location.unwrap(),
				model: self
					.vertex_model
					.unwrap_or_else(|| "gemini-1.5-pro".to_string()),
			})
		} else {
			None
		};

		LlmConfig {
			provider,
			anthropic_auth,
			anthropic_model: self.anthropic_model,
			openai,
			vertex,
		}
	}
}

/// Anthropic OAuth pool configuration (runtime).
#[derive(Clone)]
pub struct AnthropicOAuthPool {
	pub credential_file: PathBuf,
	pub cooldown_secs: u64,
}

impl std::fmt::Debug for AnthropicOAuthPool {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AnthropicOAuthPool")
			.field("credential_file", &self.credential_file)
			.field("cooldown_secs", &self.cooldown_secs)
			.finish()
	}
}

/// Anthropic provider authentication (runtime).
#[derive(Clone)]
pub enum AnthropicAuth {
	ApiKey(SecretString),
	OAuthPool(AnthropicOAuthPool),
}

impl std::fmt::Debug for AnthropicAuth {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AnthropicAuth::ApiKey(key) => f.debug_tuple("ApiKey").field(key).finish(),
			AnthropicAuth::OAuthPool(pool) => f.debug_tuple("OAuthPool").field(pool).finish(),
		}
	}
}

/// OpenAI provider configuration (runtime).
#[derive(Clone)]
pub struct OpenAiConfig {
	pub api_key: SecretString,
	pub model: String,
	pub organization: Option<String>,
}

impl std::fmt::Debug for OpenAiConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("OpenAiConfig")
			.field("api_key", &self.api_key)
			.field("model", &self.model)
			.field("organization", &self.organization)
			.finish()
	}
}

/// Vertex AI provider configuration (runtime).
#[derive(Clone, Debug)]
pub struct VertexConfig {
	pub project: String,
	pub location: String,
	pub model: String,
}

/// LLM configuration (runtime, resolved).
#[derive(Clone, Default)]
pub struct LlmConfig {
	pub provider: LlmProvider,
	pub anthropic_auth: Option<AnthropicAuth>,
	pub anthropic_model: Option<String>,
	pub openai: Option<OpenAiConfig>,
	pub vertex: Option<VertexConfig>,
}

impl std::fmt::Debug for LlmConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LlmConfig")
			.field("provider", &self.provider)
			.field("anthropic_auth", &self.anthropic_auth)
			.field("anthropic_model", &self.anthropic_model)
			.field("openai", &self.openai)
			.field("vertex", &self.vertex)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_config::Secret;
	use proptest::prelude::*;

	mod llm_provider {
		use super::*;

		#[test]
		fn parsing_is_case_insensitive() {
			assert_eq!(
				"anthropic".parse::<LlmProvider>().unwrap(),
				LlmProvider::Anthropic
			);
			assert_eq!(
				"ANTHROPIC".parse::<LlmProvider>().unwrap(),
				LlmProvider::Anthropic
			);
			assert_eq!(
				"openai".parse::<LlmProvider>().unwrap(),
				LlmProvider::OpenAi
			);
			assert_eq!(
				"OPENAI".parse::<LlmProvider>().unwrap(),
				LlmProvider::OpenAi
			);
			assert_eq!(
				"vertex".parse::<LlmProvider>().unwrap(),
				LlmProvider::Vertex
			);
			assert_eq!(
				"VERTEX".parse::<LlmProvider>().unwrap(),
				LlmProvider::Vertex
			);
		}

		#[test]
		fn invalid_provider_returns_error() {
			let result = "invalid".parse::<LlmProvider>();
			assert!(result.is_err());
		}

		proptest! {
			#[test]
			fn display_and_parse_roundtrip(provider in prop_oneof![
				Just(LlmProvider::Anthropic),
				Just(LlmProvider::OpenAi),
				Just(LlmProvider::Vertex),
			]) {
				let displayed = provider.to_string();
				let parsed: LlmProvider = displayed.parse().unwrap();
				prop_assert_eq!(provider, parsed);
			}
		}
	}

	mod llm_config_layer {
		use super::*;

		#[test]
		fn merge_provider_override() {
			let mut base = LlmConfigLayer {
				provider: Some(LlmProvider::Anthropic),
				..Default::default()
			};
			let overlay = LlmConfigLayer {
				provider: Some(LlmProvider::OpenAi),
				..Default::default()
			};

			base.merge(overlay);
			assert_eq!(base.provider, Some(LlmProvider::OpenAi));
		}

		#[test]
		fn merge_preserves_base_when_overlay_is_none() {
			let mut base = LlmConfigLayer {
				provider: Some(LlmProvider::Anthropic),
				anthropic_auth: Some(AnthropicAuthConfig::ApiKey(Secret::new(
					"base-key".to_string(),
				))),
				anthropic_model: Some("claude-3".to_string()),
				..Default::default()
			};
			let overlay = LlmConfigLayer::default();

			base.merge(overlay);
			assert_eq!(base.provider, Some(LlmProvider::Anthropic));
			assert!(base.anthropic_auth.is_some());
			match &base.anthropic_auth {
				Some(AnthropicAuthConfig::ApiKey(key)) => assert_eq!(key.expose(), "base-key"),
				_ => panic!("Expected ApiKey"),
			}
		}

		#[test]
		fn merge_anthropic_fields_individually() {
			let mut base = LlmConfigLayer {
				anthropic_auth: Some(AnthropicAuthConfig::ApiKey(Secret::new(
					"base-key".to_string(),
				))),
				anthropic_model: Some("claude-2".to_string()),
				..Default::default()
			};
			let overlay = LlmConfigLayer {
				anthropic_model: Some("claude-3".to_string()),
				..Default::default()
			};

			base.merge(overlay);
			match &base.anthropic_auth {
				Some(AnthropicAuthConfig::ApiKey(key)) => assert_eq!(key.expose(), "base-key"),
				_ => panic!("Expected ApiKey"),
			}
			assert_eq!(base.anthropic_model, Some("claude-3".to_string()));
		}

		#[test]
		fn finalize_default_provider() {
			let layer = LlmConfigLayer::default();
			let config = layer.finalize();
			assert_eq!(config.provider, LlmProvider::Anthropic);
		}

		#[test]
		fn finalize_anthropic_api_key() {
			let layer = LlmConfigLayer {
				provider: Some(LlmProvider::Anthropic),
				anthropic_auth: Some(AnthropicAuthConfig::ApiKey(Secret::new(
					"test-key".to_string(),
				))),
				anthropic_model: Some("claude-3".to_string()),
				..Default::default()
			};

			let config = layer.finalize();
			assert!(matches!(
				config.anthropic_auth,
				Some(AnthropicAuth::ApiKey(_))
			));
			assert_eq!(config.anthropic_model, Some("claude-3".to_string()));
		}

		#[test]
		fn finalize_anthropic_oauth_pool() {
			let layer = LlmConfigLayer {
				provider: Some(LlmProvider::Anthropic),
				anthropic_auth: Some(AnthropicAuthConfig::OAuthPool {
					credential_file: PathBuf::from("/path/to/creds"),
					cooldown_secs: 3600,
				}),
				..Default::default()
			};

			let config = layer.finalize();
			match config.anthropic_auth {
				Some(AnthropicAuth::OAuthPool(pool)) => {
					assert_eq!(pool.credential_file, PathBuf::from("/path/to/creds"));
					assert_eq!(pool.cooldown_secs, 3600);
				}
				_ => panic!("Expected OAuthPool"),
			}
		}

		#[test]
		fn finalize_openai() {
			let layer = LlmConfigLayer {
				provider: Some(LlmProvider::OpenAi),
				openai_api_key: Some(Secret::new("openai-key".to_string())),
				openai_model: Some("gpt-4-turbo".to_string()),
				openai_organization: Some("org-123".to_string()),
				..Default::default()
			};

			let config = layer.finalize();
			assert!(config.openai.is_some());
			let openai = config.openai.unwrap();
			assert_eq!(openai.api_key.expose(), "openai-key");
			assert_eq!(openai.model, "gpt-4-turbo");
			assert_eq!(openai.organization, Some("org-123".to_string()));
		}

		#[test]
		fn finalize_vertex() {
			let layer = LlmConfigLayer {
				provider: Some(LlmProvider::Vertex),
				vertex_project: Some("my-project".to_string()),
				vertex_location: Some("us-central1".to_string()),
				vertex_model: Some("gemini-1.5-flash".to_string()),
				..Default::default()
			};

			let config = layer.finalize();
			assert!(config.vertex.is_some());
			let vertex = config.vertex.unwrap();
			assert_eq!(vertex.project, "my-project");
			assert_eq!(vertex.location, "us-central1");
			assert_eq!(vertex.model, "gemini-1.5-flash");
		}
	}

	mod debug_redaction {
		use super::*;

		#[test]
		fn debug_redacts_api_keys() {
			let layer = LlmConfigLayer {
				anthropic_auth: Some(AnthropicAuthConfig::ApiKey(Secret::new(
					"sk-ant-super-secret".to_string(),
				))),
				openai_api_key: Some(Secret::new("sk-openai-super-secret".to_string())),
				..Default::default()
			};

			let debug_output = format!("{layer:?}");
			assert!(!debug_output.contains("sk-ant-super-secret"));
			assert!(!debug_output.contains("sk-openai-super-secret"));
			assert!(debug_output.contains("[REDACTED]"));
		}

		#[test]
		fn resolved_config_debug_redacts_api_keys() {
			let layer = LlmConfigLayer {
				anthropic_auth: Some(AnthropicAuthConfig::ApiKey(Secret::new(
					"sk-ant-super-secret".to_string(),
				))),
				openai_api_key: Some(Secret::new("sk-openai-super-secret".to_string())),
				openai_model: Some("gpt-4".to_string()),
				..Default::default()
			};

			let config = layer.finalize();
			let debug_output = format!("{config:?}");
			assert!(!debug_output.contains("sk-ant-super-secret"));
			assert!(!debug_output.contains("sk-openai-super-secret"));
			assert!(debug_output.contains("[REDACTED]"));
		}
	}
}
