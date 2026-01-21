// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Vertex AI-specific API types and conversions.

use loom_common_core::{
	LlmError, LlmRequest, LlmResponse, Message, Role, ToolCall, ToolDefinition, Usage,
};
use serde::{Deserialize, Serialize};

/// Configuration for the Vertex AI client.
#[derive(Debug, Clone)]
pub struct VertexConfig {
	/// GCP project ID.
	pub project_id: String,
	/// GCP region (e.g., "us-central1").
	pub location: String,
	/// Model name (e.g., "gemini-1.5-pro").
	pub model: String,
	/// Base URL for Vertex AI API.
	pub base_url: String,
}

impl VertexConfig {
	/// Creates a new configuration with the specified project and location.
	///
	/// Uses default model "gemini-1.5-pro" and constructs the base URL from location.
	pub fn new(project_id: impl Into<String>, location: impl Into<String>) -> Self {
		let location = location.into();
		let base_url = format!("https://{location}-aiplatform.googleapis.com");
		Self {
			project_id: project_id.into(),
			location,
			model: "gemini-1.5-pro".to_string(),
			base_url,
		}
	}

	/// Sets the model name.
	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}

	/// Sets a custom base URL.
	pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
		self.base_url = base_url.into();
		self
	}
}

/// Vertex AI generateContent request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexRequest {
	/// Conversation contents.
	pub contents: Vec<VertexContent>,
	/// System instruction (optional).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub system_instruction: Option<VertexContent>,
	/// Tool definitions for function calling.
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub tools: Vec<VertexTool>,
	/// Generation configuration.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub generation_config: Option<VertexGenerationConfig>,
}

/// Content in the Vertex conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexContent {
	/// Role: "user", "model", or omitted for system.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub role: Option<String>,
	/// Content parts.
	pub parts: Vec<VertexPart>,
}

/// A part of content (text, function call, or function response).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VertexPart {
	/// Text content.
	Text { text: String },
	/// Function call from the model.
	FunctionCall {
		#[serde(rename = "functionCall")]
		function_call: VertexFunctionCall,
	},
	/// Function response from the user.
	FunctionResponse {
		#[serde(rename = "functionResponse")]
		function_response: VertexFunctionResponse,
	},
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexFunctionCall {
	/// Function name.
	pub name: String,
	/// Function arguments as JSON.
	#[serde(default)]
	pub args: serde_json::Value,
}

/// Function response details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexFunctionResponse {
	/// Function name.
	pub name: String,
	/// Response content.
	pub response: serde_json::Value,
}

/// Tool definitions wrapper.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexTool {
	/// Function declarations.
	pub function_declarations: Vec<VertexFunctionDeclaration>,
}

/// Function declaration for tool definition.
#[derive(Debug, Clone, Serialize)]
pub struct VertexFunctionDeclaration {
	/// Function name.
	pub name: String,
	/// Function description.
	pub description: String,
	/// Parameters JSON schema.
	pub parameters: serde_json::Value,
}

/// Generation configuration.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexGenerationConfig {
	/// Maximum output tokens.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_output_tokens: Option<u32>,
	/// Sampling temperature.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f32>,
}

/// Response from Vertex AI generateContent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexResponse {
	/// Response candidates.
	#[serde(default)]
	pub candidates: Vec<VertexCandidate>,
	/// Usage metadata.
	#[serde(rename = "usageMetadata")]
	pub usage_metadata: Option<VertexUsageMetadata>,
}

/// A response candidate.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexCandidate {
	/// Generated content.
	pub content: VertexContent,
	/// Finish reason (e.g., "STOP", "MAX_TOKENS").
	pub finish_reason: Option<String>,
}

/// Usage metadata from Vertex AI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VertexUsageMetadata {
	/// Prompt token count.
	pub prompt_token_count: u32,
	/// Candidates token count.
	pub candidates_token_count: Option<u32>,
	/// Total token count.
	pub total_token_count: Option<u32>,
}

/// Error response from Vertex AI.
#[derive(Debug, Clone, Deserialize)]
pub struct VertexError {
	/// Error details.
	pub error: VertexErrorDetail,
}

/// Error detail structure.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VertexErrorDetail {
	/// Error message.
	pub message: String,
	/// Error status (e.g., "INVALID_ARGUMENT").
	pub status: Option<String>,
	/// Error code.
	pub code: Option<i32>,
}

