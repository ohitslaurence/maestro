// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Agent implementation with state machine event handling.

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::config::AgentConfig;
use crate::error::{AgentError, AgentResult};
use crate::llm::{LlmClient, LlmEvent, LlmRequest};
use crate::message::{Message, Role, ToolCall};
use crate::state::{
	AgentEvent, AgentState, ConversationContext, ErrorOrigin, ToolExecutionOutcome,
	ToolExecutionStatus,
};
use crate::tool::ToolDefinition;

/// Information about a completed tool for post-tool hooks.
#[derive(Clone, Debug)]
pub struct CompletedToolInfo {
	pub tool_name: String,
	pub succeeded: bool,
}

/// Actions that the caller should perform in response to state changes.
#[derive(Clone, Debug)]
pub enum AgentAction {
	/// Send a request to the LLM.
	SendLlmRequest(LlmRequest),
	/// Execute the given tool calls.
	ExecuteTools(Vec<ToolCall>),
	/// Run post-tool hooks (auto-commit, etc.).
	RunPostToolsHook {
		completed_tools: Vec<CompletedToolInfo>,
	},
	/// Wait for user input (idle state).
	WaitForInput,
	/// Display a message to the user.
	DisplayMessage(String),
	/// Display an error to the user.
	DisplayError(String),
	/// Shutdown the agent.
	Shutdown,
}

/// Checks if any tool executions are mutating (edit_file or bash) and
/// succeeded.
fn has_mutating_tools(executions: &[ToolExecutionStatus]) -> bool {
	const MUTATING_TOOLS: &[&str] = &["edit_file", "bash"];
	executions.iter().any(|exec| {
		if let ToolExecutionStatus::Completed {
			tool_name, outcome, ..
		} = exec
		{
			MUTATING_TOOLS.contains(&tool_name.as_str())
				&& matches!(outcome, ToolExecutionOutcome::Success { .. })
		} else {
			false
		}
	})
}

/// Extracts completed tool information from execution statuses.
fn extract_completed_tools(executions: &[ToolExecutionStatus]) -> Vec<CompletedToolInfo> {
	executions
		.iter()
		.filter_map(|exec| {
			if let ToolExecutionStatus::Completed {
				tool_name, outcome, ..
			} = exec
			{
				Some(CompletedToolInfo {
					tool_name: tool_name.clone(),
					succeeded: matches!(outcome, ToolExecutionOutcome::Success { .. }),
				})
			} else {
				None
			}
		})
		.collect()
}

/// The main agent struct managing conversation state and LLM interaction.
pub struct Agent {
	state: AgentState,
	config: AgentConfig,
	#[allow(dead_code)]
	llm: Arc<dyn LlmClient>,
	tools: Vec<ToolDefinition>,
}

impl Agent {
	/// Creates a new agent with the given configuration and LLM client.
	pub fn new(config: AgentConfig, llm: Arc<dyn LlmClient>, tools: Vec<ToolDefinition>) -> Self {
		let conversation = ConversationContext::new();
		info!(
				conversation_id = %conversation.id,
				model = %config.model_name,
				tool_count = tools.len(),
				"creating new agent"
		);
		Self {
			state: AgentState::WaitingForUserInput { conversation },
			config,
			llm,
			tools,
		}
	}

	/// Returns a reference to the current state.
	pub fn state(&self) -> &AgentState {
		&self.state
	}

	/// Returns a reference to the conversation context from the current state.
	pub fn conversation(&self) -> &ConversationContext {
		match &self.state {
			AgentState::WaitingForUserInput { conversation } => conversation,
			AgentState::CallingLlm { conversation, .. } => conversation,
			AgentState::ProcessingLlmResponse { conversation, .. } => conversation,
			AgentState::ExecutingTools { conversation, .. } => conversation,
			AgentState::PostToolsHook { conversation, .. } => conversation,
			AgentState::Error { conversation, .. } => conversation,
			AgentState::ShuttingDown => panic!("Cannot get conversation from ShuttingDown state"),
		}
	}

	/// Returns a mutable reference to the conversation context.
	fn get_conversation_mut(&mut self) -> &mut ConversationContext {
		match &mut self.state {
			AgentState::WaitingForUserInput { conversation } => conversation,
			AgentState::CallingLlm { conversation, .. } => conversation,
			AgentState::ProcessingLlmResponse { conversation, .. } => conversation,
			AgentState::ExecutingTools { conversation, .. } => conversation,
			AgentState::PostToolsHook { conversation, .. } => conversation,
			AgentState::Error { conversation, .. } => conversation,
			AgentState::ShuttingDown => panic!("Cannot get conversation from ShuttingDown state"),
		}
	}

	/// Appends a message to the conversation.
	pub fn append_message(&mut self, msg: Message) {
		debug!(role = ?msg.role, "appending message to conversation");
		self.get_conversation_mut().messages.push(msg);
	}

	/// Builds an LLM request from the current conversation.
	pub fn build_llm_request(&self) -> LlmRequest {
		LlmRequest {
			model: self.config.model_name.clone(),
			messages: self.conversation().messages.clone(),
			tools: self.tools.clone(),
			max_tokens: Some(self.config.max_tokens),
			temperature: self.config.temperature,
		}
	}

