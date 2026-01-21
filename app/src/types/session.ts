/**
 * Session persistence types for frontend consumption.
 *
 * These types define the local-first session persistence data model.
 * See specs/session-persistence.md §3 for the full specification.
 */

import type { AgentStateSnapshot } from "./agent";

// ============================================================================
// Schema Constants
// ============================================================================

/** Current schema version for all persistence records. */
export const PERSISTENCE_SCHEMA_VERSION = 1;

// ============================================================================
// Privacy Settings (§3)
// ============================================================================

/** Privacy controls for a thread. */
export type ThreadPrivacy = {
  /** If true, thread is never synced to remote. */
  localOnly: boolean;
  /** If true, redact user inputs before persistence. */
  redactInputs: boolean;
  /** If true, redact assistant outputs before persistence. */
  redactOutputs: boolean;
};

// ============================================================================
// Thread Metadata (§3)
// ============================================================================

/** User-defined metadata for a thread. */
export type ThreadMetadata = {
  /** Tags for categorization. */
  tags: string[];
  /** Whether the thread is pinned to the top of lists. */
  pinned: boolean;
};

// ============================================================================
// ThreadRecord (§3)
// ============================================================================

/**
 * User-visible conversation container.
 *
 * One thread per workspace context. Stored at `threads/<thread_id>.json`.
 */
export type ThreadRecord = {
  /** Schema version for migrations. */
  schemaVersion: number;
  /** Unique thread identifier (e.g., "thr_123"). */
  id: string;
  /** User-visible title. */
  title: string;
  /** ISO 8601 timestamp when thread was created. */
  createdAt: string;
  /** ISO 8601 timestamp when thread was last updated. */
  updatedAt: string;
  /** Absolute path to the project/workspace. */
  projectPath: string;
  /** Agent harness identifier (e.g., "opencode", "claude_code"). */
  harness: string;
  /** LLM model identifier. */
  model: string;
  /** ID of the most recent session in this thread. */
  lastSessionId: string | null;
  /** Snapshot of agent state for resume. */
  stateSnapshot: AgentStateSnapshot | null;
  /** Privacy controls. */
  privacy: ThreadPrivacy;
  /** User-defined metadata. */
  metadata: ThreadMetadata;
};

// ============================================================================
// SessionRecord (§3)
// ============================================================================

/** Status of a session. */
export type SessionStatus = "running" | "completed" | "failed" | "stopped";

/** Agent configuration embedded in a session. */
export type SessionAgentConfig = {
  /** Harness identifier. */
  harness: string;
  /** Hash of the agent configuration for change detection. */
  configHash: string;
  /** Environment variables passed to the agent. */
  env: Record<string, string>;
};

/** Tool execution summary within a session. */
export type SessionToolRun = {
  /** Unique run identifier. */
  runId: string;
  /** Name of the tool that was executed. */
  toolName: string;
  /** Final status of the tool run. */
  status: "Succeeded" | "Failed" | "Canceled";
};

/**
 * Runtime instance of an agent.
 *
 * Linked to a thread. Stored at `sessions/<session_id>.json`.
 */
export type SessionRecord = {
  /** Schema version for migrations. */
  schemaVersion: number;
  /** Unique session identifier (e.g., "ses_456"). */
  id: string;
  /** ID of the thread this session belongs to. */
  threadId: string;
  /** Current status of the session. */
  status: SessionStatus;
  /** ISO 8601 timestamp when session started. */
  startedAt: string;
  /** ISO 8601 timestamp when session ended, or null if running. */
  endedAt: string | null;
  /** Absolute path to the workspace root. */
  workspaceRoot: string;
  /** Agent configuration. */
  agent: SessionAgentConfig;
  /** Summary of tool executions in this session. */
  toolRuns: SessionToolRun[];
};

// ============================================================================
// MessageRecord (§3)
// ============================================================================

/** Role of a message sender. */
export type MessageRole = "user" | "assistant" | "tool" | "system";

/**
 * A single message in the conversation log.
 *
 * Append-only. Stored at `messages/<thread_id>/<message_id>.json`.
 */
