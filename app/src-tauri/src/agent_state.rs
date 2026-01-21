//! Agent state machine types and transitions.
//!
//! This module defines the deterministic state machine for orchestrating LLM turns,
//! tool execution, and post-tool hooks. See specs/agent-state-machine.md for the full spec.

use serde::{Deserialize, Serialize};

// ============================================================================
// Core State Types (§3)
// ============================================================================

/// High-level session state. Mirrors UI state + orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStateKind {
    Idle,
    Starting,
    Ready,
    CallingLlm,
    ProcessingResponse,
    ExecutingTools,
    PostToolsHook,
    Error,
    Stopping,
    Stopped,
}

impl Default for AgentStateKind {
    fn default() -> Self {
        Self::Idle
    }
}

/// Tool execution lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

/// Hook execution lifecycle status (same as tools).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
}

/// Source of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSource {
    Harness,
    Tool,
    Hook,
    Orchestrator,
}

/// An error that occurred during agent execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub source: ErrorSource,
}

/// Normalized tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    /// True for file edits, git ops, or shell commands.
    pub mutating: bool,
}

/// Tool execution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRunRecord {
    pub run_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub mutating: bool,
    pub status: ToolRunStatus,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub attempt: u32,
    pub error: Option<String>,
}

/// Hook execution metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRunRecord {
    pub run_id: String,
    pub hook_name: String,
    pub tool_run_ids: Vec<String>,
    pub status: HookRunStatus,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub attempt: u32,
    pub error: Option<String>,
}

/// Full in-memory agent state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentState {
    pub kind: AgentStateKind,
    pub active_stream_id: Option<String>,
    pub retries: u32,
    pub pending_tool_calls: Vec<ToolCall>,
    pub tool_runs: Vec<ToolRunRecord>,
    pub hook_runs: Vec<HookRunRecord>,
    pub last_error: Option<AgentError>,
}

/// Snapshot for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
    pub kind: AgentStateKind,
    pub active_stream_id: Option<String>,
    pub pending_tool_calls: Vec<ToolCall>,
    pub tool_runs: Vec<ToolRunRecord>,
    pub hook_runs: Vec<HookRunRecord>,
    pub last_error: Option<AgentError>,
}

// ============================================================================
// Events (§3, §4)
// ============================================================================

/// Retry target for timeout events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryTarget {
    Llm,
    Tool { run_id: String },
    Hook { run_id: String },
}

/// Stream event from harness (simplified; full spec in streaming-event-schema.md).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    TextDelta { content: String },
    ToolCallDelta { call_id: String, content: String },
    Completed,
    Error { message: String },
}

/// Inputs to the state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    UserInput {
        session_id: String,
        text: String,
    },
    HarnessStream {
        session_id: String,
        stream_event: StreamEvent,
    },
    ToolRequested {
        session_id: String,
        call: ToolCall,
    },
    ToolStarted {
        session_id: String,
        run_id: String,
    },
    ToolCompleted {
        session_id: String,
        run_id: String,
        status: ToolRunStatus,
    },
    HookStarted {
        session_id: String,
        run_id: String,
        tool_run_id: String,
    },
    HookCompleted {
        session_id: String,
        run_id: String,
        status: HookRunStatus,
    },
    RetryTimeout {
        session_id: String,
        target: RetryTarget,
    },
    StopRequested {
        session_id: String,
    },
    HarnessExited {
        session_id: String,
        code: Option<i32>,
    },
}

// ============================================================================
// Actions (§3)
// ============================================================================

/// Outputs from the state machine. Drives I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    SendToHarness { session_id: String, input: String },
    ExecuteTools { session_id: String, tools: Vec<ToolCall> },
    RunPostToolHooks { session_id: String, tool_runs: Vec<String> },
    EmitStateChange { session_id: String, from: AgentStateKind, to: AgentStateKind },
    StopHarness { session_id: String },
    Wait,
}

// ============================================================================
// Hook Configuration (§3)
// ============================================================================

/// Hook failure policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookFailurePolicy {
    FailSession,
    WarnContinue,
    Retry { max_attempts: u32, delay_ms: u64 },
}

impl Default for HookFailurePolicy {
    fn default() -> Self {
        Self::FailSession
    }
}

/// Hook tool filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookToolFilter {
    AnyMutating,
    ToolNames(Vec<String>),
}

impl Default for HookToolFilter {
    fn default() -> Self {
        Self::AnyMutating
    }
}

/// Hook definition from hooks.json.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookConfig {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub failure_policy: HookFailurePolicy,
    #[serde(default)]
    pub tool_filter: HookToolFilter,
}

fn default_hook_timeout_ms() -> u64 {
    120_000
}

/// Root config for hooks.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
}

// ============================================================================
// State Event Payloads (§4)
// ============================================================================

/// Reason for a state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateChangeReason {
    UserInput,
    StreamCompleted,
    ToolsRequested,
    ToolsCompleted,
    HooksCompleted,
    StopRequested,
    HarnessExited,
}