	/// Handles an event and returns the action the caller should perform.
	pub fn handle_event(&mut self, event: AgentEvent) -> AgentResult<AgentAction> {
		let old_state_name = self.state.name();
		debug!(state = old_state_name, event = ?std::mem::discriminant(&event), "handling event");

		let action = match (&mut self.state, event) {
			// WaitingForUserInput + UserInput -> CallingLlm
			(AgentState::WaitingForUserInput { conversation }, AgentEvent::UserInput(msg)) => {
				conversation.messages.push(msg);
				let request = LlmRequest {
					model: self.config.model_name.clone(),
					messages: conversation.messages.clone(),
					tools: self.tools.clone(),
					max_tokens: Some(self.config.max_tokens),
					temperature: self.config.temperature,
				};
				let new_conversation = conversation.clone();
				self.state = AgentState::CallingLlm {
					conversation: new_conversation,
					retries: 0,
				};
				info!(from = old_state_name, to = "CallingLlm", "state transition");
				AgentAction::SendLlmRequest(request)
			}

			// CallingLlm + LlmEvent::TextDelta -> stay in CallingLlm, display text
			(AgentState::CallingLlm { .. }, AgentEvent::LlmEvent(LlmEvent::TextDelta { content })) => {
				AgentAction::DisplayMessage(content)
			}

			// CallingLlm + LlmEvent::ToolCallDelta -> stay in CallingLlm
			(AgentState::CallingLlm { .. }, AgentEvent::LlmEvent(LlmEvent::ToolCallDelta { .. })) => {
				AgentAction::WaitForInput
			}

			// CallingLlm + LlmEvent::Completed -> ProcessingLlmResponse
			(
				AgentState::CallingLlm { conversation, .. },
				AgentEvent::LlmEvent(LlmEvent::Completed(response)),
			) => {
				let mut conv = conversation.clone();
				conv.messages.push(response.message.clone());
				self.state = AgentState::ProcessingLlmResponse {
					conversation: conv,
					response,
				};
				info!(
					from = old_state_name,
					to = "ProcessingLlmResponse",
					"state transition"
				);
				self.process_llm_response()
			}

			// CallingLlm + LlmEvent::Error -> Error state with retry
			(
				AgentState::CallingLlm {
					conversation,
					retries,
				},
				AgentEvent::LlmEvent(LlmEvent::Error(e)),
			) => {
				let new_retries = *retries + 1;
				warn!(
						error = %e,
						retries = new_retries,
						max_retries = self.config.max_retries,
						"LLM error"
				);
				if new_retries < self.config.max_retries {
					let conv = conversation.clone();
					self.state = AgentState::Error {
						conversation: conv,
						error: AgentError::Llm(e),
						retries: new_retries,
						origin: ErrorOrigin::Llm,
					};
					info!(
						from = old_state_name,
						to = "Error",
						"state transition (will retry)"
					);
					AgentAction::WaitForInput
				} else {
					let conv = conversation.clone();
					self.state = AgentState::WaitingForUserInput { conversation: conv };
					info!(
						from = old_state_name,
						to = "WaitingForUserInput",
						"state transition (max retries)"
					);
					AgentAction::DisplayError(AgentError::Llm(e).to_string())
				}
			}

			// Error + RetryTimeoutFired -> CallingLlm (retry)
			(
				AgentState::Error {
					conversation,
					retries,
					origin: ErrorOrigin::Llm,
					..
				},
				AgentEvent::RetryTimeoutFired,
			) => {
				let request = LlmRequest {
					model: self.config.model_name.clone(),
					messages: conversation.messages.clone(),
					tools: self.tools.clone(),
					max_tokens: Some(self.config.max_tokens),
					temperature: self.config.temperature,
				};
				let new_retries = *retries;
				let conv = conversation.clone();
				self.state = AgentState::CallingLlm {
					conversation: conv,
					retries: new_retries,
				};
				info!(
					from = old_state_name,
					to = "CallingLlm",
					retries = new_retries,
					"retrying LLM request"
				);
				AgentAction::SendLlmRequest(request)
			}

			// ExecutingTools + ToolCompleted -> check if all done
			(
				AgentState::ExecutingTools {
					conversation,
					executions,
				},
				AgentEvent::ToolCompleted { call_id, outcome },
			) => {
				for exec in executions.iter_mut() {
					if exec.call_id() == call_id {
						let tool_name = exec.tool_name().to_string();
						*exec = ToolExecutionStatus::Completed {
							call_id: call_id.clone(),
							tool_name,
							started_at: std::time::Instant::now(),
							completed_at: std::time::Instant::now(),
							outcome: outcome.clone(),
						};
					}
				}

				let all_complete = executions.iter().all(|e| e.is_completed());
				if all_complete {
					let mut tool_messages = Vec::new();
					for exec in executions.iter() {
						if let ToolExecutionStatus::Completed {
							call_id, outcome, ..
						} = exec
						{
							let content = match outcome {
								ToolExecutionOutcome::Success { output, .. } => {
									serde_json::to_string(output).unwrap_or_else(|_| "{}".to_string())
								}
								ToolExecutionOutcome::Error { error, .. } => format!("Error: {error}"),
							};
							tool_messages.push(Message {
								role: Role::Tool,
								content,
								tool_call_id: Some(call_id.clone()),
								name: None,
								tool_calls: Vec::new(),
							});
						}
					}

					let mut conv = conversation.clone();
					conv.messages.extend(tool_messages);
					let request = LlmRequest {
						model: self.config.model_name.clone(),
						messages: conv.messages.clone(),
						tools: self.tools.clone(),
						max_tokens: Some(self.config.max_tokens),
						temperature: self.config.temperature,
					};

					if has_mutating_tools(executions) {
						let completed_tools = extract_completed_tools(executions);
						debug!(
							tool_count = completed_tools.len(),
							"mutating tools detected, running post-tools hook"
						);
						self.state = AgentState::PostToolsHook {
							conversation: conv,
							pending_llm_request: request,
							completed_tools: completed_tools.clone(),
						};
						info!(
							from = old_state_name,
							to = "PostToolsHook",
							"state transition for post-tools hook"
						);
						AgentAction::RunPostToolsHook { completed_tools }
					} else {
						self.state = AgentState::CallingLlm {
							conversation: conv,
							retries: 0,
						};
						info!(
							from = old_state_name,
							to = "CallingLlm",
							"all tools complete, sending results to LLM"
						);
						AgentAction::SendLlmRequest(request)
					}
				} else {
					AgentAction::WaitForInput
				}
			}

			// PostToolsHook + PostToolsHookCompleted -> CallingLlm
			(
				AgentState::PostToolsHook {
					conversation,
					pending_llm_request,
					..
				},
				AgentEvent::PostToolsHookCompleted { action_taken },
			) => {
				debug!(action_taken = action_taken, "post-tools hook completed");
				let conv = conversation.clone();
				let request = pending_llm_request.clone();
				self.state = AgentState::CallingLlm {
					conversation: conv,
					retries: 0,
				};
				info!(
					from = "PostToolsHook",
					to = "CallingLlm",
					"state transition after post-tools hook"
				);
				AgentAction::SendLlmRequest(request)
			}

			// ShutdownRequested from any state
			(_, AgentEvent::ShutdownRequested) => {
				self.state = AgentState::ShuttingDown;
				info!(
					from = old_state_name,
					to = "ShuttingDown",
					"state transition"
				);
				AgentAction::Shutdown
			}

			// Invalid transitions
			(state, event) => {
				warn!(
						state = state.name(),
						event = ?std::mem::discriminant(&event),
						"invalid state transition"
				);
				AgentAction::WaitForInput
			}
		};

		Ok(action)
	}

