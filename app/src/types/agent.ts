/**
 * Agent state machine types for frontend consumption.
 *
 * These types mirror the Rust definitions in app/src-tauri/src/agent_state.rs.
 * See specs/agent-state-machine.md for the full specification.
 */

// ============================================================================
// Core State Types (§3)
// ============================================================================

/** High-level session state. Mirrors UI state + orchestration. */
export type AgentStateKind =
  | "idle"
  | "starting"
  | "ready"
  | "calling_llm"
  | "processing_response"
  | "executing_tools"
  | "post_tools_hook"
  | "error"
  | "stopping"
  | "stopped";

/** Tool execution lifecycle status. */
export type ToolRunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "canceled";

/** Hook execution lifecycle status. */
export type HookRunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "canceled";

/** Source of an error. */
export type ErrorSource = "harness" | "tool" | "hook" | "orchestrator";

/** An error that occurred during agent execution. */
export type AgentError = {
  code: string;
  message: string;
  retryable: boolean;
  source: ErrorSource;
};

/** Normalized tool invocation. */
export type ToolCall = {
  call_id: string;
  name: string;
  arguments: Record<string, unknown>;
  /** True for file edits, git ops, or shell commands. */
  mutating: boolean;
};

/** Tool execution metadata. */
export type ToolRunRecord = {
  run_id: string;
  call_id: string;
  tool_name: string;
  mutating: boolean;
  status: ToolRunStatus;
  started_at_ms: number;
  finished_at_ms?: number;
  attempt: number;
  error?: string;
};

/** Hook execution metadata. */
export type HookRunRecord = {
  run_id: string;
  hook_name: string;
  tool_run_ids: string[];
  status: HookRunStatus;
  started_at_ms: number;
  finished_at_ms?: number;
  attempt: number;
  error?: string;
};

/** Full in-memory agent state. */
export type AgentState = {
  kind: AgentStateKind;
  active_stream_id?: string;
  retries: number;
  pending_tool_calls: ToolCall[];
  tool_runs: ToolRunRecord[];
  hook_runs: HookRunRecord[];
  last_error?: AgentError;
};

/** Snapshot for persistence. */
export type AgentStateSnapshot = {
  kind: AgentStateKind;
  active_stream_id?: string;
  pending_tool_calls: ToolCall[];
  tool_runs: ToolRunRecord[];
  hook_runs: HookRunRecord[];
  last_error?: AgentError;
};

// ============================================================================
// Events (§3, §4)
// ============================================================================

/** Retry target for timeout events. */
export type RetryTarget =
  | { type: "llm" }
  | { type: "tool"; run_id: string }
  | { type: "hook"; run_id: string };

/** Stream event from harness. */
export type StreamEvent =
  | { type: "text_delta"; content: string }
  | { type: "tool_call_delta"; call_id: string; content: string }
  | { type: "completed" }
  | { type: "error"; message: string };

/** Inputs to the state machine. */
export type AgentEvent =
  | { type: "user_input"; session_id: string; text: string }
  | { type: "harness_stream"; session_id: string; stream_event: StreamEvent }
  | { type: "tool_requested"; session_id: string; call: ToolCall }
  | { type: "tool_started"; session_id: string; run_id: string }
  | {
      type: "tool_completed";
      session_id: string;
      run_id: string;
      status: ToolRunStatus;
    }
  | {
      type: "hook_started";
      session_id: string;
      run_id: string;
      tool_run_id: string;
    }
  | {
      type: "hook_completed";
      session_id: string;
      run_id: string;
      status: HookRunStatus;
    }
  | { type: "retry_timeout"; session_id: string; target: RetryTarget }
  | { type: "stop_requested"; session_id: string }
  | { type: "harness_exited"; session_id: string; code?: number };

// ============================================================================
// Actions (§3)
// ============================================================================

/** Outputs from the state machine. Drives I/O. */
export type AgentAction =
  | { type: "send_to_harness"; session_id: string; input: string }
  | { type: "execute_tools"; session_id: string; tools: ToolCall[] }
  | { type: "run_post_tool_hooks"; session_id: string; tool_runs: string[] }
  | {
      type: "emit_state_change";
      session_id: string;
      from: AgentStateKind;
      to: AgentStateKind;
    }
  | { type: "stop_harness"; session_id: string }
  | { type: "wait" };

// ============================================================================
// Hook Configuration (§3)
// ============================================================================

/** Hook failure policy. */
export type HookFailurePolicy =
  | { type: "fail_session" }
  | { type: "warn_continue" }
  | { type: "retry"; max_attempts: number; delay_ms: number };

/** Hook tool filter. */
export type HookToolFilter =
  | { type: "any_mutating" }
  | { type: "tool_names"; names: string[] };

/** Hook definition from hooks.json. */
export type HookConfig = {
  name: string;
  command: string[];
  timeout_ms: number;
  failure_policy: HookFailurePolicy;
  tool_filter: HookToolFilter;
};

/** Root config for hooks.json. */
export type HooksConfig = {
  hooks: HookConfig[];
};

// ============================================================================
// State Event Payloads (§4)
// ============================================================================

/** Reason for a state change. */
export type StateChangeReason =
  | "user_input"
  | "stream_completed"
  | "tools_requested"
  | "tools_completed"
  | "hooks_completed"
  | "stop_requested"
  | "harness_exited";

/** State change event payload. */
export type StateChangedPayload = {
  sessionId: string;
  from: AgentStateKind;
  to: AgentStateKind;
  reason: StateChangeReason;
  timestampMs: number;
  streamId?: string;
};

/** Tool lifecycle event payload. */
export type ToolLifecyclePayload = {
  sessionId: string;
  runId: string;
  callId: string;
  toolName: string;
  mutating: boolean;
  status: ToolRunStatus;
  attempt: number;
  startedAtMs: number;
  finishedAtMs?: number;
  error?: string;
};

/** Hook lifecycle event payload. */
export type HookLifecyclePayload = {
  sessionId: string;
  runId: string;
  hookName: string;
  toolRunIds: string[];
  status: HookRunStatus;
  attempt: number;
  startedAtMs: number;
  finishedAtMs?: number;
  error?: string;
};

/** Session error event payload. */
export type SessionErrorPayload = {
  sessionId: string;
  code: string;
  message: string;
  retryable: boolean;
  source: ErrorSource;
};

/** Union of all state event payloads (agent:state_event channel). */
export type AgentStateEvent =
  | ({ type: "state_changed" } & StateChangedPayload)
  | ({ type: "tool_lifecycle" } & ToolLifecyclePayload)
  | ({ type: "hook_lifecycle" } & HookLifecyclePayload)
  | ({ type: "session_error" } & SessionErrorPayload);

/** Envelope for all state events (includes required fields per §4). */
export type AgentStateEventEnvelope = {
  eventId: string;
  timestampMs: number;
  sessionId: string;
  payload: AgentStateEvent;
};

// ============================================================================
// Event Channel Constants
// ============================================================================

/** Channel name for agent state events. */
export const AGENT_STATE_EVENT_CHANNEL = "agent:state_event";