/// State change event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateChangedPayload {
    pub session_id: String,
    pub from: AgentStateKind,
    pub to: AgentStateKind,
    pub reason: StateChangeReason,
    pub timestamp_ms: u64,
    pub stream_id: Option<String>,
}

/// Tool lifecycle event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLifecyclePayload {
    pub session_id: String,
    pub run_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub mutating: bool,
    pub status: ToolRunStatus,
    pub attempt: u32,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub error: Option<String>,
}

/// Hook lifecycle event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookLifecyclePayload {
    pub session_id: String,
    pub run_id: String,
    pub hook_name: String,
    pub tool_run_ids: Vec<String>,
    pub status: HookRunStatus,
    pub attempt: u32,
    pub started_at_ms: u64,
    pub finished_at_ms: Option<u64>,
    pub error: Option<String>,
}

/// Session error event payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionErrorPayload {
    pub session_id: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub source: ErrorSource,
}

/// Union of all state event payloads (agent:state_event channel).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStateEvent {
    StateChanged(StateChangedPayload),
    ToolLifecycle(ToolLifecyclePayload),
    HookLifecycle(HookLifecyclePayload),
    SessionError(SessionErrorPayload),
}

/// Envelope for all state events (includes required fields per §4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStateEventEnvelope {
    pub event_id: String,
    pub timestamp_ms: u64,
    pub session_id: String,
    pub payload: AgentStateEvent,
}

// ============================================================================
// State Machine Transitions (§5)
// ============================================================================

/// Result of a state transition.
#[derive(Debug, Clone)]
pub struct TransitionResult {
    /// The new state kind after the transition.
    pub new_kind: AgentStateKind,
    /// Action to execute (advisory; caller must handle I/O).
    pub action: AgentAction,
    /// Reason for the state change (for event emission).
    pub reason: Option<StateChangeReason>,
}

/// Error when a transition is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: AgentStateKind,
    pub event_type: &'static str,
    pub message: String,
}

impl std::fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Invalid transition from {:?} on {}: {}",
            self.from, self.event_type, self.message
        )
    }
}

impl std::error::Error for InvalidTransition {}

