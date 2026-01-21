/**
 * Unified Streaming Event Schema types for frontend consumption.
 *
 * These types define the normalized event stream from all harnesses and LLM providers.
 * See specs/streaming-event-schema.md for the full specification.
 */

// ============================================================================
// Schema Constants
// ============================================================================

/** Current schema version. */
export const STREAM_SCHEMA_VERSION = "1.0";

/** Channel name for streaming events. */
export const STREAM_EVENT_CHANNEL = "agent:stream_event";

// ============================================================================
// Event Types (§3)
// ============================================================================

/** All possible streaming event types. */
export type StreamEventType =
  | "text_delta"
  | "tool_call_delta"
  | "tool_call_completed"
  | "completed"
  | "error"
  | "status"
  | "thinking_delta"
  | "artifact_delta"
  | "metadata";

// ============================================================================
// Payload Types (§3)
// ============================================================================

/** Payload for text_delta events. */
export type TextDeltaPayload = {
  text: string;
  role: "assistant";
};

/** Payload for tool_call_delta events. */
export type ToolCallDeltaPayload = {
  callId: string;
  toolName: string;
  argumentsDelta: string;
};

/** Status of a completed tool call. */
export type ToolCallStatus = "completed" | "failed" | "canceled";

/** Payload for tool_call_completed events. */
export type ToolCallCompletedPayload = {
  callId: string;
  toolName: string;
  arguments: Record<string, unknown>;
  output: string;
  status: ToolCallStatus;
  errorMessage?: string;
};

/** Reason for stream completion. */
export type CompletionReason = "stop" | "length" | "tool_error" | "user_abort";

/** Token usage statistics. */
export type TokenUsage = {
  inputTokens: number;
  outputTokens: number;
  reasoningTokens?: number;
};

/** Payload for completed events. */
export type CompletedPayload = {
  reason: CompletionReason;
  usage: TokenUsage;
};

/** Error codes for stream errors (§6). */
export type StreamErrorCode =
  | "provider_error"
  | "stream_gap"
  | "protocol_error"
  | "tool_error"
  | "session_aborted";

/** Payload for error events. */
export type ErrorPayload = {
  code: StreamErrorCode;
  message: string;
  recoverable: boolean;
  details?: Record<string, unknown>;
};

/** Agent processing state. */
export type AgentProcessingState =
  | "idle"
  | "processing"
  | "waiting"
  | "aborted";

/** Payload for status events. */
export type StatusPayload = {
  state: AgentProcessingState;
  detail?: string;
};

/** Payload for thinking_delta events (optional). */
export type ThinkingDeltaPayload = {
  text: string;
};

/** Payload for artifact_delta events (optional). */
export type ArtifactDeltaPayload = {
  artifactId: string;
  artifactType: string;
  contentDelta: string;
};

/** Payload for metadata events. */
export type MetadataPayload = {
  model: string;
  latencyMs: number;
  providerRequestId?: string;
};

// ============================================================================
// Discriminated Union Payloads
// ============================================================================

/** All payload types mapped by event type. */
export type StreamPayloadMap = {
  text_delta: TextDeltaPayload;
  tool_call_delta: ToolCallDeltaPayload;
  tool_call_completed: ToolCallCompletedPayload;
  completed: CompletedPayload;
  error: ErrorPayload;
  status: StatusPayload;
  thinking_delta: ThinkingDeltaPayload;
  artifact_delta: ArtifactDeltaPayload;
  metadata: MetadataPayload;
};

// ============================================================================
// StreamEvent Envelope (§3)
// ============================================================================

/** Known harness identifiers. */
export type HarnessId = "claude_code" | "open_code" | string;

/** Known provider identifiers. */
export type ProviderId = "anthropic" | "openai" | "opencode" | string;

/**
 * Base envelope fields common to all streaming events.
 *
 * Per §3: schemaVersion, eventId, sessionId, harness, streamId, seq, timestampMs, type are required.
 */
export type StreamEventBase = {
  /** Schema version, always "1.0". */
  schemaVersion: typeof STREAM_SCHEMA_VERSION;
  /** Unique identifier for this event. */
  eventId: string;
  /** Maestro session ID. */
  sessionId: string;
  /** Harness that produced this event. */
  harness: HarnessId;
  /** Provider that generated the response (optional). */
  provider?: ProviderId;
  /** Stable identifier for the current assistant response stream. */
  streamId: string;
  /** Monotonically increasing sequence number per streamId. */
  seq: number;
  /** Unix epoch milliseconds when event was created. */
  timestampMs: number;
  /** Provider message ID (optional). */
  messageId?: string;
  /** Parent message ID for threading (optional). */
  parentMessageId?: string;
};

/**
 * Typed streaming event with discriminated payload.
 *
 * Use type narrowing on the `type` field to access the correct payload shape.
 */
export type StreamEvent<T extends StreamEventType = StreamEventType> =
  StreamEventBase & {
    type: T;
    payload: StreamPayloadMap[T];
  };

// ============================================================================
// Type Guards
// ============================================================================

/** Type guard for text_delta events. */
export function isTextDelta(
  event: StreamEvent
): event is StreamEvent<"text_delta"> {
  return event.type === "text_delta";
}

/** Type guard for tool_call_delta events. */
export function isToolCallDelta(
  event: StreamEvent
): event is StreamEvent<"tool_call_delta"> {
  return event.type === "tool_call_delta";
}

/** Type guard for tool_call_completed events. */
export function isToolCallCompleted(
  event: StreamEvent
): event is StreamEvent<"tool_call_completed"> {
  return event.type === "tool_call_completed";
}

/** Type guard for completed events. */
export function isCompleted(
  event: StreamEvent
): event is StreamEvent<"completed"> {
  return event.type === "completed";
}

/** Type guard for error events. */
export function isError(event: StreamEvent): event is StreamEvent<"error"> {
  return event.type === "error";
}

/** Type guard for status events. */
export function isStatus(event: StreamEvent): event is StreamEvent<"status"> {
  return event.type === "status";
}

/** Type guard for thinking_delta events. */
export function isThinkingDelta(
  event: StreamEvent
): event is StreamEvent<"thinking_delta"> {
  return event.type === "thinking_delta";
}

/** Type guard for artifact_delta events. */
export function isArtifactDelta(
  event: StreamEvent
): event is StreamEvent<"artifact_delta"> {
  return event.type === "artifact_delta";
}

/** Type guard for metadata events. */
export function isMetadata(
  event: StreamEvent
): event is StreamEvent<"metadata"> {
  return event.type === "metadata";
}

// ============================================================================
// Buffering Utilities (§5)
// ============================================================================

/**
 * Buffer for reconstructing assistant responses from stream events.
 *
 * Per §5: Consumers may buffer in-memory by streamId to reconstruct assistant responses.
 */
export type StreamBuffer = {
  streamId: string;
  events: StreamEvent[];
  lastSeq: number;
  gaps: number[];
  completed: boolean;
};

/** Create a new stream buffer for a streamId. */
export function createStreamBuffer(streamId: string): StreamBuffer {
  return {
    streamId,
    events: [],
    lastSeq: -1,
    gaps: [],
    completed: false,
  };
}