	/// Process an LLM response and determine the next action.
	fn process_llm_response(&mut self) -> AgentAction {
		let (conversation, response) = match &self.state {
			AgentState::ProcessingLlmResponse {
				conversation,
				response,
			} => (conversation.clone(), response.clone()),
			_ => return AgentAction::WaitForInput,
		};

		if !response.tool_calls.is_empty() {
			let executions: Vec<ToolExecutionStatus> = response
				.tool_calls
				.iter()
				.map(|tc| ToolExecutionStatus::Pending {
					call_id: tc.id.clone(),
					tool_name: tc.tool_name.clone(),
					requested_at: std::time::Instant::now(),
				})
				.collect();

			self.state = AgentState::ExecutingTools {
				conversation,
				executions,
			};
			info!(
				from = "ProcessingLlmResponse",
				to = "ExecutingTools",
				tool_count = response.tool_calls.len(),
				"state transition"
			);
			AgentAction::ExecuteTools(response.tool_calls)
		} else {
			self.state = AgentState::WaitingForUserInput { conversation };
			info!(
				from = "ProcessingLlmResponse",
				to = "WaitingForUserInput",
				"state transition"
			);
			AgentAction::WaitForInput
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::error::LlmError;
	use crate::llm::{LlmResponse, LlmStream};
	use crate::message::Role;
	use proptest::prelude::*;

	/// Mock LLM client for testing agent state transitions.
	///
	/// This mock is intentionally unimplemented as the Agent tests focus on
	/// state machine transitions triggered by events, not actual LLM calls.
	struct MockLlmClient;

	#[async_trait::async_trait]
	impl LlmClient for MockLlmClient {
		async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, LlmError> {
			unimplemented!("MockLlmClient::complete not needed for state machine tests")
		}
		async fn complete_streaming(&self, _request: LlmRequest) -> Result<LlmStream, LlmError> {
			unimplemented!("MockLlmClient::complete_streaming not needed for state machine tests")
		}
	}

	fn create_test_agent() -> Agent {
		let config = AgentConfig::default();
		let llm = Arc::new(MockLlmClient);
		Agent::new(config, llm, vec![])
	}

	fn create_test_agent_with_config(config: AgentConfig) -> Agent {
		let llm = Arc::new(MockLlmClient);
		Agent::new(config, llm, vec![])
	}

	/// Creates a simple LLM response without tool calls for testing.
	fn create_simple_response(content: &str) -> LlmResponse {
		LlmResponse {
			message: Message::assistant(content),
			tool_calls: vec![],
			usage: None,
			finish_reason: Some("stop".to_string()),
		}
	}

	/// Creates an LLM response with tool calls for testing ExecutingTools
	/// transitions.
	fn create_response_with_tools(tool_calls: Vec<ToolCall>) -> LlmResponse {
		LlmResponse {
			message: Message::assistant("I'll use some tools"),
			tool_calls,
			usage: None,
			finish_reason: Some("tool_calls".to_string()),
		}
	}

	/// **Test: Agent starts in WaitingForUserInput state**
	///
	/// This test verifies the fundamental invariant that a newly created agent
	/// is always in the WaitingForUserInput state. This is critical because:
	/// - The agent must be ready to receive user input immediately after creation
	/// - No operations should be in flight for a fresh agent
	/// - The conversation context should be initialized but empty
	#[test]
	fn test_initial_state_is_waiting_for_user_input() {
		let agent = create_test_agent();

		match agent.state() {
			AgentState::WaitingForUserInput { conversation } => {
				assert!(
					conversation.messages.is_empty(),
					"new agent should have empty conversation"
				);
			}
			other => panic!("expected WaitingForUserInput, got {}", other.name()),
		}
	}

	/// **Test: UserInput event triggers transition to CallingLlm with
	/// SendLlmRequest action**
	///
	/// This test verifies the primary user interaction flow:
	/// - When user provides input, agent must transition to CallingLlm
	/// - The action must be SendLlmRequest containing the user's message
	/// - The request should include all configured parameters (model, tools,
	///   etc.)
	///
	/// This is essential because it's the entry point for all agent interactions.
	#[test]
	fn test_user_input_transitions_to_calling_llm() {
		let mut agent = create_test_agent();
		let user_message = Message::user("Hello, agent!");

		let action = agent
			.handle_event(AgentEvent::UserInput(user_message.clone()))
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::CallingLlm {
				conversation,
				retries,
			} => {
				assert_eq!(*retries, 0, "retries should start at 0");
				assert_eq!(
					conversation.messages.len(),
					1,
					"conversation should contain user message"
				);
				assert_eq!(conversation.messages[0].role, Role::User);
				assert_eq!(conversation.messages[0].content, "Hello, agent!");
			}
			other => panic!("expected CallingLlm, got {}", other.name()),
		}

		match action {
			AgentAction::SendLlmRequest(request) => {
				assert_eq!(request.messages.len(), 1);
				assert_eq!(request.messages[0].content, "Hello, agent!");
			}
			other => panic!("expected SendLlmRequest, got {other:?}"),
		}
	}

	/// **Test: TextDelta event keeps agent in CallingLlm with DisplayMessage
	/// action**
	///
	/// This test verifies streaming response handling:
	/// - TextDelta events should NOT cause state transitions
	/// - The agent must emit DisplayMessage actions for UI rendering
	/// - Multiple TextDeltas should be handled without state changes
	///
	/// Critical for ensuring streaming LLM responses display correctly to users.
	#[test]
	fn test_text_delta_stays_in_calling_llm() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::TextDelta {
				content: "Hello".to_string(),
			}))
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::CallingLlm { .. }),
			"should stay in CallingLlm after TextDelta"
		);

		match action {
			AgentAction::DisplayMessage(content) => {
				assert_eq!(content, "Hello");
			}
			other => panic!("expected DisplayMessage, got {other:?}"),
		}
	}

	/// **Test: Completed event transitions to WaitingForUserInput when no tools**
	///
	/// This test verifies the completion flow for simple responses:
	/// - When LLM responds without tool calls, agent should return to idle
	/// - The response message must be appended to conversation history
	/// - Agent should emit WaitForInput action
	///
	/// This is the happy path for conversational interactions without tool use.
	#[test]
	fn test_completed_without_tools_transitions_to_waiting() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		let response = create_simple_response("I'm a helpful assistant!");
		let action = agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Completed(response)))
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::WaitingForUserInput { conversation } => {
				assert_eq!(
					conversation.messages.len(),
					2,
					"should have user + assistant messages"
				);
				assert_eq!(conversation.messages[1].role, Role::Assistant);
			}
			other => panic!("expected WaitingForUserInput, got {}", other.name()),
		}

		assert!(
			matches!(action, AgentAction::WaitForInput),
			"expected WaitForInput action"
		);
	}

	/// **Test: Completed event transitions to ExecutingTools when tools present**
	///
	/// This test verifies the tool execution flow initiation:
	/// - When LLM requests tool calls, agent must transition to ExecutingTools
	/// - The action must be ExecuteTools with all requested tool calls
	/// - Tool executions should be tracked in pending state
	///
	/// Essential for the agent's ability to use tools as requested by the LLM.
	#[test]
	fn test_completed_with_tools_transitions_to_executing_tools() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("read a file")))
			.expect("handle_event should succeed");

		let tool_calls = vec![ToolCall {
			id: "call_123".to_string(),
			tool_name: "read_file".to_string(),
			arguments_json: serde_json::json!({"path": "/test.txt"}),
		}];
		let response = create_response_with_tools(tool_calls.clone());

		let action = agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Completed(response)))
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::ExecutingTools { executions, .. } => {
				assert_eq!(executions.len(), 1);
				assert!(!executions[0].is_completed());
				assert_eq!(executions[0].call_id(), "call_123");
			}
			other => panic!("expected ExecutingTools, got {}", other.name()),
		}

		match action {
			AgentAction::ExecuteTools(calls) => {
				assert_eq!(calls.len(), 1);
				assert_eq!(calls[0].tool_name, "read_file");
			}
			other => panic!("expected ExecuteTools, got {other:?}"),
		}
	}

	/// **Test: LLM error with retries remaining transitions to Error state**
	///
	/// This test verifies error handling with retry capability:
	/// - Errors should transition to Error state when retries remain
	/// - Retry count should be incremented
	/// - Agent should wait for retry timeout (WaitForInput action)
	///
	/// Critical for resilience against transient LLM failures.
	#[test]
	fn test_llm_error_with_retries_transitions_to_error() {
		let mut config = AgentConfig::default();
		config.max_retries = 3;
		let mut agent = create_test_agent_with_config(config);

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout)))
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::Error {
				retries, origin, ..
			} => {
				assert_eq!(*retries, 1, "retry count should be incremented");
				assert_eq!(*origin, ErrorOrigin::Llm);
			}
			other => panic!("expected Error state, got {}", other.name()),
		}

		assert!(
			matches!(action, AgentAction::WaitForInput),
			"should wait for retry timeout"
		);
	}

	/// **Test: LLM error at max retries transitions to WaitingForUserInput with
	/// error display**
	///
	/// This test verifies error handling when retries are exhausted:
	/// - When max_retries is reached, agent should give up and return to idle
	/// - Error must be displayed to user via DisplayError action
	/// - Agent should be ready to accept new user input
	///
	/// Essential for preventing infinite retry loops and informing users of
	/// failures.
	#[test]
	fn test_llm_error_at_max_retries_transitions_to_waiting() {
		let mut config = AgentConfig::default();
		config.max_retries = 2;
		let mut agent = create_test_agent_with_config(config);

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout)))
			.expect("handle_event should succeed");

		agent
			.handle_event(AgentEvent::RetryTimeoutFired)
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Api(
				"server error".to_string(),
			))))
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::WaitingForUserInput { .. }),
			"should return to WaitingForUserInput after max retries"
		);

		match action {
			AgentAction::DisplayError(msg) => {
				assert!(
					msg.contains("API error"),
					"error message should describe the failure"
				);
			}
			other => panic!("expected DisplayError, got {other:?}"),
		}
	}

	/// **Test: RetryTimeoutFired in Error state transitions back to CallingLlm**
	///
	/// This test verifies the retry mechanism:
	/// - RetryTimeoutFired should trigger a retry attempt
	/// - Agent should return to CallingLlm with preserved retry count
	/// - A new SendLlmRequest action should be emitted
	///
	/// Critical for automatic recovery from transient failures.
	#[test]
	fn test_retry_timeout_fired_transitions_to_calling_llm() {
		let mut config = AgentConfig::default();
		config.max_retries = 3;
		let mut agent = create_test_agent_with_config(config);

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout)))
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::Error { retries: 1, .. }),
			"should be in Error state with 1 retry"
		);

		let action = agent
			.handle_event(AgentEvent::RetryTimeoutFired)
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::CallingLlm { retries, .. } => {
				assert_eq!(*retries, 1, "retry count should be preserved");
			}
			other => panic!("expected CallingLlm, got {}", other.name()),
		}

		assert!(
			matches!(action, AgentAction::SendLlmRequest(_)),
			"should emit SendLlmRequest for retry"
		);
	}

	/// **Test: All tools completed transitions to CallingLlm with tool results**
	///
	/// This test verifies the tool completion flow:
	/// - When all tools complete, agent should call LLM with results
	/// - Tool results must be added to conversation as Tool messages
	/// - Agent should transition to CallingLlm to continue the conversation
	///
	/// Essential for the agentic loop where LLM can use tool results.
	#[test]
	fn test_all_tools_completed_transitions_to_calling_llm() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("read a file")))
			.expect("handle_event should succeed");

		let tool_calls = vec![
			ToolCall {
				id: "call_1".to_string(),
				tool_name: "read_file".to_string(),
				arguments_json: serde_json::json!({"path": "/a.txt"}),
			},
			ToolCall {
				id: "call_2".to_string(),
				tool_name: "read_file".to_string(),
				arguments_json: serde_json::json!({"path": "/b.txt"}),
			},
		];
		let response = create_response_with_tools(tool_calls);

		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Completed(response)))
			.expect("handle_event should succeed");

		agent
			.handle_event(AgentEvent::ToolCompleted {
				call_id: "call_1".to_string(),
				outcome: ToolExecutionOutcome::Success {
					call_id: "call_1".to_string(),
					output: serde_json::json!({"content": "file a contents"}),
				},
			})
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::ExecutingTools { .. }),
			"should still be executing tools (one remaining)"
		);

		let action = agent
			.handle_event(AgentEvent::ToolCompleted {
				call_id: "call_2".to_string(),
				outcome: ToolExecutionOutcome::Success {
					call_id: "call_2".to_string(),
					output: serde_json::json!({"content": "file b contents"}),
				},
			})
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::CallingLlm {
				conversation,
				retries,
			} => {
				assert_eq!(*retries, 0, "retries should reset after tools complete");
				let tool_msgs: Vec<_> = conversation
					.messages
					.iter()
					.filter(|m| m.role == Role::Tool)
					.collect();
				assert_eq!(tool_msgs.len(), 2, "should have two tool result messages");
			}
			other => panic!("expected CallingLlm, got {}", other.name()),
		}

		assert!(
			matches!(action, AgentAction::SendLlmRequest(_)),
			"should send LLM request with tool results"
		);
	}

	/// **Test: ShutdownRequested from any state transitions to ShuttingDown**
	///
	/// This test verifies graceful shutdown from all possible states:
	/// - ShutdownRequested must always succeed regardless of current state
	/// - Agent must transition to ShuttingDown state
	/// - Shutdown action must be emitted
	///
	/// Critical for clean application termination and resource cleanup.
	#[test]
	fn test_shutdown_requested_from_waiting_for_input() {
		let mut agent = create_test_agent();

		let action = agent
			.handle_event(AgentEvent::ShutdownRequested)
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::ShuttingDown),
			"should transition to ShuttingDown"
		);
		assert!(
			matches!(action, AgentAction::Shutdown),
			"should emit Shutdown action"
		);
	}

	#[test]
	fn test_shutdown_requested_from_calling_llm() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::ShutdownRequested)
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::ShuttingDown),
			"should transition to ShuttingDown from CallingLlm"
		);
		assert!(matches!(action, AgentAction::Shutdown));
	}

	#[test]
	fn test_shutdown_requested_from_error_state() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("test")))
			.expect("handle_event should succeed");
		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout)))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::ShutdownRequested)
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::ShuttingDown),
			"should transition to ShuttingDown from Error"
		);
		assert!(matches!(action, AgentAction::Shutdown));
	}

	/// **Test: Invalid state transitions are handled gracefully**
	///
	/// This test verifies robustness against invalid event sequences:
	/// - Invalid transitions should not crash the agent
	/// - Agent should remain in current state or return to safe state
	/// - WaitForInput action should be returned as a safe default
	///
	/// Important for system stability under unexpected conditions.
	#[test]
	fn test_invalid_transition_returns_wait_for_input() {
		let mut agent = create_test_agent();

		let action = agent
			.handle_event(AgentEvent::RetryTimeoutFired)
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::WaitingForUserInput { .. }),
			"should stay in WaitingForUserInput"
		);
		assert!(
			matches!(action, AgentAction::WaitForInput),
			"should return WaitForInput for invalid transition"
		);
	}

	proptest! {
			/// **Property test: Agent always starts in WaitingForUserInput regardless of config**
			///
			/// This property verifies that the initial state invariant holds for all
			/// valid configurations. Essential for ensuring consistent agent behavior
			/// across different deployment configurations.
			#[test]
			fn agent_initial_state_invariant(
					max_retries in 1u32..10,
					max_tokens in 100u32..10000,
			) {
					let config = AgentConfig {
							max_retries,
							max_tokens,
							..AgentConfig::default()
					};
					let agent = create_test_agent_with_config(config);

					prop_assert!(
							matches!(agent.state(), AgentState::WaitingForUserInput { .. }),
							"agent must start in WaitingForUserInput"
					);
			}

			/// **Property test: UserInput always transitions from WaitingForUserInput to CallingLlm**
			///
			/// This property verifies that user input handling is deterministic:
			/// - Any valid user message must trigger CallingLlm transition
			/// - The message content must be preserved in the request
			///
			/// Ensures the fundamental user interaction contract is maintained.
			#[test]
			fn user_input_always_triggers_llm_call(
					content in "[a-zA-Z0-9 ]{1,100}",
			) {
					let mut agent = create_test_agent();
					let message = Message::user(&content);

					let action = agent.handle_event(AgentEvent::UserInput(message)).unwrap();

					prop_assert!(
							matches!(agent.state(), AgentState::CallingLlm { .. }),
							"must transition to CallingLlm"
					);

					if let AgentAction::SendLlmRequest(request) = action {
							prop_assert_eq!(
									request.messages.last().map(|m| m.content.as_str()),
									Some(content.as_str()),
									"request must contain user message"
							);
					} else {
							prop_assert!(false, "action must be SendLlmRequest");
					}
			}

			/// **Property test: Retry count never exceeds max_retries**
			///
			/// This property verifies the retry bound invariant:
			/// - After max_retries errors, agent must stop retrying
			/// - Agent should transition to WaitingForUserInput at the limit
			///
			/// Prevents infinite retry loops that could exhaust resources.
			#[test]
			fn retry_count_bounded_by_max_retries(
					max_retries in 1u32..5,
			) {
					let config = AgentConfig {
							max_retries,
							..AgentConfig::default()
					};
					let mut agent = create_test_agent_with_config(config);

					agent.handle_event(AgentEvent::UserInput(Message::user("test"))).unwrap();

					for _ in 0..max_retries {
							let result = agent.handle_event(
									AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout))
							);
							prop_assert!(result.is_ok());

							if matches!(agent.state(), AgentState::Error { .. }) {
									agent.handle_event(AgentEvent::RetryTimeoutFired).unwrap();
							}
					}

					agent.handle_event(
							AgentEvent::LlmEvent(LlmEvent::Error(LlmError::Timeout))
					).unwrap();

					prop_assert!(
							matches!(agent.state(), AgentState::WaitingForUserInput { .. }),
							"must return to WaitingForUserInput after max retries"
					);
			}

			/// **Property test: ShutdownRequested always results in ShuttingDown state**
			///
			/// This property verifies shutdown is always possible:
			/// - From any reachable state, ShutdownRequested must succeed
			/// - Result must always be ShuttingDown state
			///
			/// Critical safety property for graceful termination.
			#[test]
			fn shutdown_always_succeeds(
					num_user_inputs in 0usize..3,
			) {
					let mut agent = create_test_agent();

					for i in 0..num_user_inputs {
							agent.handle_event(AgentEvent::UserInput(
									Message::user(format!("message {i}"))
							)).unwrap();

							let response = create_simple_response("response");
							agent.handle_event(
									AgentEvent::LlmEvent(LlmEvent::Completed(response))
							).unwrap();
					}

					let action = agent.handle_event(AgentEvent::ShutdownRequested).unwrap();

					prop_assert!(
							matches!(agent.state(), AgentState::ShuttingDown),
							"must be in ShuttingDown state"
					);
					prop_assert!(
							matches!(action, AgentAction::Shutdown),
							"must emit Shutdown action"
					);
			}

			/// **Property test: PostToolsHook always transitions to CallingLlm on completion**
			///
			/// This property verifies that the post-tools hook state machine is deterministic:
			/// - Regardless of whether an action was taken (e.g., commit made), the hook
			///   must complete and allow the agent to continue with the LLM call
			/// - The pending LLM request must be preserved and sent after the hook completes
			///
			/// Essential for ensuring auto-commit and other post-tool hooks don't block the agent loop.
			#[test]
			fn post_tools_hook_always_completes(action_taken in proptest::bool::ANY) {
					let config = AgentConfig::default();
					let llm = Arc::new(MockLlmClient);
					let tools = vec![];

					let conversation = ConversationContext::new();
					let pending_request = LlmRequest {
							model: config.model_name.clone(),
							messages: vec![Message::user("test")],
							tools: vec![],
							max_tokens: Some(config.max_tokens),
							temperature: config.temperature,
					};
					let completed_tools = vec![CompletedToolInfo {
							tool_name: "edit_file".to_string(),
							succeeded: true,
					}];

					let mut agent = Agent {
							state: AgentState::PostToolsHook {
									conversation,
									pending_llm_request: pending_request,
									completed_tools,
							},
							config,
							llm,
							tools,
					};

					let action = agent
							.handle_event(AgentEvent::PostToolsHookCompleted { action_taken })
							.unwrap();

					prop_assert!(
							matches!(agent.state(), AgentState::CallingLlm { .. }),
							"must transition to CallingLlm after PostToolsHookCompleted"
					);
					prop_assert!(
							matches!(action, AgentAction::SendLlmRequest(_)),
							"must emit SendLlmRequest action"
					);
			}
	}

	/// **Test: Mutating tools (edit_file) trigger PostToolsHook state**
	///
	/// This test verifies that when edit_file (a mutating tool) completes
	/// successfully, the agent transitions to PostToolsHook instead of directly
	/// to CallingLlm. This is critical for auto-commit support, as we need a hook
	/// point to commit changes before continuing the LLM conversation.
	#[test]
	fn test_mutating_tools_trigger_post_tools_hook() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("edit a file")))
			.expect("handle_event should succeed");

		let tool_calls = vec![ToolCall {
			id: "call_edit".to_string(),
			tool_name: "edit_file".to_string(),
			arguments_json: serde_json::json!({"path": "/test.txt", "content": "hello"}),
		}];
		let response = create_response_with_tools(tool_calls);

		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Completed(response)))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::ToolCompleted {
				call_id: "call_edit".to_string(),
				outcome: ToolExecutionOutcome::Success {
					call_id: "call_edit".to_string(),
					output: serde_json::json!({"success": true}),
				},
			})
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::PostToolsHook {
				completed_tools, ..
			} => {
				assert_eq!(completed_tools.len(), 1);
				assert_eq!(completed_tools[0].tool_name, "edit_file");
				assert!(completed_tools[0].succeeded);
			}
			other => panic!("expected PostToolsHook, got {}", other.name()),
		}

		match action {
			AgentAction::RunPostToolsHook { completed_tools } => {
				assert_eq!(completed_tools.len(), 1);
				assert_eq!(completed_tools[0].tool_name, "edit_file");
			}
			other => panic!("expected RunPostToolsHook, got {other:?}"),
		}
	}

	/// **Test: Non-mutating tools skip PostToolsHook and go directly to
	/// CallingLlm**
	///
	/// This test verifies that read-only tools (like read_file) don't trigger the
	/// post-tools hook. This is important for performance - we only want to run
	/// auto-commit when there are actual file changes to commit.
	#[test]
	fn test_non_mutating_tools_skip_post_tools_hook() {
		let mut agent = create_test_agent();

		agent
			.handle_event(AgentEvent::UserInput(Message::user("read a file")))
			.expect("handle_event should succeed");

		let tool_calls = vec![ToolCall {
			id: "call_read".to_string(),
			tool_name: "read_file".to_string(),
			arguments_json: serde_json::json!({"path": "/test.txt"}),
		}];
		let response = create_response_with_tools(tool_calls);

		agent
			.handle_event(AgentEvent::LlmEvent(LlmEvent::Completed(response)))
			.expect("handle_event should succeed");

		let action = agent
			.handle_event(AgentEvent::ToolCompleted {
				call_id: "call_read".to_string(),
				outcome: ToolExecutionOutcome::Success {
					call_id: "call_read".to_string(),
					output: serde_json::json!({"content": "file contents"}),
				},
			})
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::CallingLlm { retries, .. } => {
				assert_eq!(*retries, 0, "retries should be 0");
			}
			other => panic!("expected CallingLlm, got {}", other.name()),
		}

		assert!(
			matches!(action, AgentAction::SendLlmRequest(_)),
			"expected SendLlmRequest, got {action:?}"
		);
	}

	/// **Test: PostToolsHookCompleted transitions to CallingLlm with
	/// SendLlmRequest**
	///
	/// This test verifies the completion of the post-tools hook cycle:
	/// - When PostToolsHookCompleted is received, the agent must transition to
	///   CallingLlm
	/// - The pending LLM request must be sent to continue the conversation
	/// - This ensures the agentic loop continues after auto-commit or other hooks
	///   complete
	#[test]
	fn test_post_tools_hook_completed_transitions() {
		let config = AgentConfig::default();
		let llm = Arc::new(MockLlmClient);

		let mut conversation = ConversationContext::new();
		conversation.messages.push(Message::user("test"));
		conversation
			.messages
			.push(Message::assistant("I'll edit the file"));

		let pending_request = LlmRequest {
			model: config.model_name.clone(),
			messages: conversation.messages.clone(),
			tools: vec![],
			max_tokens: Some(config.max_tokens),
			temperature: config.temperature,
		};
		let completed_tools = vec![CompletedToolInfo {
			tool_name: "edit_file".to_string(),
			succeeded: true,
		}];

		let mut agent = Agent {
			state: AgentState::PostToolsHook {
				conversation,
				pending_llm_request: pending_request.clone(),
				completed_tools,
			},
			config,
			llm,
			tools: vec![],
		};

		let action = agent
			.handle_event(AgentEvent::PostToolsHookCompleted { action_taken: true })
			.expect("handle_event should succeed");

		match agent.state() {
			AgentState::CallingLlm {
				conversation,
				retries,
			} => {
				assert_eq!(*retries, 0);
				assert!(!conversation.messages.is_empty());
			}
			other => panic!("expected CallingLlm, got {}", other.name()),
		}

		match action {
			AgentAction::SendLlmRequest(request) => {
				assert_eq!(request.messages.len(), pending_request.messages.len());
			}
			other => panic!("expected SendLlmRequest, got {other:?}"),
		}
	}

	/// **Test: ShutdownRequested from PostToolsHook works correctly**
	///
	/// This test verifies that graceful shutdown is possible from the
	/// PostToolsHook state. This is important because auto-commit might take
	/// time, and users should be able to interrupt the process cleanly.
	#[test]
	fn test_shutdown_from_post_tools_hook() {
		let config = AgentConfig::default();
		let llm = Arc::new(MockLlmClient);

		let conversation = ConversationContext::new();
		let pending_request = LlmRequest {
			model: config.model_name.clone(),
			messages: vec![Message::user("test")],
			tools: vec![],
			max_tokens: Some(config.max_tokens),
			temperature: config.temperature,
		};

		let mut agent = Agent {
			state: AgentState::PostToolsHook {
				conversation,
				pending_llm_request: pending_request,
				completed_tools: vec![],
			},
			config,
			llm,
			tools: vec![],
		};

		let action = agent
			.handle_event(AgentEvent::ShutdownRequested)
			.expect("handle_event should succeed");

		assert!(
			matches!(agent.state(), AgentState::ShuttingDown),
			"should transition to ShuttingDown from PostToolsHook"
		);
		assert!(
			matches!(action, AgentAction::Shutdown),
			"should emit Shutdown action"
		);
	}
}
