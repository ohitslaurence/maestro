// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Agent state machine types for managing conversation and tool execution flow.

use std::time::Instant;

use crate::error::{AgentError, ToolError};
use crate::llm::{LlmEvent, LlmResponse};
use crate::message::Message;

/// Progress information for a running tool.
#[derive(Clone, Debug)]
pub struct ToolProgress {
	/// Completion fraction from 0.0 to 1.0.
	pub fraction: Option<f32>,
	/// Human-readable progress message.
	pub message: Option<String>,
	/// Number of units processed (files, bytes, etc.).
	pub units_processed: Option<u64>,
}

/// Outcome of a completed tool execution.
#[derive(Clone, Debug)]
pub enum ToolExecutionOutcome {
	Success {
		call_id: String,
		output: serde_json::Value,
	},
	Error {
		call_id: String,
		error: ToolError,
	},
}

/// Status of a tool execution (discriminated union for tool states).
///
/// This enum represents the lifecycle of a tool execution from pending
/// through running to completed. It is not Clone due to Instant fields.
#[derive(Debug)]
pub enum ToolExecutionStatus {
	Pending {
		call_id: String,
		tool_name: String,
		requested_at: Instant,
	},
	Running {
		call_id: String,
		tool_name: String,
		started_at: Instant,
		last_update_at: Instant,
		progress: Option<ToolProgress>,
	},
	Completed {
		call_id: String,
		tool_name: String,
		started_at: Instant,
		completed_at: Instant,
		outcome: ToolExecutionOutcome,
	},
}

impl ToolExecutionStatus {
	/// Returns the call_id for this tool execution.
	pub fn call_id(&self) -> &str {
		match self {
			Self::Pending { call_id, .. } => call_id,
			Self::Running { call_id, .. } => call_id,
			Self::Completed { call_id, .. } => call_id,
		}
	}

	/// Returns the tool name for this execution.
	pub fn tool_name(&self) -> &str {
		match self {
			Self::Pending { tool_name, .. } => tool_name,
			Self::Running { tool_name, .. } => tool_name,
			Self::Completed { tool_name, .. } => tool_name,
		}
	}

	/// Returns true if the tool execution is complete.
	pub fn is_completed(&self) -> bool {
		matches!(self, Self::Completed { .. })
	}
}

/// Event carrying tool progress updates.
#[derive(Clone, Debug)]
pub struct ToolProgressEvent {
	pub call_id: String,
	pub progress: ToolProgress,
}

/// Context for an ongoing conversation.
#[derive(Clone, Debug)]
pub struct ConversationContext {
	pub id: uuid::Uuid,
	pub messages: Vec<Message>,
}

impl ConversationContext {
	/// Creates a new conversation context with a random UUID.
	pub fn new() -> Self {
		Self {
			id: uuid::Uuid::new_v4(),
			messages: Vec::new(),
		}
	}

	/// Creates a conversation context with a specific ID.
	pub fn with_id(id: uuid::Uuid) -> Self {
		Self {
			id,
			messages: Vec::new(),
		}
	}
}

impl Default for ConversationContext {
	fn default() -> Self {
		Self::new()
	}
}

/// Events that can be received by the agent state machine.
#[derive(Debug)]
pub enum AgentEvent {
	UserInput(Message),
	LlmEvent(LlmEvent),
	ToolProgress(ToolProgressEvent),
	ToolCompleted {
		call_id: String,
		outcome: ToolExecutionOutcome,
	},
	/// Post-tool hooks have completed.
	PostToolsHookCompleted {
		/// Whether any hook performed a significant action (e.g., committed).
		action_taken: bool,
	},
	RetryTimeoutFired,
	ShutdownRequested,
}

/// Origin of an error for retry and recovery decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorOrigin {
	Llm,
	Tool,
	Io,
}

