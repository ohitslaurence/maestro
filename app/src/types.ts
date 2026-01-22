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

// --- Permission types (dynamic-tool-approvals spec §3) ---

/** Tool-specific context for permission UI */
export type PermissionMetadata = {
  description?: string;   // Human-readable description
  filePath?: string;      // File operations
  diff?: string;          // Edit: unified diff
  command?: string;       // Bash command
  url?: string;           // WebFetch URL
  query?: string;         // WebSearch query
};

/** SDK-provided suggestion for "Always Allow" */
export type PermissionSuggestion = {
  type: "addRules" | "addDirectories";
  patterns: string[];
  description: string;
};

/** Permission request from the SDK (dynamic-tool-approvals spec §3) */
export type PermissionRequest = {
  id: string;                          // UUID
  sessionId: string;
  messageId: string;                   // Current assistant message ID
  tool: string;                        // Tool name (Read, Write, Bash, etc.)
  permission: string;                  // Permission type (read, write, bash, etc.)
  input: Record<string, unknown>;      // Tool input (filepath, command, etc.)
  patterns: string[];                  // Affected patterns (exact strings)
  metadata: PermissionMetadata;        // Tool-specific context
  suggestions: PermissionSuggestion[]; // SDK-provided "always allow" patterns
  createdAt: number;
};

/** Reply type for permission requests */
export type PermissionReply = "allow" | "deny" | "always";

/** Response from permission reply endpoint */
export type PermissionReplyResponse = {
  success: boolean;
  error?: string;
};

/** Response from pending permissions endpoint */
export type PermissionPendingResponse = {
  requests: PermissionRequest[];
};

// --- Session Settings types (session-settings spec §3) ---

/** System prompt configuration modes */
export type SystemPromptMode = "default" | "append" | "custom";

/** System prompt configuration */
export type SystemPromptConfig = {
  mode: SystemPromptMode;
  content?: string;  // Required for 'append' and 'custom' modes
};

/** Session settings for Claude SDK sessions (session-settings spec §3.1) */
export type SessionSettings = {
  maxTurns: number;                    // Default: 100, Range: 1-1000
  systemPrompt: SystemPromptConfig;    // Default: { mode: 'default' }
  disallowedTools?: string[];          // Blocklist (removed from defaults)
};

/** Partial settings for update requests (session-settings spec §4.1) */
export type SessionSettingsUpdate = {
  maxTurns?: number | null;            // null = reset to default
  systemPrompt?: {
    mode?: SystemPromptMode;
    content?: string | null;           // null = reset to default
  } | null;
  disallowedTools?: string[] | null;   // null = reset to default
};

/** Default session settings (session-settings spec §3.1) */
export const DEFAULT_SESSION_SETTINGS: SessionSettings = {
  maxTurns: 100,
  systemPrompt: { mode: "default" },
  disallowedTools: undefined,
};

/** Available Claude tools for blocklist UI (session-settings spec Appendix) */
export const CLAUDE_TOOLS = [
  { id: "Read", name: "Read", description: "Read file contents", category: "files" },
  { id: "Write", name: "Write", description: "Write file contents", category: "files" },
  { id: "Edit", name: "Edit", description: "Edit file contents", category: "files" },
  { id: "Glob", name: "Glob", description: "Find files by pattern", category: "files" },
  { id: "Grep", name: "Grep", description: "Search file contents", category: "files" },
  { id: "Bash", name: "Bash", description: "Run shell commands", category: "system" },
  { id: "Task", name: "Task", description: "Spawn subagents", category: "agents" },
  { id: "TodoWrite", name: "TodoWrite", description: "Track tasks", category: "agents" },
  { id: "WebFetch", name: "WebFetch", description: "Fetch web pages", category: "web" },
  { id: "WebSearch", name: "WebSearch", description: "Search the web", category: "web" },
] as const;

/** Tool metadata type */
export type ClaudeTool = (typeof CLAUDE_TOOLS)[number];
