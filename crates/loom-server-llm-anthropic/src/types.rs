// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Anthropic-specific API types and conversions.

use loom_cli_credentials::{CredentialStore, MemoryCredentialStore};
use loom_common_core::{LlmError, LlmRequest, LlmResponse, Message, Role, ToolCall, Usage};
use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::{AnthropicAuth, OAuthClient, OAuthCredentials};

/// Configuration for the Anthropic client.
#[derive(Debug)]
pub struct AnthropicConfig<S: CredentialStore = MemoryCredentialStore> {
	pub auth: AnthropicAuth<S>,
	pub base_url: String,
	pub model: String,
}

impl<S: CredentialStore> Clone for AnthropicConfig<S> {
	fn clone(&self) -> Self {
		Self {
			auth: self.auth.clone(),
			base_url: self.base_url.clone(),
			model: self.model.clone(),
		}
	}
}

impl AnthropicConfig<MemoryCredentialStore> {
	/// Create a new config with an API key (legacy interface).
	pub fn new(api_key: impl Into<String>) -> Self {
		Self::new_with_api_key(api_key)
	}

	/// Create a new config with an API key.
	pub fn new_with_api_key(api_key: impl Into<String>) -> Self {
		Self {
			auth: AnthropicAuth::api_key(api_key),
			base_url: "https://api.anthropic.com".to_string(),
			model: "claude-sonnet-4-20250514".to_string(),
		}
	}
}

impl<S: CredentialStore> AnthropicConfig<S> {
	/// Create a new config with OAuth authentication.
	pub fn new_with_oauth(
		provider_id: impl Into<String>,
		refresh: impl Into<String>,
		access: impl Into<String>,
		expires: u64,
		store: Arc<S>,
	) -> Self {
		let creds = OAuthCredentials::new(
			SecretString::new(refresh.into()),
			SecretString::new(access.into()),
			expires,
		);

		Self {
			auth: AnthropicAuth::OAuth {
				client: OAuthClient::new(provider_id, creds, store),
			},
			base_url: "https://api.anthropic.com".to_string(),
			model: "claude-sonnet-4-20250514".to_string(),
		}
	}

	/// Create a new config with a pre-built auth.
	pub fn new_with_auth(auth: AnthropicAuth<S>) -> Self {
		Self {
			auth,
			base_url: "https://api.anthropic.com".to_string(),
			model: "claude-sonnet-4-20250514".to_string(),
		}
	}

	pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
		self.base_url = base_url.into();
		self
	}

	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}
}

/// Anthropic Messages API request.
#[derive(Debug, Clone, Serialize)]
pub struct AnthropicRequest {
	pub model: String,
	pub messages: Vec<AnthropicMessage>,
	pub max_tokens: u32,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub system: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tools: Option<Vec<AnthropicTool>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stream: Option<bool>,
}

impl AnthropicRequest {
	/// Prepend the required OAuth system prompt prefix for Opus/Sonnet models.
	///
	/// Anthropic validates OAuth requests to ensure they come from legitimate coding
	/// assistant tools. The system prompt MUST start with the magic phrase for OAuth
	/// tokens to work with premium models like Opus and Sonnet.
	///
	/// This method prepends the required prefix to any existing system prompt,
	/// or sets it if no system prompt exists.
	///
	/// Haiku models work without this prefix, but calling this method is harmless.
	pub fn with_oauth_system_prompt(mut self) -> Self {
		use crate::auth::OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX;

		self.system = Some(match self.system {
			Some(existing) => {
				// Only prepend if not already present
				if existing.starts_with(OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX) {
					existing
				} else {
					format!("{OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX} {existing}")
				}
			}
			None => OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX.to_string(),
		});
		self
	}
}

/// A message in the Anthropic conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
	pub role: String,
	pub content: AnthropicMessageContent,
}

/// Content can be a string or a list of content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicMessageContent {
	Text(String),
	Blocks(Vec<AnthropicContent>),
}

/// A content block in the message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicContent {
	#[serde(rename = "text")]
	Text { text: String },
	#[serde(rename = "tool_use")]
	ToolUse(AnthropicToolUse),
	#[serde(rename = "tool_result")]
	ToolResult(AnthropicToolResult),
}

/// Tool use block from the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicToolUse {
	pub id: String,
	pub name: String,
	pub input: serde_json::Value,
}

/// Tool result block from the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicToolResult {
	pub tool_use_id: String,
	pub content: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub is_error: Option<bool>,
}

/// Tool definition for Anthropic API.
#[derive(Debug, Clone, Serialize)]
pub struct AnthropicTool {
	pub name: String,
	pub description: String,
	pub input_schema: serde_json::Value,
}

/// Response from Anthropic Messages API.
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicResponse {
	pub id: String,
	#[serde(rename = "type")]
	pub response_type: String,
	pub role: String,
	pub content: Vec<AnthropicResponseContent>,
	pub model: String,
	pub stop_reason: Option<String>,
	pub usage: AnthropicUsage,
}

/// Content block in the response.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicResponseContent {
	#[serde(rename = "text")]
	Text { text: String },
	#[serde(rename = "tool_use")]
	ToolUse {
		id: String,
		name: String,
		input: serde_json::Value,
	},
}

/// Usage statistics from the response.
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicUsage {
	pub input_tokens: u32,
	pub output_tokens: u32,
}

