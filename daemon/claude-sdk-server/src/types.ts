/**
 * Core types for Claude SDK Server (§3)
 *
 * Types match OpenCode SDK v2 for frontend compatibility.
 */

// --- Permission Mode ---

export type PermissionMode = 'default' | 'acceptEdits' | 'bypassPermissions';

// --- Session ---

export interface Session {
  id: string;
  workspaceId: string;       // Matches daemon workspace (server-assigned)
  directory: string;         // Absolute path on VPS (server-assigned)
  title: string;             // User-visible title
  parentId?: string;         // For branched sessions
  resumeId?: string;         // SDK resume token
  modelId?: string;          // e.g., "claude-sonnet-4-20250514"
  createdAt: number;         // Unix ms
  updatedAt: number;         // Unix ms
  status: SessionStatus;
  permission: PermissionMode;
  summary?: string;          // Auto-generated summary
}

export type SessionStatus = 'idle' | 'busy' | 'error';

// --- Message ---

export interface MessageInfo {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant';
  createdAt: number;         // Unix ms
  completedAt?: number;      // Unix ms (for assistant)
  modelId?: string;          // e.g., "claude-sonnet-4-20250514"
  providerId?: string;       // e.g., "anthropic"
  cost?: number;             // USD
  tokens?: TokenUsage;
  error?: string;
}

export interface TokenUsage {
  input: number;
  output: number;
}

// --- Parts (OpenCode-compatible) ---

export type Part =
  | TextPart
  | ReasoningPart
  | ToolPart
  | StepStartPart
  | StepFinishPart
  | RetryPart;

export interface TextPart {
  id: string;
  messageId: string;
  type: 'text';
  text: string;
}

export interface ReasoningPart {
  id: string;
  messageId: string;
  type: 'reasoning';
  text: string;
}

export type ToolStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface ToolPart {
  id: string;
  messageId: string;
  type: 'tool';
  toolUseId: string;
  toolName: string;
  input: unknown;
  output?: string;
  status: ToolStatus;
  error?: string;
}

export interface StepStartPart {
  id: string;
  messageId: string;
  type: 'step-start';
}

export interface StepFinishPart {
  id: string;
  messageId: string;
  type: 'step-finish';
  usage?: TokenUsage;
  cost?: number;
}

export interface RetryPart {
  id: string;
  messageId: string;
  type: 'retry';
  attempt: number;
  reason: string;
}

// --- API Request/Response Types ---

export interface CreateSessionRequest {
  title: string;
  parentId?: string | null;
  permission?: PermissionMode;
  modelId?: string;
}

export interface ListSessionsQuery {
  start?: number;  // Only sessions updated after this Unix ms
  limit?: number;  // Max results (default 50)
  search?: string; // Filter by title
}

export interface SendMessageRequest {
  parts: MessagePartInput[];
}

export interface MessagePartInput {
  type: 'text';
  text: string;
}

export interface SendMessageResponse {
  info: MessageInfo;
  parts: Part[];
}

// --- Error Types (§6) ---

export enum ErrorCode {
  INVALID_REQUEST = 'INVALID_REQUEST',
  SESSION_NOT_FOUND = 'SESSION_NOT_FOUND',
  SESSION_BUSY = 'SESSION_BUSY',
  SDK_ERROR = 'SDK_ERROR',
  PERMISSION_DENIED = 'PERMISSION_DENIED',
  RATE_LIMITED = 'RATE_LIMITED',
  INTERNAL_ERROR = 'INTERNAL_ERROR',
}

export interface ApiError {
  code: ErrorCode;
  message: string;
  details?: unknown;
}

// --- SSE Event Types (§4) ---

export type SSEEventType =
  | 'session.created'
  | 'session.updated'
  | 'session.status'
  | 'session.error'
  | 'message.updated'
  | 'message.part.updated'
  | 'message.part.removed'
  | 'permission.asked'
  | 'permission.replied';

export interface SSEEvent<T = unknown> {
  type: SSEEventType;
  properties: T;
}

export interface SessionCreatedEvent {
  info: Session;
}

export interface SessionUpdatedEvent {
  info: Session;
}

export interface SessionStatusEvent {
  sessionId: string;
  status: {
    type: SessionStatus;
    attempt?: number;
    message?: string;
  };
}

export interface SessionErrorEvent {
  sessionId: string;
  error: string;
}

export interface MessageUpdatedEvent {
  info: MessageInfo;
}

export interface MessagePartUpdatedEvent {
  part: Part;
  delta?: string;
}

export interface MessagePartRemovedEvent {
  sessionId: string;
  messageId: string;
  partId: string;
}

export interface PermissionAskedEvent {
  id: string;
  sessionId: string;
  permission: string;
  tool?: string;
  patterns?: string[];
}

export interface PermissionRepliedEvent {
  sessionId: string;
  requestId: string;
  reply: 'allow' | 'deny';
}
