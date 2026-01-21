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
}