export type MessageRecord = {
  /** Schema version for migrations. */
  schemaVersion: number;
  /** Unique message identifier (e.g., "msg_001"). */
  id: string;
  /** ID of the thread this message belongs to. */
  threadId: string;
  /** ID of the session that produced this message. */
  sessionId: string;
  /** Role of the message sender. */
  role: MessageRole;
  /** Message content (text or tool output). */
  content: string;
  /** ISO 8601 timestamp when message was created. */
  createdAt: string;
  /** Tool call ID if this is a tool response message. */
  toolCallId: string | null;
};

// ============================================================================
// SyncQueueItem (§3, optional)
// ============================================================================

/** Entity type for sync operations. */
export type SyncEntityType = "thread" | "session" | "message";

/** Operation type for sync queue. */
export type SyncOperation = "upsert" | "delete";

/**
 * Pending sync intent for eventual consistency.
 *
 * Optional feature, guarded by sync enable flag and privacy.localOnly.
 * Stored at `sync_queue/<item_id>.json`.
 */
export type SyncQueueItem = {
  /** Schema version for migrations. */
  schemaVersion: number;
  /** Unique sync item identifier (e.g., "sq_789"). */
  id: string;
  /** Entity type being synced. */
  entity: SyncEntityType;
  /** ID of the entity being synced. */
  entityId: string;
  /** Operation to perform. */
  op: SyncOperation;
  /** SHA256 hash of the payload for deduplication. */
  payloadHash: string;
  /** Number of sync attempts so far. */
  attempts: number;
  /** ISO 8601 timestamp for next retry attempt. */
  nextAttemptAt: string;
  /** ISO 8601 timestamp when item was created. */
  createdAt: string;
};

// ============================================================================
// Index Types (§2, §3)
// ============================================================================

/** Summary of a thread for list views. */
export type ThreadSummary = {
  /** Thread ID. */
  id: string;
  /** Thread title. */
  title: string;
  /** ISO 8601 timestamp when thread was last updated. */
  updatedAt: string;
  /** Project path. */
  projectPath: string;
  /** Harness identifier. */
  harness: string;
  /** Whether the thread is pinned. */
  pinned: boolean;
};

/**
 * Index of recent threads for fast list queries.
 *
 * Stored at `index.json`. Rebuilt from threads/ if missing (§5).
 */
export type ThreadIndex = {
  /** Schema version for migrations. */
  schemaVersion: number;
  /** List of thread summaries, ordered by updatedAt descending. */
  threads: ThreadSummary[];
  /** ISO 8601 timestamp when index was last rebuilt. */
  rebuiltAt: string;
};

// ============================================================================
// Resume Result (§5)
// ============================================================================

/**
 * Result of resuming a thread, containing both thread and session records.
 */
export type ResumeResult = {
  /** The thread that was resumed. */
  thread: ThreadRecord;
  /** The session (new or existing) for the thread. */
  session: SessionRecord;
  /** True if a new session was created, false if existing session was resumed. */
  newSession: boolean;
};

// ============================================================================
// Event Payloads (§4)
// ============================================================================

/** Payload for session:persisted event. */
export type SessionPersistedPayload = {
  threadId: string;
  sessionId: string;
  updatedAt: string;
};

/** Payload for session:resumed event. */
export type SessionResumedPayload = {
  threadId: string;
  sessionId: string;
};

/** Payload for session:persistence_failed event. */
export type SessionPersistenceFailedPayload = {
  threadId?: string;
  sessionId?: string;
  code: string;
  message: string;
};

/** Payload for sync:enqueued event. */
export type SyncEnqueuedPayload = {
  entity: SyncEntityType;
  entityId: string;
  op: SyncOperation;
};

/** Payload for sync:failed event. */
export type SyncFailedPayload = {
  itemId: string;
  code: string;
  attempts: number;
};

// ============================================================================
// Event Channel Constants
// ============================================================================

/** Channel name for session persistence events. */
export const SESSION_PERSISTED_EVENT = "session:persisted";
export const SESSION_RESUMED_EVENT = "session:resumed";
export const SESSION_PERSISTENCE_FAILED_EVENT = "session:persistence_failed";
export const SYNC_ENQUEUED_EVENT = "sync:enqueued";
export const SYNC_FAILED_EVENT = "sync:failed";