impl AgentState {
    /// Handle an event and return the resulting action.
    ///
    /// This is the core state machine transition function. It is pure: no I/O occurs here.
    /// The caller is responsible for executing the returned action and emitting events.
    ///
    /// See spec §5 for the state transition table.
    pub fn handle_event(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        // StopRequested can happen from any state except Stopped
        if let AgentEvent::StopRequested { session_id: sid } = event {
            if sid != session_id {
                return Err(InvalidTransition {
                    from: self.kind,
                    event_type: "StopRequested",
                    message: "Session ID mismatch".to_string(),
                });
            }
            if self.kind == AgentStateKind::Stopped {
                return Err(InvalidTransition {
                    from: self.kind,
                    event_type: "StopRequested",
                    message: "Session already stopped".to_string(),
                });
            }
            let _from = self.kind;
            self.kind = AgentStateKind::Stopping;
            return Ok(TransitionResult {
                new_kind: AgentStateKind::Stopping,
                action: AgentAction::StopHarness {
                    session_id: session_id.to_string(),
                },
                reason: Some(StateChangeReason::StopRequested),
            });
        }

        match self.kind {
            AgentStateKind::Idle => self.handle_idle(event, session_id),
            AgentStateKind::Starting => self.handle_starting(event, session_id),
            AgentStateKind::Ready => self.handle_ready(event, session_id),
            AgentStateKind::CallingLlm => self.handle_calling_llm(event, session_id),
            AgentStateKind::ProcessingResponse => {
                self.handle_processing_response(event, session_id)
            }
            AgentStateKind::ExecutingTools => self.handle_executing_tools(event, session_id),
            AgentStateKind::PostToolsHook => self.handle_post_tools_hook(event, session_id),
            AgentStateKind::Error => self.handle_error(event, session_id),
            AgentStateKind::Stopping => self.handle_stopping(event, session_id),
            AgentStateKind::Stopped => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "Session is stopped; no transitions allowed".to_string(),
            }),
        }
    }

    fn handle_idle(
        &mut self,
        event: &AgentEvent,
        _session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        // Idle only transitions to Starting via spawn_session (external trigger)
        // The state machine itself doesn't handle spawn; caller sets Starting directly.
        Err(InvalidTransition {
            from: self.kind,
            event_type: event_type_name(event),
            message: "Idle state only transitions via spawn_session".to_string(),
        })
    }

    fn handle_starting(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::HarnessExited { code, .. } => {
                // Successful harness init (code=0 or None for ready signal) -> Ready
                // Non-zero exit -> Stopped
                if *code == Some(0) || code.is_none() {
                    self.kind = AgentStateKind::Ready;
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::Ready,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::Starting,
                            to: AgentStateKind::Ready,
                        },
                        reason: Some(StateChangeReason::HarnessExited),
                    })
                } else {
                    self.kind = AgentStateKind::Stopped;
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::Stopped,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::Starting,
                            to: AgentStateKind::Stopped,
                        },
                        reason: Some(StateChangeReason::HarnessExited),
                    })
                }
            }
            AgentEvent::HarnessStream { stream_event, .. } => {
                // Harness became ready via stream event
                if matches!(stream_event, StreamEvent::Completed) {
                    self.kind = AgentStateKind::Ready;
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::Ready,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::Starting,
                            to: AgentStateKind::Ready,
                        },
                        reason: Some(StateChangeReason::StreamCompleted),
                    })
                } else {
                    // Stream deltas during starting are ignored; no transition
                    Ok(TransitionResult {
                        new_kind: self.kind,
                        action: AgentAction::Wait,
                        reason: None,
                    })
                }
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "Starting state expects HarnessExited or HarnessStream".to_string(),
            }),
        }
    }

    fn handle_ready(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::UserInput { text, .. } => {
                // Create new stream and transition to CallingLlm
                let stream_id = format!("turn_{}", uuid::Uuid::new_v4());
                self.active_stream_id = Some(stream_id.clone());
                self.retries = 1;
                self.pending_tool_calls.clear();
                self.kind = AgentStateKind::CallingLlm;

                Ok(TransitionResult {
                    new_kind: AgentStateKind::CallingLlm,
                    action: AgentAction::SendToHarness {
                        session_id: session_id.to_string(),
                        input: text.clone(),
                    },
                    reason: Some(StateChangeReason::UserInput),
                })
            }
            AgentEvent::HarnessExited { code: _, .. } => {
                // Harness died while ready -> Stopped
                self.kind = AgentStateKind::Stopped;
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Stopped,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::Ready,
                        to: AgentStateKind::Stopped,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "Ready state expects UserInput or HarnessExited".to_string(),
            }),
        }
    }

    fn handle_calling_llm(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::HarnessStream { stream_event, .. } => match stream_event {
                StreamEvent::TextDelta { .. } | StreamEvent::ToolCallDelta { .. } => {
                    // Stream deltas don't cause state transitions
                    Ok(TransitionResult {
                        new_kind: self.kind,
                        action: AgentAction::Wait,
                        reason: None,
                    })
                }
                StreamEvent::Completed => {
                    // Stream completed -> ProcessingResponse
                    self.kind = AgentStateKind::ProcessingResponse;
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::ProcessingResponse,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::CallingLlm,
                            to: AgentStateKind::ProcessingResponse,
                        },
                        reason: Some(StateChangeReason::StreamCompleted),
                    })
                }
                StreamEvent::Error { message } => {
                    // Stream error -> Error
                    self.kind = AgentStateKind::Error;
                    self.last_error = Some(AgentError {
                        code: "streaming_failed".to_string(),
                        message: message.clone(),
                        retryable: true,
                        source: ErrorSource::Harness,
                    });
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::Error,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::CallingLlm,
                            to: AgentStateKind::Error,
                        },
                        reason: Some(StateChangeReason::StreamCompleted),
                    })
                }
            },
            AgentEvent::ToolRequested { call, .. } => {
                // Accumulate tool calls (these come during streaming)
                self.pending_tool_calls.push(call.clone());
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            AgentEvent::HarnessExited { .. } => {
                // Harness died during LLM call -> Error
                self.kind = AgentStateKind::Error;
                self.last_error = Some(AgentError {
                    code: "harness_failed".to_string(),
                    message: "Harness exited during LLM call".to_string(),
                    retryable: true,
                    source: ErrorSource::Harness,
                });
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Error,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::CallingLlm,
                        to: AgentStateKind::Error,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "CallingLlm expects HarnessStream, ToolRequested, or HarnessExited"
                    .to_string(),
            }),
        }
    }

    fn handle_processing_response(
        &mut self,
        event: &AgentEvent,
        _session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::ToolRequested { call, .. } => {
                // Final tool calls being registered
                self.pending_tool_calls.push(call.clone());
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            // ProcessingResponse is a transient state. The orchestrator should check
            // pending_tool_calls and either:
            // 1. Transition to ExecutingTools if there are tool calls
            // 2. Transition to Ready if there are no tool calls
            // This is done via external call to finalize_response()
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "ProcessingResponse expects ToolRequested or finalize_response call"
                    .to_string(),
            }),
        }
    }

    /// Called by orchestrator after ProcessingResponse to decide next state.
    pub fn finalize_response(&mut self, session_id: &str) -> TransitionResult {
        if self.kind != AgentStateKind::ProcessingResponse {
            // Not in ProcessingResponse; this is a no-op
            return TransitionResult {
                new_kind: self.kind,
                action: AgentAction::Wait,
                reason: None,
            };
        }

        if self.pending_tool_calls.is_empty() {
            // No tools -> Ready
            self.kind = AgentStateKind::Ready;
            TransitionResult {
                new_kind: AgentStateKind::Ready,
                action: AgentAction::EmitStateChange {
                    session_id: session_id.to_string(),
                    from: AgentStateKind::ProcessingResponse,
                    to: AgentStateKind::Ready,
                },
                reason: Some(StateChangeReason::StreamCompleted),
            }
        } else {
            // Has tools -> ExecutingTools
            let tools = self.pending_tool_calls.clone();
            self.kind = AgentStateKind::ExecutingTools;
            TransitionResult {
                new_kind: AgentStateKind::ExecutingTools,
                action: AgentAction::ExecuteTools {
                    session_id: session_id.to_string(),
                    tools,
                },
                reason: Some(StateChangeReason::ToolsRequested),
            }
        }
    }

    fn handle_executing_tools(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::ToolStarted { run_id, .. } => {
                // Mark tool as running in tool_runs
                if let Some(record) = self.tool_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = ToolRunStatus::Running;
                }
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            AgentEvent::ToolCompleted { run_id, status, .. } => {
                // Update tool record
                if let Some(record) = self.tool_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = *status;
                }

                // Check if this was a failure
                if *status == ToolRunStatus::Failed {
                    self.kind = AgentStateKind::Error;
                    self.last_error = Some(AgentError {
                        code: "tool_execution_failed".to_string(),
                        message: format!("Tool {} failed", run_id),
                        retryable: true,
                        source: ErrorSource::Tool,
                    });
                    return Ok(TransitionResult {
                        new_kind: AgentStateKind::Error,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::ExecutingTools,
                            to: AgentStateKind::Error,
                        },
                        reason: Some(StateChangeReason::ToolsCompleted),
                    });
                }

                // Check if all tools are done
                let all_done = self.tool_runs.iter().all(|r| {
                    matches!(
                        r.status,
                        ToolRunStatus::Succeeded | ToolRunStatus::Failed | ToolRunStatus::Canceled
                    )
                });

                if all_done {
                    // Check if any tool was mutating
                    let has_mutating = self.tool_runs.iter().any(|r| r.mutating);

                    if has_mutating {
                        // -> PostToolsHook
                        let tool_run_ids: Vec<String> =
                            self.tool_runs.iter().map(|r| r.run_id.clone()).collect();
                        self.kind = AgentStateKind::PostToolsHook;
                        Ok(TransitionResult {
                            new_kind: AgentStateKind::PostToolsHook,
                            action: AgentAction::RunPostToolHooks {
                                session_id: session_id.to_string(),
                                tool_runs: tool_run_ids,
                            },
                            reason: Some(StateChangeReason::ToolsCompleted),
                        })
                    } else {
                        // No mutating tools -> CallingLlm with tool results
                        self.kind = AgentStateKind::CallingLlm;
                        self.pending_tool_calls.clear();
                        Ok(TransitionResult {
                            new_kind: AgentStateKind::CallingLlm,
                            action: AgentAction::SendToHarness {
                                session_id: session_id.to_string(),
                                input: String::new(), // Tool results are appended by orchestrator
                            },
                            reason: Some(StateChangeReason::ToolsCompleted),
                        })
                    }
                } else {
                    // More tools still running
                    Ok(TransitionResult {
                        new_kind: self.kind,
                        action: AgentAction::Wait,
                        reason: None,
                    })
                }
            }
            AgentEvent::HarnessExited { .. } => {
                // Harness died during tool execution -> Error
                self.kind = AgentStateKind::Error;
                self.last_error = Some(AgentError {
                    code: "harness_failed".to_string(),
                    message: "Harness exited during tool execution".to_string(),
                    retryable: false,
                    source: ErrorSource::Harness,
                });
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Error,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::ExecutingTools,
                        to: AgentStateKind::Error,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "ExecutingTools expects ToolStarted, ToolCompleted, or HarnessExited"
                    .to_string(),
            }),
        }
    }

    fn handle_post_tools_hook(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::HookStarted { run_id, .. } => {
                // Mark hook as running
                if let Some(record) = self.hook_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = HookRunStatus::Running;
                }
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            AgentEvent::HookCompleted { run_id, status, .. } => {
                // Update hook record
                if let Some(record) = self.hook_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = *status;
                }

                // Check for failure (policy handling is done by hook runner)
                if *status == HookRunStatus::Failed {
                    // Hook runner decided this is a session-failing error
                    self.kind = AgentStateKind::Error;
                    self.last_error = Some(AgentError {
                        code: "hook_execution_failed".to_string(),
                        message: format!("Hook {} failed", run_id),
                        retryable: false,
                        source: ErrorSource::Hook,
                    });
                    return Ok(TransitionResult {
                        new_kind: AgentStateKind::Error,
                        action: AgentAction::EmitStateChange {
                            session_id: session_id.to_string(),
                            from: AgentStateKind::PostToolsHook,
                            to: AgentStateKind::Error,
                        },
                        reason: Some(StateChangeReason::HooksCompleted),
                    });
                }

                // Check if all hooks are done
                let all_done = self.hook_runs.iter().all(|r| {
                    matches!(
                        r.status,
                        HookRunStatus::Succeeded | HookRunStatus::Failed | HookRunStatus::Canceled
                    )
                });

                if all_done {
                    // All hooks done -> CallingLlm with tool results
                    self.kind = AgentStateKind::CallingLlm;
                    self.pending_tool_calls.clear();
                    Ok(TransitionResult {
                        new_kind: AgentStateKind::CallingLlm,
                        action: AgentAction::SendToHarness {
                            session_id: session_id.to_string(),
                            input: String::new(), // Tool results appended by orchestrator
                        },
                        reason: Some(StateChangeReason::HooksCompleted),
                    })
                } else {
                    Ok(TransitionResult {
                        new_kind: self.kind,
                        action: AgentAction::Wait,
                        reason: None,
                    })
                }
            }
            AgentEvent::HarnessExited { .. } => {
                self.kind = AgentStateKind::Error;
                self.last_error = Some(AgentError {
                    code: "harness_failed".to_string(),
                    message: "Harness exited during hook execution".to_string(),
                    retryable: false,
                    source: ErrorSource::Harness,
                });
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Error,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::PostToolsHook,
                        to: AgentStateKind::Error,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "PostToolsHook expects HookStarted, HookCompleted, or HarnessExited"
                    .to_string(),
            }),
        }
    }

    fn handle_error(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::RetryTimeout { target, .. } => {
                match target {
                    RetryTarget::Llm => {
                        // Retry LLM call
                        self.retries += 1;
                        self.kind = AgentStateKind::CallingLlm;
                        self.last_error = None;
                        Ok(TransitionResult {
                            new_kind: AgentStateKind::CallingLlm,
                            action: AgentAction::SendToHarness {
                                session_id: session_id.to_string(),
                                input: String::new(), // Retry uses existing context
                            },
                            reason: Some(StateChangeReason::UserInput), // Treat as resuming
                        })
                    }
                    RetryTarget::Tool { run_id } => {
                        // Retry specific tool
                        self.kind = AgentStateKind::ExecutingTools;
                        self.last_error = None;
                        // Find the tool call to retry
                        let tools: Vec<ToolCall> = self
                            .tool_runs
                            .iter()
                            .filter(|r| r.run_id == *run_id)
                            .map(|r| ToolCall {
                                call_id: r.call_id.clone(),
                                name: r.tool_name.clone(),
                                arguments: serde_json::Value::Null, // Orchestrator has full args
                                mutating: r.mutating,
                            })
                            .collect();
                        Ok(TransitionResult {
                            new_kind: AgentStateKind::ExecutingTools,
                            action: AgentAction::ExecuteTools {
                                session_id: session_id.to_string(),
                                tools,
                            },
                            reason: Some(StateChangeReason::ToolsRequested),
                        })
                    }
                    RetryTarget::Hook { run_id } => {
                        // Retry specific hook
                        self.kind = AgentStateKind::PostToolsHook;
                        self.last_error = None;
                        let tool_runs: Vec<String> = self
                            .hook_runs
                            .iter()
                            .filter(|r| r.run_id == *run_id)
                            .flat_map(|r| r.tool_run_ids.clone())
                            .collect();
                        Ok(TransitionResult {
                            new_kind: AgentStateKind::PostToolsHook,
                            action: AgentAction::RunPostToolHooks {
                                session_id: session_id.to_string(),
                                tool_runs,
                            },
                            reason: Some(StateChangeReason::ToolsCompleted),
                        })
                    }
                }
            }
            AgentEvent::HarnessExited { .. } => {
                self.kind = AgentStateKind::Stopped;
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Stopped,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::Error,
                        to: AgentStateKind::Stopped,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            AgentEvent::UserInput { text, .. } => {
                // User can send new input to recover from error
                let stream_id = format!("turn_{}", uuid::Uuid::new_v4());
                self.active_stream_id = Some(stream_id);
                self.retries = 1;
                self.pending_tool_calls.clear();
                self.last_error = None;
                self.kind = AgentStateKind::CallingLlm;

                Ok(TransitionResult {
                    new_kind: AgentStateKind::CallingLlm,
                    action: AgentAction::SendToHarness {
                        session_id: session_id.to_string(),
                        input: text.clone(),
                    },
                    reason: Some(StateChangeReason::UserInput),
                })
            }
            _ => Err(InvalidTransition {
                from: self.kind,
                event_type: event_type_name(event),
                message: "Error state expects RetryTimeout, UserInput, or HarnessExited"
                    .to_string(),
            }),
        }
    }

    fn handle_stopping(
        &mut self,
        event: &AgentEvent,
        session_id: &str,
    ) -> Result<TransitionResult, InvalidTransition> {
        match event {
            AgentEvent::HarnessExited { code: _, .. } => {
                self.kind = AgentStateKind::Stopped;
                Ok(TransitionResult {
                    new_kind: AgentStateKind::Stopped,
                    action: AgentAction::EmitStateChange {
                        session_id: session_id.to_string(),
                        from: AgentStateKind::Stopping,
                        to: AgentStateKind::Stopped,
                    },
                    reason: Some(StateChangeReason::HarnessExited),
                })
            }
            AgentEvent::ToolCompleted { run_id, .. } => {
                // Tool completed while stopping -> mark as canceled
                if let Some(record) = self.tool_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = ToolRunStatus::Canceled;
                }
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            AgentEvent::HookCompleted { run_id, .. } => {
                // Hook completed while stopping -> mark as canceled
                if let Some(record) = self.hook_runs.iter_mut().find(|r| r.run_id == *run_id) {
                    record.status = HookRunStatus::Canceled;
                }
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
            _ => {
                // Ignore other events while stopping
                Ok(TransitionResult {
                    new_kind: self.kind,
                    action: AgentAction::Wait,
                    reason: None,
                })
            }
        }
    }

    /// Transition from Idle to Starting (called by spawn_session).
    pub fn start(&mut self) {
        self.kind = AgentStateKind::Starting;
    }

    /// Register tool runs before executing them.
    pub fn register_tool_runs(&mut self, records: Vec<ToolRunRecord>) {
        self.tool_runs.extend(records);
    }

    /// Register hook runs before executing them.
    pub fn register_hook_runs(&mut self, records: Vec<HookRunRecord>) {
        self.hook_runs.extend(records);
    }
}