/// Error response from Anthropic API.
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicError {
	#[serde(rename = "type")]
	pub error_type: String,
	pub error: AnthropicErrorDetail,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicErrorDetail {
	#[serde(rename = "type")]
	pub error_type: String,
	pub message: String,
}

impl From<&LlmRequest> for AnthropicRequest {
	fn from(req: &LlmRequest) -> Self {
		let mut system = None;
		let mut messages = Vec::new();

		for msg in &req.messages {
			match msg.role {
				Role::System => {
					system = Some(msg.content.clone());
				}
				Role::User => {
					messages.push(AnthropicMessage {
						role: "user".to_string(),
						content: AnthropicMessageContent::Text(msg.content.clone()),
					});
				}
				Role::Assistant => {
					if msg.tool_calls.is_empty() {
						messages.push(AnthropicMessage {
							role: "assistant".to_string(),
							content: AnthropicMessageContent::Text(msg.content.clone()),
						});
					} else {
						let mut blocks: Vec<AnthropicContent> = Vec::new();
						if !msg.content.is_empty() {
							blocks.push(AnthropicContent::Text {
								text: msg.content.clone(),
							});
						}
						for tc in &msg.tool_calls {
							blocks.push(AnthropicContent::ToolUse(AnthropicToolUse {
								id: tc.id.clone(),
								name: tc.tool_name.clone(),
								input: tc.arguments_json.clone(),
							}));
						}
						messages.push(AnthropicMessage {
							role: "assistant".to_string(),
							content: AnthropicMessageContent::Blocks(blocks),
						});
					}
				}
				Role::Tool => {
					if let Some(tool_call_id) = &msg.tool_call_id {
						messages.push(AnthropicMessage {
							role: "user".to_string(),
							content: AnthropicMessageContent::Blocks(vec![AnthropicContent::ToolResult(
								AnthropicToolResult {
									tool_use_id: tool_call_id.clone(),
									content: msg.content.clone(),
									is_error: None,
								},
							)]),
						});
					}
				}
			}
		}

		let tools = if req.tools.is_empty() {
			None
		} else {
			Some(
				req
					.tools
					.iter()
					.map(|t| AnthropicTool {
						name: t.name.clone(),
						description: t.description.clone(),
						input_schema: t.input_schema.clone(),
					})
					.collect(),
			)
		};

		AnthropicRequest {
			model: req.model.clone(),
			messages,
			max_tokens: req.max_tokens.unwrap_or(4096),
			system,
			temperature: req.temperature,
			tools,
			stream: None,
		}
	}
}

impl TryFrom<AnthropicResponse> for LlmResponse {
	type Error = LlmError;

	fn try_from(resp: AnthropicResponse) -> Result<Self, Self::Error> {
		let mut content = String::new();
		let mut tool_calls = Vec::new();

		for block in resp.content {
			match block {
				AnthropicResponseContent::Text { text } => {
					content.push_str(&text);
				}
				AnthropicResponseContent::ToolUse { id, name, input } => {
					tool_calls.push(ToolCall {
						id,
						tool_name: name,
						arguments_json: input,
					});
				}
			}
		}

		Ok(LlmResponse {
			message: Message::assistant(content),
			tool_calls,
			finish_reason: resp.stop_reason,
			usage: Some(Usage {
				input_tokens: resp.usage.input_tokens,
				output_tokens: resp.usage.output_tokens,
			}),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_config_defaults() {
		let config = AnthropicConfig::new("test-key");
		assert_eq!(config.base_url, "https://api.anthropic.com");
		assert_eq!(config.model, "claude-sonnet-4-20250514");
		assert!(config.auth.is_api_key());
	}

	#[test]
	fn test_config_builder() {
		let config = AnthropicConfig::new("test-key")
			.with_base_url("http://localhost:8080")
			.with_model("claude-3-opus");
		assert_eq!(config.base_url, "http://localhost:8080");
		assert_eq!(config.model, "claude-3-opus");
	}

	#[test]
	fn test_config_with_api_key() {
		let config = AnthropicConfig::new_with_api_key("sk-test");
		assert!(config.auth.is_api_key());
		assert!(!config.auth.is_oauth());
	}

	#[test]
	fn test_config_with_oauth() {
		let store = Arc::new(MemoryCredentialStore::new());
		let config =
			AnthropicConfig::new_with_oauth("anthropic", "rt_refresh", "at_access", 1735500000000, store);
		assert!(config.auth.is_oauth());
		assert!(!config.auth.is_api_key());
	}

	#[test]
	fn test_oauth_system_prompt_sets_when_none() {
		use crate::auth::OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX;

		let request = AnthropicRequest {
			model: "claude-opus-4-5".to_string(),
			messages: vec![],
			max_tokens: 4096,
			system: None,
			temperature: None,
			tools: None,
			stream: None,
		};

		let updated = request.with_oauth_system_prompt();
		assert_eq!(
			updated.system,
			Some(OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX.to_string())
		);
	}

	#[test]
	fn test_oauth_system_prompt_prepends_to_existing() {
		use crate::auth::OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX;

		let request = AnthropicRequest {
			model: "claude-opus-4-5".to_string(),
			messages: vec![],
			max_tokens: 4096,
			system: Some("You are a helpful assistant.".to_string()),
			temperature: None,
			tools: None,
			stream: None,
		};

		let updated = request.with_oauth_system_prompt();
		let expected = format!("{OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX} You are a helpful assistant.");
		assert_eq!(updated.system, Some(expected));
	}

	#[test]
	fn test_oauth_system_prompt_does_not_duplicate() {
		use crate::auth::OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX;

		let existing = format!("{OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX} You are a helpful assistant.");
		let request = AnthropicRequest {
			model: "claude-opus-4-5".to_string(),
			messages: vec![],
			max_tokens: 4096,
			system: Some(existing.clone()),
			temperature: None,
			tools: None,
			stream: None,
		};

		let updated = request.with_oauth_system_prompt();
		assert_eq!(updated.system, Some(existing));
	}
}
