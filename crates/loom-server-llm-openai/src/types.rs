// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! OpenAI-specific API types and conversions.

use loom_common_core::{LlmRequest, LlmResponse, Message, Role, ToolCall, ToolDefinition, Usage};
use serde::{Deserialize, Serialize};

/// Configuration for the OpenAI client.
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
	pub api_key: String,
	pub base_url: String,
	pub model: String,
	pub organization: Option<String>,
}

impl OpenAIConfig {
	pub fn new(api_key: impl Into<String>) -> Self {
		Self {
			api_key: api_key.into(),
			base_url: "https://api.openai.com/v1".to_string(),
			model: "gpt-4o".to_string(),
			organization: None,
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

	pub fn with_organization(mut self, org: impl Into<String>) -> Self {
		self.organization = Some(org.into());
		self
	}
}

/// OpenAI chat completion request.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIRequest {
	pub model: String,
	pub messages: Vec<OpenAIMessage>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_tokens: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f32>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub tools: Vec<OpenAITool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_choice: Option<String>,
	#[serde(skip_serializing_if = "std::ops::Not::not")]
	pub stream: bool,
}

/// OpenAI message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
	pub role: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub content: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_call_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<OpenAIToolCall>>,
}

/// OpenAI tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
	pub id: String,
	#[serde(rename = "type")]
	pub call_type: String,
	pub function: OpenAIFunctionCall,
}

/// OpenAI function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
	pub name: String,
	pub arguments: String,
}

/// OpenAI tool definition.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAITool {
	#[serde(rename = "type")]
	pub tool_type: String,
	pub function: OpenAIFunction,
}

/// OpenAI function definition.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIFunction {
	pub name: String,
	pub description: String,
	pub parameters: serde_json::Value,
}

/// OpenAI chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIResponse {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<OpenAIChoice>,
	#[serde(default)]
	pub usage: Option<OpenAIUsage>,
}

/// OpenAI response choice.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIChoice {
	pub index: u32,
	pub message: OpenAIMessage,
	pub finish_reason: Option<String>,
}

/// OpenAI usage statistics.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIUsage {
	pub prompt_tokens: u32,
	pub completion_tokens: u32,
	pub total_tokens: u32,
}

/// OpenAI streaming chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIStreamChunk {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<OpenAIStreamChoice>,
	#[serde(default)]
	pub usage: Option<OpenAIUsage>,
}

/// OpenAI streaming choice.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIStreamChoice {
	pub index: u32,
	pub delta: OpenAIDelta,
	pub finish_reason: Option<String>,
}

/// OpenAI streaming delta.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OpenAIDelta {
	#[serde(default)]
	pub role: Option<String>,
	#[serde(default)]
	pub content: Option<String>,
	#[serde(default)]
	pub tool_calls: Option<Vec<OpenAIToolCallDelta>>,
}

/// OpenAI streaming tool call delta.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIToolCallDelta {
	pub index: u32,
	#[serde(default)]
	pub id: Option<String>,
	#[serde(rename = "type", default)]
	pub call_type: Option<String>,
	#[serde(default)]
	pub function: Option<OpenAIFunctionDelta>,
}

/// OpenAI streaming function delta.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OpenAIFunctionDelta {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub arguments: Option<String>,
}

/// OpenAI API error response.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIError {
	pub error: OpenAIErrorDetail,
}

/// OpenAI error details.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIErrorDetail {
	pub message: String,
	#[serde(rename = "type")]
	pub error_type: Option<String>,
	pub code: Option<String>,
}

impl From<&Message> for OpenAIMessage {
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

impl From<&ToolDefinition> for OpenAITool {
	fn from(tool: &ToolDefinition) -> Self {
		Self {
			tool_type: "function".to_string(),
			function: OpenAIFunction {
				name: tool.name.clone(),
				description: tool.description.clone(),
				parameters: tool.input_schema.clone(),
			},
		}
	}
}

impl OpenAIRequest {
	pub fn from_llm_request(request: &LlmRequest, stream: bool) -> Self {
		Self {
			model: request.model.clone(),
			messages: request.messages.iter().map(OpenAIMessage::from).collect(),
			max_tokens: request.max_tokens,
			temperature: request.temperature,
			tools: request.tools.iter().map(OpenAITool::from).collect(),
			tool_choice: if request.tools.is_empty() {
				None
			} else {
				Some("auto".to_string())
			},
			stream,
		}
	}
}

impl From<OpenAIResponse> for LlmResponse {
	fn from(response: OpenAIResponse) -> Self {
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