/// Get a static name for an event type (for error messages).
fn event_type_name(event: &AgentEvent) -> &'static str {
    match event {
        AgentEvent::UserInput { .. } => "UserInput",
        AgentEvent::HarnessStream { .. } => "HarnessStream",
        AgentEvent::ToolRequested { .. } => "ToolRequested",
        AgentEvent::ToolStarted { .. } => "ToolStarted",
        AgentEvent::ToolCompleted { .. } => "ToolCompleted",
        AgentEvent::HookStarted { .. } => "HookStarted",
        AgentEvent::HookCompleted { .. } => "HookCompleted",
        AgentEvent::RetryTimeout { .. } => "RetryTimeout",
        AgentEvent::StopRequested { .. } => "StopRequested",
        AgentEvent::HarnessExited { .. } => "HarnessExited",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_state_kind_default_is_idle() {
        assert_eq!(AgentStateKind::default(), AgentStateKind::Idle);
    }

    #[test]
    fn tool_call_serializes_correctly() {
        let call = ToolCall {
            call_id: "call_123".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({"path": "foo.rs"}),
            mutating: true,
        };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("call_123"));
        assert!(json.contains("edit_file"));
        assert!(json.contains("mutating"));
    }

    #[test]
    fn hook_config_defaults() {
        let json = r#"{"name": "test", "command": ["echo", "hi"]}"#;
        let config: HookConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timeout_ms, 120_000);
        assert_eq!(config.failure_policy, HookFailurePolicy::FailSession);
        assert_eq!(config.tool_filter, HookToolFilter::AnyMutating);
    }

    #[test]
    fn agent_event_deserializes_user_input() {
        let json = r#"{"type": "user_input", "session_id": "sess_1", "text": "hello"}"#;
        let event: AgentEvent = serde_json::from_str(json).unwrap();
        match event {
            AgentEvent::UserInput { session_id, text } => {
                assert_eq!(session_id, "sess_1");
                assert_eq!(text, "hello");
            }
            _ => panic!("Expected UserInput variant"),
        }
    }

    #[test]
    fn agent_action_serializes_execute_tools() {
        let action = AgentAction::ExecuteTools {
            session_id: "sess_1".to_string(),
            tools: vec![ToolCall {
                call_id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"cmd": "ls"}),
                mutating: true,
            }],
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("execute_tools"));
        assert!(json.contains("bash"));
    }

    // ========================================================================
    // State Transition Tests (§5, §6)
    // ========================================================================

    const TEST_SESSION_ID: &str = "sess_test";

    /// Helper to create a minimal AgentState in a specific kind.
    fn state_in(kind: AgentStateKind) -> AgentState {
        AgentState {
            kind,
            ..Default::default()
        }
    }

    // Valid transition: Ready + UserInput -> CallingLlm
    #[test]
    fn transition_ready_user_input_to_calling_llm() {
        let mut state = state_in(AgentStateKind::Ready);
        let event = AgentEvent::UserInput {
            session_id: TEST_SESSION_ID.to_string(),
            text: "hello".to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);
        assert!(result.is_ok());
        let result = result.unwrap();

        assert_eq!(result.new_kind, AgentStateKind::CallingLlm);
        assert_eq!(state.kind, AgentStateKind::CallingLlm);
        assert!(matches!(result.action, AgentAction::SendToHarness { .. }));
        assert_eq!(result.reason, Some(StateChangeReason::UserInput));
        assert!(state.active_stream_id.is_some());
    }

    // Valid transition: CallingLlm + HarnessStream(Completed) -> ProcessingResponse
    #[test]
    fn transition_calling_llm_stream_completed_to_processing() {
        let mut state = state_in(AgentStateKind::CallingLlm);
        let event = AgentEvent::HarnessStream {
            session_id: TEST_SESSION_ID.to_string(),
            stream_event: StreamEvent::Completed,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::ProcessingResponse);
        assert_eq!(state.kind, AgentStateKind::ProcessingResponse);
        assert_eq!(result.reason, Some(StateChangeReason::StreamCompleted));
    }

    // Valid transition: ProcessingResponse with no tools -> Ready via finalize_response
    #[test]
    fn transition_processing_response_no_tools_to_ready() {
        let mut state = state_in(AgentStateKind::ProcessingResponse);
        state.pending_tool_calls.clear();

        let result = state.finalize_response(TEST_SESSION_ID);

        assert_eq!(result.new_kind, AgentStateKind::Ready);
        assert_eq!(state.kind, AgentStateKind::Ready);
    }

    // Valid transition: ProcessingResponse with tools -> ExecutingTools via finalize_response
    #[test]
    fn transition_processing_response_with_tools_to_executing() {
        let mut state = state_in(AgentStateKind::ProcessingResponse);
        state.pending_tool_calls.push(ToolCall {
            call_id: "call_1".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({}),
            mutating: true,
        });

        let result = state.finalize_response(TEST_SESSION_ID);

        assert_eq!(result.new_kind, AgentStateKind::ExecutingTools);
        assert_eq!(state.kind, AgentStateKind::ExecutingTools);
        assert!(matches!(result.action, AgentAction::ExecuteTools { .. }));
    }

    // Valid transition: StopRequested from Ready -> Stopping
    #[test]
    fn transition_stop_requested_from_ready() {
        let mut state = state_in(AgentStateKind::Ready);
        let event = AgentEvent::StopRequested {
            session_id: TEST_SESSION_ID.to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Stopping);
        assert_eq!(state.kind, AgentStateKind::Stopping);
        assert!(matches!(result.action, AgentAction::StopHarness { .. }));
    }

    // Valid transition: Stopping + HarnessExited -> Stopped
    #[test]
    fn transition_stopping_harness_exited_to_stopped() {
        let mut state = state_in(AgentStateKind::Stopping);
        let event = AgentEvent::HarnessExited {
            session_id: TEST_SESSION_ID.to_string(),
            code: Some(0),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Stopped);
        assert_eq!(state.kind, AgentStateKind::Stopped);
    }

    // Valid transition: Error + RetryTimeout(Llm) -> CallingLlm
    #[test]
    fn transition_error_retry_llm_to_calling_llm() {
        let mut state = state_in(AgentStateKind::Error);
        state.last_error = Some(AgentError {
            code: "streaming_failed".to_string(),
            message: "timeout".to_string(),
            retryable: true,
            source: ErrorSource::Harness,
        });

        let event = AgentEvent::RetryTimeout {
            session_id: TEST_SESSION_ID.to_string(),
            target: RetryTarget::Llm,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::CallingLlm);
        assert_eq!(state.kind, AgentStateKind::CallingLlm);
        assert!(state.last_error.is_none());
    }

    // Valid transition: CallingLlm + stream error -> Error
    #[test]
    fn transition_calling_llm_stream_error_to_error() {
        let mut state = state_in(AgentStateKind::CallingLlm);
        let event = AgentEvent::HarnessStream {
            session_id: TEST_SESSION_ID.to_string(),
            stream_event: StreamEvent::Error {
                message: "connection lost".to_string(),
            },
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Error);
        assert_eq!(state.kind, AgentStateKind::Error);
        assert!(state.last_error.is_some());
        assert_eq!(state.last_error.as_ref().unwrap().code, "streaming_failed");
    }

    // Invalid transition: Idle + UserInput (Idle only transitions via spawn_session)
    #[test]
    fn invalid_transition_idle_user_input() {
        let mut state = state_in(AgentStateKind::Idle);
        let event = AgentEvent::UserInput {
            session_id: TEST_SESSION_ID.to_string(),
            text: "hello".to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, AgentStateKind::Idle);
        assert_eq!(err.event_type, "UserInput");
    }

    // Invalid transition: Ready + ToolCompleted (wrong state for this event)
    #[test]
    fn invalid_transition_ready_tool_completed() {
        let mut state = state_in(AgentStateKind::Ready);
        let event = AgentEvent::ToolCompleted {
            session_id: TEST_SESSION_ID.to_string(),
            run_id: "toolrun_1".to_string(),
            status: ToolRunStatus::Succeeded,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, AgentStateKind::Ready);
        assert_eq!(err.event_type, "ToolCompleted");
    }

    // Invalid transition: Stopped + any event (terminal state)
    #[test]
    fn invalid_transition_stopped_is_terminal() {
        let mut state = state_in(AgentStateKind::Stopped);
        let event = AgentEvent::UserInput {
            session_id: TEST_SESSION_ID.to_string(),
            text: "hello".to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.from, AgentStateKind::Stopped);
        assert!(err.message.contains("stopped"));
    }

    // Invalid transition: StopRequested from Stopped (already stopped)
    #[test]
    fn invalid_transition_stop_from_stopped() {
        let mut state = state_in(AgentStateKind::Stopped);
        let event = AgentEvent::StopRequested {
            session_id: TEST_SESSION_ID.to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("already stopped"));
    }

    // Invalid transition: session ID mismatch
    #[test]
    fn invalid_transition_session_id_mismatch() {
        let mut state = state_in(AgentStateKind::Ready);
        let event = AgentEvent::StopRequested {
            session_id: "other_session".to_string(),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("mismatch"));
    }

    // ExecutingTools: all tools completed with mutating -> PostToolsHook
    #[test]
    fn transition_executing_tools_mutating_to_post_hooks() {
        let mut state = state_in(AgentStateKind::ExecutingTools);
        state.tool_runs.push(ToolRunRecord {
            run_id: "toolrun_1".to_string(),
            call_id: "call_1".to_string(),
            tool_name: "edit_file".to_string(),
            mutating: true,
            status: ToolRunStatus::Running,
            started_at_ms: 0,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        });

        let event = AgentEvent::ToolCompleted {
            session_id: TEST_SESSION_ID.to_string(),
            run_id: "toolrun_1".to_string(),
            status: ToolRunStatus::Succeeded,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::PostToolsHook);
        assert!(matches!(
            result.action,
            AgentAction::RunPostToolHooks { .. }
        ));
    }

    // ExecutingTools: all tools completed non-mutating -> CallingLlm
    #[test]
    fn transition_executing_tools_non_mutating_to_calling_llm() {
        let mut state = state_in(AgentStateKind::ExecutingTools);
        state.tool_runs.push(ToolRunRecord {
            run_id: "toolrun_1".to_string(),
            call_id: "call_1".to_string(),
            tool_name: "read_file".to_string(),
            mutating: false,
            status: ToolRunStatus::Running,
            started_at_ms: 0,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        });

        let event = AgentEvent::ToolCompleted {
            session_id: TEST_SESSION_ID.to_string(),
            run_id: "toolrun_1".to_string(),
            status: ToolRunStatus::Succeeded,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::CallingLlm);
        assert!(matches!(result.action, AgentAction::SendToHarness { .. }));
    }

    // ExecutingTools: tool failure -> Error
    #[test]
    fn transition_executing_tools_failure_to_error() {
        let mut state = state_in(AgentStateKind::ExecutingTools);
        state.tool_runs.push(ToolRunRecord {
            run_id: "toolrun_1".to_string(),
            call_id: "call_1".to_string(),
            tool_name: "bash".to_string(),
            mutating: true,
            status: ToolRunStatus::Running,
            started_at_ms: 0,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        });

        let event = AgentEvent::ToolCompleted {
            session_id: TEST_SESSION_ID.to_string(),
            run_id: "toolrun_1".to_string(),
            status: ToolRunStatus::Failed,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Error);
        assert!(state.last_error.is_some());
        assert_eq!(
            state.last_error.as_ref().unwrap().code,
            "tool_execution_failed"
        );
    }

    // PostToolsHook: all hooks completed -> CallingLlm
    #[test]
    fn transition_post_hooks_completed_to_calling_llm() {
        let mut state = state_in(AgentStateKind::PostToolsHook);
        state.hook_runs.push(HookRunRecord {
            run_id: "hookrun_1".to_string(),
            hook_name: "auto_commit".to_string(),
            tool_run_ids: vec!["toolrun_1".to_string()],
            status: HookRunStatus::Running,
            started_at_ms: 0,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        });

        let event = AgentEvent::HookCompleted {
            session_id: TEST_SESSION_ID.to_string(),
            run_id: "hookrun_1".to_string(),
            status: HookRunStatus::Succeeded,
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::CallingLlm);
        assert!(matches!(result.action, AgentAction::SendToHarness { .. }));
    }

    // Stream deltas don't cause state transitions
    #[test]
    fn stream_delta_no_transition() {
        let mut state = state_in(AgentStateKind::CallingLlm);
        let event = AgentEvent::HarnessStream {
            session_id: TEST_SESSION_ID.to_string(),
            stream_event: StreamEvent::TextDelta {
                content: "hello".to_string(),
            },
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::CallingLlm);
        assert!(matches!(result.action, AgentAction::Wait));
        assert!(result.reason.is_none());
    }

    // Starting + HarnessExited(code=0) -> Ready
    #[test]
    fn transition_starting_harness_ready_to_ready() {
        let mut state = state_in(AgentStateKind::Starting);
        let event = AgentEvent::HarnessExited {
            session_id: TEST_SESSION_ID.to_string(),
            code: None, // None indicates ready signal
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Ready);
        assert_eq!(state.kind, AgentStateKind::Ready);
    }

    // Starting + HarnessExited(code=1) -> Stopped
    #[test]
    fn transition_starting_harness_failed_to_stopped() {
        let mut state = state_in(AgentStateKind::Starting);
        let event = AgentEvent::HarnessExited {
            session_id: TEST_SESSION_ID.to_string(),
            code: Some(1),
        };

        let result = state.handle_event(&event, TEST_SESSION_ID).unwrap();

        assert_eq!(result.new_kind, AgentStateKind::Stopped);
        assert_eq!(state.kind, AgentStateKind::Stopped);
    }
}