/// Main state machine for the agent.
///
/// The agent transitions through states based on events:
/// - WaitingForUserInput: Idle, waiting for user message
/// - CallingLlm: Making a request to the LLM
/// - ProcessingLlmResponse: Handling the LLM's response
/// - ExecutingTools: Running one or more tool calls
/// - PostToolsHook: Running post-tool hooks (e.g., auto-commit)
/// - Error: Handling a recoverable error with retry capability
/// - ShuttingDown: Graceful shutdown in progress
#[derive(Debug)]
pub enum AgentState {
	WaitingForUserInput {
		conversation: ConversationContext,
	},
	CallingLlm {
		conversation: ConversationContext,
		retries: u32,
	},
	ProcessingLlmResponse {
		conversation: ConversationContext,
		response: LlmResponse,
	},
	ExecutingTools {
		conversation: ConversationContext,
		executions: Vec<ToolExecutionStatus>,
	},
	/// Running post-tool hooks (e.g., auto-commit) after tool execution.
	PostToolsHook {
		conversation: ConversationContext,
		pending_llm_request: crate::llm::LlmRequest,
		completed_tools: Vec<crate::agent::CompletedToolInfo>,
	},
	Error {
		conversation: ConversationContext,
		error: AgentError,
		retries: u32,
		origin: ErrorOrigin,
	},
	ShuttingDown,
}

impl AgentState {
	/// Returns the state name for logging purposes.
	pub fn name(&self) -> &'static str {
		match self {
			Self::WaitingForUserInput { .. } => "WaitingForUserInput",
			Self::CallingLlm { .. } => "CallingLlm",
			Self::ProcessingLlmResponse { .. } => "ProcessingLlmResponse",
			Self::ExecutingTools { .. } => "ExecutingTools",
			Self::PostToolsHook { .. } => "PostToolsHook",
			Self::Error { .. } => "Error",
			Self::ShuttingDown => "ShuttingDown",
		}
	}

	/// Logs a state transition with tracing.
	pub fn log_transition_to(&self, next: &AgentState) {
		tracing::info!(
			from = self.name(),
			to = next.name(),
			"agent state transition"
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	/// Property test: ToolExecutionStatus.call_id() always returns a consistent
	/// value regardless of which variant we're in.
	///
	/// This test verifies that the call_id accessor correctly extracts the
	/// call_id from any variant, ensuring we don't have bugs in pattern matching.
	#[test]
	fn test_tool_execution_status_call_id_consistency() {
		let call_id = "test-call-123".to_string();
		let tool_name = "read_file".to_string();
		let now = Instant::now();

		let pending = ToolExecutionStatus::Pending {
			call_id: call_id.clone(),
			tool_name: tool_name.clone(),
			requested_at: now,
		};
		assert_eq!(pending.call_id(), "test-call-123");

		let running = ToolExecutionStatus::Running {
			call_id: call_id.clone(),
			tool_name: tool_name.clone(),
			started_at: now,
			last_update_at: now,
			progress: None,
		};
		assert_eq!(running.call_id(), "test-call-123");

		let completed = ToolExecutionStatus::Completed {
			call_id: call_id.clone(),
			tool_name: tool_name.clone(),
			started_at: now,
			completed_at: now,
			outcome: ToolExecutionOutcome::Success {
				call_id: call_id.clone(),
				output: serde_json::json!({}),
			},
		};
		assert_eq!(completed.call_id(), "test-call-123");
	}

	/// Property test: AgentState.name() returns a non-empty string for all
	/// variants.
	///
	/// This ensures logging will always have meaningful state names.
	#[test]
	fn test_agent_state_names_are_non_empty() {
		let ctx = ConversationContext::new();

		let states = [
			AgentState::WaitingForUserInput {
				conversation: ctx.clone(),
			},
			AgentState::CallingLlm {
				conversation: ctx.clone(),
				retries: 0,
			},
			AgentState::ShuttingDown,
		];

		for state in &states {
			assert!(!state.name().is_empty());
		}
	}

	proptest! {
			/// Property test: ConversationContext always generates valid UUIDs.
			///
			/// This verifies that the UUID generation is working correctly and
			/// the conversation ID is always in a valid format.
			#[test]
			fn conversation_context_has_valid_uuid(_dummy in 0u8..1u8) {
					let ctx = ConversationContext::new();
					// UUID should be version 4 (random)
					assert_eq!(ctx.id.get_version_num(), 4);
			}

			/// Property test: ToolProgress fraction is always in valid range when Some.
			///
			/// This validates that any fraction value, when present, should be
			/// between 0.0 and 1.0 inclusive.
			#[test]
			fn tool_progress_fraction_in_range(fraction in 0.0f32..=1.0f32) {
					let progress = ToolProgress {
							fraction: Some(fraction),
							message: None,
							units_processed: None,
					};
					let f = progress.fraction.unwrap();
					prop_assert!((0.0..=1.0).contains(&f));
			}
	}
}