impl From<&LlmRequest> for VertexRequest {
	fn from(req: &LlmRequest) -> Self {
		let mut system_instruction = None;
		let mut contents = Vec::new();

		for msg in &req.messages {
			match msg.role {
				Role::System => {
					if system_instruction.is_none() {
						system_instruction = Some(VertexContent {
							role: None,
							parts: vec![VertexPart::Text {
								text: msg.content.clone(),
							}],
						});
					}
				}
				Role::User => {
					contents.push(VertexContent {
						role: Some("user".to_string()),
						parts: vec![VertexPart::Text {
							text: msg.content.clone(),
						}],
					});
				}
				Role::Assistant => {
					contents.push(VertexContent {
						role: Some("model".to_string()),
						parts: vec![VertexPart::Text {
							text: msg.content.clone(),
						}],
					});
				}
				Role::Tool => {
					if let Some(tool_name) = &msg.name {
						let response_value = serde_json::from_str(&msg.content)
							.unwrap_or(serde_json::Value::String(msg.content.clone()));
						contents.push(VertexContent {
							role: Some("user".to_string()),
							parts: vec![VertexPart::FunctionResponse {
								function_response: VertexFunctionResponse {
									name: tool_name.clone(),
									response: response_value,
								},
							}],
						});
					}
				}
			}
		}

		let tools: Vec<VertexTool> = if req.tools.is_empty() {
			Vec::new()
		} else {
			vec![VertexTool {
				function_declarations: req
					.tools
					.iter()
					.map(|t: &ToolDefinition| VertexFunctionDeclaration {
						name: t.name.clone(),
						description: t.description.clone(),
						parameters: t.input_schema.clone(),
					})
					.collect(),
			}]
		};

		let generation_config = if req.max_tokens.is_some() || req.temperature.is_some() {
			Some(VertexGenerationConfig {
				max_output_tokens: req.max_tokens,
				temperature: req.temperature,
			})
		} else {
			None
		};

		VertexRequest {
			contents,
			system_instruction,
			tools,
			generation_config,
		}
	}
}

impl TryFrom<VertexResponse> for LlmResponse {
	type Error = LlmError;

	fn try_from(resp: VertexResponse) -> Result<Self, Self::Error> {
		let candidate = resp
			.candidates
			.first()
			.ok_or_else(|| LlmError::InvalidResponse("Vertex response had no candidates".to_string()))?;

		let mut content = String::new();
		let mut tool_calls = Vec::new();

		for part in &candidate.content.parts {
			match part {
				VertexPart::Text { text } => {
					content.push_str(text);
				}
				VertexPart::FunctionCall { function_call } => {
					let id = format!("vertex_call_{}", tool_calls.len());
					tool_calls.push(ToolCall {
						id,
						tool_name: function_call.name.clone(),
						arguments_json: function_call.args.clone(),
					});
				}
				VertexPart::FunctionResponse { .. } => {
					// Tool result from user; ignore in assistant output
				}
			}
		}

		let usage = resp.usage_metadata.map(|u| Usage {
			input_tokens: u.prompt_token_count,
			output_tokens: u.candidates_token_count.unwrap_or(0),
		});

		Ok(LlmResponse {
			message: Message::assistant(content),
			tool_calls,
			finish_reason: candidate.finish_reason.clone(),
			usage,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn config_defaults() {
		let config = VertexConfig::new("my-project", "us-central1");
		assert_eq!(config.project_id, "my-project");
		assert_eq!(config.location, "us-central1");
		assert_eq!(config.model, "gemini-1.5-pro");
		assert_eq!(
			config.base_url,
			"https://us-central1-aiplatform.googleapis.com"
		);
	}

	#[test]
	fn config_builder() {
		let config = VertexConfig::new("proj", "europe-west1")
			.with_model("gemini-2.0-flash")
			.with_base_url("http://localhost:8080");
		assert_eq!(config.model, "gemini-2.0-flash");
		assert_eq!(config.base_url, "http://localhost:8080");
	}

	#[test]
	fn request_conversion_basic() {
		let request = LlmRequest::new("gemini-1.5-pro").with_messages(vec![Message::user("Hello")]);
		let vertex_req = VertexRequest::from(&request);

		assert_eq!(vertex_req.contents.len(), 1);
		assert!(vertex_req.system_instruction.is_none());
		assert!(vertex_req.tools.is_empty());
	}

	#[test]
	fn request_conversion_with_system() {
		let request = LlmRequest::new("gemini-1.5-pro").with_messages(vec![
			Message::system("You are helpful"),
			Message::user("Hi"),
		]);
		let vertex_req = VertexRequest::from(&request);

		assert!(vertex_req.system_instruction.is_some());
		assert_eq!(vertex_req.contents.len(), 1);
	}

	#[test]
	fn response_conversion() {
		let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello there!"}]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

		let vertex_resp: VertexResponse = serde_json::from_str(json).unwrap();
		let llm_resp = LlmResponse::try_from(vertex_resp).unwrap();

		assert_eq!(llm_resp.message.content, "Hello there!");
		assert_eq!(llm_resp.finish_reason, Some("STOP".to_string()));
		assert!(llm_resp.tool_calls.is_empty());
		assert_eq!(llm_resp.usage.as_ref().unwrap().input_tokens, 10);
		assert_eq!(llm_resp.usage.as_ref().unwrap().output_tokens, 5);
	}

	#[test]
	fn response_with_function_call() {
		let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "get_weather",
                            "args": {"location": "NYC"}
                        }
                    }]
                },
                "finishReason": "STOP"
            }]
        }"#;

		let vertex_resp: VertexResponse = serde_json::from_str(json).unwrap();
		let llm_resp = LlmResponse::try_from(vertex_resp).unwrap();

		assert_eq!(llm_resp.tool_calls.len(), 1);
		assert_eq!(llm_resp.tool_calls[0].tool_name, "get_weather");
		assert_eq!(
			llm_resp.tool_calls[0].arguments_json,
			serde_json::json!({"location": "NYC"})
		);
	}
}
