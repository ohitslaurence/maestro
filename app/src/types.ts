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
