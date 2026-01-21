/**
 * Hook for subscribing to agent state machine events.
 *
 * Consumes `agent:state_event` channel and maintains session state
 * for UI consumption. See specs/agent-state-machine.md for the full spec.
 */

import { useCallback, useEffect, useReducer } from "react";
import { subscribeAgentStateEvents } from "../services/events";
import type {
  AgentStateEventEnvelope,
  AgentStateKind,
  AgentError,
  ToolRunRecord,
  HookRunRecord,
  ToolLifecyclePayload,
  HookLifecyclePayload,
} from "../types/agent";

// ============================================================================
// State
// ============================================================================

export type AgentSessionState = {
  /** Current state kind. */
  kind: AgentStateKind;
  /** Active stream ID for current LLM request. */
  activeStreamId?: string;
  /** Tool runs for the current batch. */
  toolRuns: Map<string, ToolRunRecord>;
  /** Hook runs for the current batch. */
  hookRuns: Map<string, HookRunRecord>;
  /** Last error, if any. */
  lastError?: AgentError;
  /** Last event ID processed. */
  lastEventId?: string;
  /** Last event timestamp. */
  lastEventTimestampMs?: number;
};

const initialState: AgentSessionState = {
  kind: "idle",
  toolRuns: new Map(),
  hookRuns: new Map(),
};

// ============================================================================
// Actions
// ============================================================================

type Action =
  | { type: "state_changed"; payload: AgentStateEventEnvelope }
  | { type: "tool_lifecycle"; payload: AgentStateEventEnvelope }
  | { type: "hook_lifecycle"; payload: AgentStateEventEnvelope }
  | { type: "session_error"; payload: AgentStateEventEnvelope }
  | { type: "reset" };

// ============================================================================
// Reducer
// ============================================================================

function toolLifecycleToRecord(payload: ToolLifecyclePayload): ToolRunRecord {
  return {
    run_id: payload.runId,
    call_id: payload.callId,
    tool_name: payload.toolName,
    mutating: payload.mutating,
    status: payload.status,
    started_at_ms: payload.startedAtMs,
    finished_at_ms: payload.finishedAtMs,
    attempt: payload.attempt,
    error: payload.error,
  };
}

function hookLifecycleToRecord(payload: HookLifecyclePayload): HookRunRecord {
  return {
    run_id: payload.runId,
    hook_name: payload.hookName,
    tool_run_ids: payload.toolRunIds,
    status: payload.status,
    started_at_ms: payload.startedAtMs,
    finished_at_ms: payload.finishedAtMs,
    attempt: payload.attempt,
    error: payload.error,
  };
}

function reducer(state: AgentSessionState, action: Action): AgentSessionState {
  switch (action.type) {
    case "state_changed": {
      const { payload, eventId, timestampMs } = action.payload;
      if (payload.type !== "state_changed") return state;

      const { to, streamId } = payload;

      // Clear tool/hook runs on certain transitions
      const clearRuns =
        to === "ready" || to === "idle" || to === "stopped" || to === "error";

      return {
        ...state,
        kind: to,
        activeStreamId: streamId ?? state.activeStreamId,
        toolRuns: clearRuns ? new Map() : state.toolRuns,
        hookRuns: clearRuns ? new Map() : state.hookRuns,
        lastError: to === "error" ? state.lastError : undefined,
        lastEventId: eventId,
        lastEventTimestampMs: timestampMs,
      };
    }

    case "tool_lifecycle": {
      const { payload, eventId, timestampMs } = action.payload;
      if (payload.type !== "tool_lifecycle") return state;

      const record = toolLifecycleToRecord(payload);
      const newToolRuns = new Map(state.toolRuns);
      newToolRuns.set(record.run_id, record);

      return {
        ...state,
        toolRuns: newToolRuns,
        lastEventId: eventId,
        lastEventTimestampMs: timestampMs,
      };
    }

    case "hook_lifecycle": {
      const { payload, eventId, timestampMs } = action.payload;
      if (payload.type !== "hook_lifecycle") return state;

      const record = hookLifecycleToRecord(payload);
      const newHookRuns = new Map(state.hookRuns);
      newHookRuns.set(record.run_id, record);

      return {
        ...state,
        hookRuns: newHookRuns,
        lastEventId: eventId,
        lastEventTimestampMs: timestampMs,
      };
    }

    case "session_error": {
      const { payload, eventId, timestampMs } = action.payload;
      if (payload.type !== "session_error") return state;

      const { code, message, retryable, source } = payload;

      return {
        ...state,
        lastError: { code, message, retryable, source },
        lastEventId: eventId,
        lastEventTimestampMs: timestampMs,
      };
    }

    case "reset":
      return initialState;

    default:
      return state;
  }
}

// ============================================================================
// Hook
// ============================================================================

export type UseAgentSessionOptions = {
  /** Session ID to filter events for. If undefined, receives all events. */
  sessionId?: string;
};

export type UseAgentSessionResult = {
  state: AgentSessionState;
  /** Reset state to initial. */
  reset: () => void;
};

/**
 * Derive UI-friendly status from AgentStateKind.
 *
 * Per state-machine-wiring.md §4, §5:
 * - idle, ready, stopped → "idle"
 * - starting, calling_llm, processing_response, executing_tools, post_tools_hook, stopping → "working"
 * - error → "error"
 */
export function isAgentWorking(kind: AgentStateKind): boolean {
  switch (kind) {
    case "starting":
    case "calling_llm":
    case "processing_response":
    case "executing_tools":
    case "post_tools_hook":
    case "stopping":
      return true;
    case "idle":
    case "ready":
    case "stopped":
    case "error":
    default:
      return false;
  }
}

/**
 * Subscribe to agent state machine events for a session.
 *
 * @param options.sessionId - Filter events for this session (optional)
 * @returns Current session state and reset function
 */
export function useAgentSession(
  options: UseAgentSessionOptions = {},
): UseAgentSessionResult {
  const { sessionId } = options;
  const [state, dispatch] = useReducer(reducer, initialState);

  const reset = useCallback(() => {
    dispatch({ type: "reset" });
  }, []);

  useEffect(() => {
    const unsubscribe = subscribeAgentStateEvents((envelope) => {
      // Filter by session if specified
      if (sessionId && envelope.sessionId !== sessionId) {
        return;
      }

      // Dispatch based on event type
      switch (envelope.payload.type) {
        case "state_changed":
          dispatch({ type: "state_changed", payload: envelope });
          break;
        case "tool_lifecycle":
          dispatch({ type: "tool_lifecycle", payload: envelope });
          break;
        case "hook_lifecycle":
          dispatch({ type: "hook_lifecycle", payload: envelope });
          break;
        case "session_error":
          dispatch({ type: "session_error", payload: envelope });
          break;
      }
    });

    return unsubscribe;
  }, [sessionId]);

  // Reset when session changes
  useEffect(() => {
    reset();
  }, [sessionId, reset]);

  return { state, reset };
}
