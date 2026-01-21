// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Z.ai (智谱AI/ZhipuAI) API types and conversions.

use loom_common_core::{LlmRequest, LlmResponse, Message, Role, ToolCall, ToolDefinition, Usage};
use serde::{Deserialize, Serialize};

/// Configuration for Z.ai client.
#[derive(Debug, Clone)]
pub struct ZaiConfig {
	pub api_key: String,
	pub base_url: String,
	pub model: String,
}

impl ZaiConfig {
	pub fn new(api_key: impl Into<String>) -> Self {
		Self {
			api_key: api_key.into(),
			base_url: "https://api.z.ai/api/paas/v4".to_string(),
			model: "glm-4.7".to_string(),
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

/// Z.ai chat completion request (OpenAI-compatible).
#[derive(Debug, Clone, Serialize)]
pub struct ZaiRequest {
	pub model: String,
	pub messages: Vec<ZaiMessage>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_tokens: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f32>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub tools: Vec<ZaiTool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_choice: Option<String>,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub stream: bool,
}

/// Z.ai message format (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZaiMessage {
	pub role: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub content: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_call_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<ZaiToolCall>>,
}

/// Z.ai tool call (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZaiToolCall {
	pub id: String,
	#[serde(rename = "type")]
	pub call_type: String,
	pub function: ZaiFunctionCall,
}

/// Z.ai function call details (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZaiFunctionCall {
	pub name: String,
	pub arguments: String,
}

/// Z.ai tool definition (OpenAI-compatible).
#[derive(Debug, Clone, Serialize)]
pub struct ZaiTool {
	#[serde(rename = "type")]
	pub tool_type: String,
	pub function: ZaiFunction,
}

/// Z.ai function definition (OpenAI-compatible).
#[derive(Debug, Clone, Serialize)]
pub struct ZaiFunction {
	pub name: String,
	pub description: String,
	pub parameters: serde_json::Value,
}

/// Z.ai chat completion response (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiResponse {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<ZaiChoice>,
	#[serde(default)]
	pub usage: Option<ZaiUsage>,
}

/// Z.ai response choice (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiChoice {
	pub index: u32,
	pub message: ZaiMessage,
	pub finish_reason: Option<String>,
}

/// Z.ai usage statistics (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiUsage {
	pub prompt_tokens: u32,
	pub completion_tokens: u32,
	pub total_tokens: u32,
}

/// Z.ai streaming chunk (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiStreamChunk {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<ZaiStreamChoice>,
	#[serde(default)]
	pub usage: Option<ZaiUsage>,
}

/// Z.ai streaming choice (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiStreamChoice {
	pub index: u32,
	pub delta: ZaiDelta,
	pub finish_reason: Option<String>,
}

/// Z.ai streaming delta (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ZaiDelta {
	#[serde(default)]
	pub role: Option<String>,
	#[serde(default)]
	pub content: Option<String>,
	#[serde(default)]
	pub tool_calls: Option<Vec<ZaiToolCallDelta>>,
}

/// Z.ai streaming tool call delta (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiToolCallDelta {
	pub index: u32,
	#[serde(default)]
	pub id: Option<String>,
	#[serde(rename = "type", default)]
	pub call_type: Option<String>,
	#[serde(default)]
	pub function: Option<ZaiFunctionDelta>,
}

/// Z.ai streaming function delta (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ZaiFunctionDelta {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub arguments: Option<String>,
}

/// Z.ai API error response (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiError {
	pub error: ZaiErrorDetail,
}

/// Z.ai error details (OpenAI-compatible).
#[derive(Debug, Clone, Deserialize)]
pub struct ZaiErrorDetail {
	pub message: String,
	#[serde(rename = "type")]
	pub error_type: Option<String>,
	pub code: Option<String>,
}

impl From<&Message> for ZaiMessage {
	fn from(msg: &Message) -> Self {
		let role = match msg.role {
			Role::System => "system",
			Role::User => "user",
			Role::Assistant => "assistant",
			Role::Tool => "tool",
		};

		Self {
			role: role.to_string(),
			content: if msg.content.is_empty() {
				None
			} else {
				Some(msg.content.clone())
			},
			name: msg.name.clone(),
			tool_call_id: msg.tool_call_id.clone(),
			tool_calls: None,
		}
	}
}

impl From<&ToolDefinition> for ZaiTool {
	fn from(tool: &ToolDefinition) -> Self {
		Self {
			tool_type: "function".to_string(),
			function: ZaiFunction {
				name: tool.name.clone(),
				description: tool.description.clone(),
				parameters: tool.input_schema.clone(),
			},
		}
	}
}

impl ZaiRequest {
	pub fn from_llm_request(request: &LlmRequest, stream: bool) -> Self {
		Self {
			model: request.model.clone(),
			messages: request.messages.iter().map(ZaiMessage::from).collect(),
			max_tokens: request.max_tokens,
			temperature: request.temperature,
			tools: request.tools.iter().map(ZaiTool::from).collect(),
			tool_choice: if request.tools.is_empty() {
				None
			} else {
				Some("auto".to_string())
			},
			stream,
		}
	}
}

impl From<ZaiResponse> for LlmResponse {
	fn from(response: ZaiResponse) -> Self {
		let choice = response.choices.first();

		let (content, tool_calls, finish_reason) = match choice {
			Some(c) => {
				let content = c.message.content.clone().unwrap_or_default();
				let tool_calls = c
					.message
					.tool_calls
					.as_ref()
					.map(|tcs| {
						tcs
							.iter()
							.map(|tc| ToolCall {
								id: tc.id.clone(),
								tool_name: tc.function.name.clone(),
								arguments_json: serde_json::from_str(&tc.function.arguments)
									.unwrap_or(serde_json::Value::Null),
							})
							.collect()
					})
					.unwrap_or_default();
				let finish_reason = c.finish_reason.clone();
				(content, tool_calls, finish_reason)
			}
			None => (String::new(), Vec::new(), None),
		};

		let usage = response.usage.map(|u| Usage {
			input_tokens: u.prompt_tokens,
			output_tokens: u.completion_tokens,
		});

		Self {
			message: Message::assistant(content),
			tool_calls,
			usage,
			finish_reason,
		}
	}
}
