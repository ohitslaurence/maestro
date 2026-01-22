// --- Agent types ---

export type AgentHarness = "claude_code" | "open_code";

export type SessionStatus = "running" | "idle" | "stopped";

export type AgentSession = {
  id: string;
  name: string;
  harness: AgentHarness;
  project_path: string;
  status: SessionStatus;
};

// --- Daemon types ---

export type DaemonConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

export type DaemonStatus = {
  connected: boolean;
  host?: string;
  port?: number;
};

export type DaemonConnectionProfile = {
  id: string;
  name?: string;
  host: string;
  port: number;
  token: string;
  lastUsedAt?: number;
};

/** Session info from daemon's list_sessions */
export type SessionInfo = {
  path: string;
  name: string;
};

/** Extended session info from session_info command */
export type SessionInfoResult = {
  path: string;
  name: string;
  hasGit: boolean;
};

// --- Terminal types ---

export type TerminalSession = {
  id: string;
};

export type TerminalStatus = "idle" | "connecting" | "ready" | "error";

// --- Git types ---

export type GitFileStatus = {
  path: string;
  status: string;
  additions: number;
  deletions: number;
};

export type GitFileDiff = {
  path: string;
  diff: string;
};

export type GitLogEntry = {
  sha: string;
  summary: string;
  author: string;
  timestamp: number;
};

/** Result from git_status command */
export type GitStatusResult = {
  branchName: string;
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  totalAdditions: number;
  totalDeletions: number;
};

/** Result from git_diff command */
export type GitDiffResult = {
  files: GitFileDiff[];
  truncated: boolean;
  truncatedFiles: string[];
};

/** Result from git_log command */
export type GitLogResult = {
  entries: GitLogEntry[];
  ahead: number;
  behind: number;
  upstream?: string;
};

/** @deprecated Use GitStatusResult instead */
export type GitStatus = {
  branchName: string;
  files: GitFileStatus[];
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  totalAdditions: number;
  totalDeletions: number;
};

/** @deprecated Use GitLogResult instead */
export type GitLogResponse = {
  total: number;
  entries: GitLogEntry[];
  ahead: number;
  behind: number;
  aheadEntries: GitLogEntry[];
  behindEntries: GitLogEntry[];
  upstream: string | null;
};

// --- OpenCode types ---

export type OpenCodeConnectResult = {
  workspaceId: string;
  baseUrl: string;
};

export type OpenCodeStatusResult = {
  connected: boolean;
  baseUrl?: string;
};

export type OpenCodeSession = {
  id: string;
  [key: string]: unknown;
};

/** OpenCode event forwarded from daemon */
export type OpenCodeEvent = {
  workspaceId: string;
  eventType: string;
  event: unknown;
};

// --- OpenCode Thread UI types ---

export type OpenCodeToolStatus = "pending" | "running" | "completed" | "error";

/** UI-ready thread item (discriminated union) */
export type OpenCodeThreadItem =
  | { id: string; kind: "user-message"; text: string }
  | { id: string; kind: "assistant-message"; text: string }
  | { id: string; kind: "reasoning"; text: string; time?: { start: number; end?: number } }
  | {
      id: string;
      kind: "tool";
      tool: string;
      callId: string;
      status: OpenCodeToolStatus;
      title?: string;
      input: Record<string, unknown>;
      output?: string;
      error?: string;
    }
  | { id: string; kind: "patch"; hash: string; files: string[] }
  | {
      id: string;
      kind: "step-finish";
      cost: number;
      tokens: { input: number; output: number; reasoning: number };
    };

export type OpenCodeThreadStatus = "idle" | "processing" | "error";

// --- Claude SDK types (composer-options spec §3, §4) ---

/** Model information from SDK's supportedModels() */
export type ModelInfo = {
  value: string;       // e.g., "claude-sonnet-4-20250514"
  displayName: string; // e.g., "Claude Sonnet 4"
  description: string;
};

/** Thinking mode presets (spec §3) */
export type ThinkingMode = "off" | "low" | "medium" | "high" | "max";

/** Maps thinking modes to maxThinkingTokens values (spec §3) */
export const THINKING_BUDGETS: Record<ThinkingMode, number | undefined> = {
  off: undefined,  // SDK default (no thinking)
  low: 4_000,
  medium: 10_000,
  high: 16_000,
  max: 32_000,
};
