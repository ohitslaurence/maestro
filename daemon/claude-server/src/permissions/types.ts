/**
 * Permission types for dynamic tool approvals.
 * Reference: specs/dynamic-tool-approvals.md §3 Data Model
 */

import type { PermissionResult } from "@anthropic-ai/claude-agent-sdk";

/**
 * Tool-specific context for permission requests.
 */
export interface PermissionMetadata {
  /** Human-readable description */
  description?: string;
  /** File path for file operations */
  filePath?: string;
  /** Unified diff for Edit operations */
  diff?: string;
  /** Command string for Bash operations */
  command?: string;
  /** URL for WebFetch operations */
  url?: string;
  /** Query for WebSearch operations */
  query?: string;
}

/**
 * SDK-provided suggestion for "always allow" patterns.
 */
export interface PermissionSuggestion {
  type: "addRules" | "addDirectories";
  patterns: string[];
  description: string;
}

/**
 * Full permission request object sent to the UI.
 */
export interface PermissionRequest {
  /** UUID for this request */
  id: string;
  /** Session this request belongs to */
  sessionId: string;
  /** Current assistant message ID */
  messageId: string;
  /** Tool name (Read, Write, Bash, etc.) */
  tool: string;
  /** Permission type (read, write, bash, etc.) */
  permission: string;
  /** Tool input (filepath, command, etc.) */
  input: Record<string, unknown>;
  /** Affected patterns (exact strings, not globs) */
  patterns: string[];
  /** Tool-specific context */
  metadata: PermissionMetadata;
  /** SDK-provided "always allow" patterns */
  suggestions: PermissionSuggestion[];
  /** Creation timestamp */
  createdAt: number;
}

/**
 * Request body for permission reply endpoint.
 */
export interface PermissionReplyRequest {
  reply: "allow" | "deny" | "always";
  /** Feedback message on deny */
  message?: string;
}

/**
 * Response from permission reply endpoint.
 */
export interface PermissionReplyResponse {
  success: boolean;
  error?: string;
}

/**
 * Internal state for a pending permission request.
 */
export interface PendingPermission {
  request: PermissionRequest;
  resolve: (result: PermissionResult) => void;
  reject: (error: Error) => void;
  signal: AbortSignal;
  timeoutId: ReturnType<typeof setTimeout>;
}
