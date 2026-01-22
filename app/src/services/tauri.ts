import { invoke } from "./bridge";
import type {
  AgentHarness,
  AgentSession,
  DaemonStatus,
  GitDiffResult,
  GitFileDiff,
  GitLogResult,
  GitStatusResult,
  ModelInfo,
  OpenCodeConnectResult,
  OpenCodeStatusResult,
  PermissionPendingResponse,
  PermissionReply,
  PermissionReplyResponse,
  SessionInfo,
  SessionInfoResult,
  SessionSettingsUpdate,
  TerminalSession,
} from "../types";
import type {
  MessageRecord,
  ResumeResult,
  SessionAgentConfig,
  SessionRecord,
  SessionStatus,
  ThreadIndex,
  ThreadRecord,
  ThreadSummary,
} from "../types/session";

async function invokeCommand<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, payload);
  } catch (error) {
    if (typeof error === "string") {
      throw error;
    }
    if (error instanceof Error) {
      throw error.message;
    }
    throw "Unknown Tauri error";
  }
}

// --- Daemon connection commands ---

/** Configure daemon connection (saves to disk) */
export async function daemonConfigure(
  host: string,
  port: number,
  token: string,
): Promise<void> {
  return invokeCommand("daemon_configure", { host, port, token });
}

/** Connect to configured daemon */
export async function daemonConnect(): Promise<{ connected: boolean }> {
  return invokeCommand<{ connected: boolean }>("daemon_connect");
}

/** Disconnect from daemon */
export async function daemonDisconnect(): Promise<void> {
  return invokeCommand("daemon_disconnect");
}

/** Get daemon connection status */
export async function daemonStatus(): Promise<DaemonStatus> {
  return invokeCommand<DaemonStatus>("daemon_status");
}

// --- Session commands ---

/** List active sessions from daemon */
export async function listSessions(): Promise<SessionInfo[]> {
  return invokeCommand<SessionInfo[]>("list_sessions");
}

/** Get detailed session info */
export async function sessionInfo(sessionId: string): Promise<SessionInfoResult> {
  return invokeCommand<SessionInfoResult>("session_info", { sessionId });
}

/** Spawn a new agent session for a project (local only) */
export async function spawnSession(
  harness: AgentHarness,
  projectPath: string,
): Promise<AgentSession> {
  return invokeCommand<AgentSession>("spawn_session", {
    harness,
    projectPath,
  });
}

/** Stop a running agent session (local only) */
export async function stopSession(sessionId: string): Promise<void> {
  return invokeCommand("stop_session", { sessionId });
}

// --- Terminal commands ---

/** Open a terminal stream for a session */
export async function openTerminal(
  sessionId: string,
  terminalId: string,
  cols: number,
  rows: number,
): Promise<TerminalSession> {
  return invokeCommand<TerminalSession>("terminal_open", {
    sessionId,
    terminalId,
    cols,
    rows,
  });
}

/** Write data into a terminal stream */
export async function writeTerminal(
  sessionId: string,
  terminalId: string,
  data: string,
): Promise<void> {
  return invokeCommand("terminal_write", { sessionId, terminalId, data });
}

/** Resize a terminal stream */
export async function resizeTerminal(
  sessionId: string,
  terminalId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invokeCommand("terminal_resize", { sessionId, terminalId, cols, rows });
}

/** Close a terminal stream */
export async function closeTerminal(
  sessionId: string,
  terminalId: string,
): Promise<void> {
  return invokeCommand("terminal_close", { sessionId, terminalId });
}

// --- Git commands ---

/** Retrieve git status for a session workspace */
export async function gitStatus(sessionId: string): Promise<GitStatusResult> {
  return invokeCommand<GitStatusResult>("git_status", { sessionId });
}

/** Retrieve git diffs for a session workspace */
export async function gitDiff(sessionId: string): Promise<GitDiffResult> {
  return invokeCommand<GitDiffResult>("git_diff", { sessionId });
}

/** Retrieve git log entries for a session workspace */
export async function gitLog(
  sessionId: string,
  limit = 40,
): Promise<GitLogResult> {
  return invokeCommand<GitLogResult>("git_log", { sessionId, limit });
}

// --- Deprecated commands (for backward compatibility) ---

/** @deprecated Use gitStatus instead */
export async function getGitStatus(sessionId: string): Promise<GitStatusResult> {
  return gitStatus(sessionId);
}

/** @deprecated Use gitDiff instead */
export async function getGitDiffs(sessionId: string): Promise<GitFileDiff[]> {
  const result = await gitDiff(sessionId);
  return result.files;
}

/** @deprecated Use gitLog instead */
export async function getGitLog(
  sessionId: string,
  limit = 40,
): Promise<GitLogResult> {
  return gitLog(sessionId, limit);
}

// --- OpenCode commands ---

/** Connect to OpenCode server for a workspace */
export async function opencodeConnectWorkspace(
  workspaceId: string,
  workspacePath: string,
): Promise<OpenCodeConnectResult> {
  return invokeCommand<OpenCodeConnectResult>("opencode_connect_workspace", {
    workspaceId,
    workspacePath,
  });
}

/** Disconnect OpenCode server for a workspace */
export async function opencodeDisconnectWorkspace(
  workspaceId: string,
): Promise<{ ok: boolean }> {
  return invokeCommand<{ ok: boolean }>("opencode_disconnect_workspace", {
    workspaceId,
  });
}

/** Get OpenCode status for a workspace */
export async function opencodeStatus(
  workspaceId: string,
): Promise<OpenCodeStatusResult> {
  return invokeCommand<OpenCodeStatusResult>("opencode_status", { workspaceId });
}

/** List OpenCode sessions for a workspace */
export async function opencodeSessionList(
  workspaceId: string,
): Promise<unknown> {
  return invokeCommand<unknown>("opencode_session_list", { workspaceId });
}

/** Create a new OpenCode session */
export async function opencodeSessionCreate(
  workspaceId: string,
  title?: string,
): Promise<unknown> {
  return invokeCommand<unknown>("opencode_session_create", { workspaceId, title });
}

/** Send a prompt to an OpenCode session */
export async function opencodeSessionPrompt(
  workspaceId: string,
  sessionId: string,
  message: string,
): Promise<unknown> {
  return invokeCommand<unknown>("opencode_session_prompt", {
    workspaceId,
    sessionId,
    message,
  });
}

/** Abort an OpenCode session */
export async function opencodeSessionAbort(
  workspaceId: string,
  sessionId: string,
): Promise<unknown> {
  return invokeCommand<unknown>("opencode_session_abort", {
    workspaceId,
    sessionId,
  });
}

/** Get messages for an OpenCode session (history rehydration) */
export async function opencodeSessionMessages(
  workspaceId: string,
  sessionId: string,
): Promise<unknown> {
  return invokeCommand<unknown>("opencode_session_messages", {
    workspaceId,
    sessionId,
  });
}

// --- Claude SDK commands (mirrors OpenCode API per claude-sdk-server spec §4) ---

/** Connect to Claude SDK server for a workspace */
export async function claudeSdkConnectWorkspace(
  workspaceId: string,
  workspacePath: string,
): Promise<OpenCodeConnectResult> {
  return invokeCommand<OpenCodeConnectResult>("claude_sdk_connect_workspace", {
    workspaceId,
    workspacePath,
  });
}

/** Disconnect Claude SDK server for a workspace */
export async function claudeSdkDisconnectWorkspace(
  workspaceId: string,
): Promise<{ ok: boolean }> {
  return invokeCommand<{ ok: boolean }>("claude_sdk_disconnect_workspace", {
    workspaceId,
  });
}

/** Get Claude SDK status for a workspace */
export async function claudeSdkStatus(
  workspaceId: string,
): Promise<OpenCodeStatusResult> {
  return invokeCommand<OpenCodeStatusResult>("claude_sdk_status", { workspaceId });
}

/** List Claude SDK sessions for a workspace */
export async function claudeSdkSessionList(
  workspaceId: string,
): Promise<unknown> {
  return invokeCommand<unknown>("claude_sdk_session_list", { workspaceId });
}

/** Create a new Claude SDK session */
export async function claudeSdkSessionCreate(
  workspaceId: string,
  title?: string,
): Promise<unknown> {
  return invokeCommand<unknown>("claude_sdk_session_create", { workspaceId, title });
}

/** Send a prompt to a Claude SDK session (composer-options spec §4) */
export async function claudeSdkSessionPrompt(
  workspaceId: string,
  sessionId: string,
  message: string,
  options?: { model?: string; maxThinkingTokens?: number },
): Promise<unknown> {
  return invokeCommand<unknown>("claude_sdk_session_prompt", {
    workspaceId,
    sessionId,
    message,
    model: options?.model,
    maxThinkingTokens: options?.maxThinkingTokens,
  });
}

/** Abort a Claude SDK session */
export async function claudeSdkSessionAbort(
  workspaceId: string,
  sessionId: string,
): Promise<unknown> {
  return invokeCommand<unknown>("claude_sdk_session_abort", {
    workspaceId,
    sessionId,
  });
}

/** Fetch available models from Claude SDK server (composer-options spec §4) */
export async function claudeSdkModels(
  workspaceId: string,
): Promise<ModelInfo[]> {
  return invokeCommand<ModelInfo[]>("claude_sdk_models", { workspaceId });
}

/** Reply to a pending permission request (dynamic-tool-approvals spec §4) */
export async function claudeSdkPermissionReply(
  workspaceId: string,
  requestId: string,
  reply: PermissionReply,
  message?: string,
): Promise<PermissionReplyResponse> {
  return invokeCommand<PermissionReplyResponse>("claude_sdk_permission_reply", {
    workspaceId,
    requestId,
    reply,
    message,
  });
}

/** Get pending permission requests (dynamic-tool-approvals spec §4) */
export async function claudeSdkPermissionPending(
  workspaceId: string,
  sessionId?: string,
): Promise<PermissionPendingResponse> {
  return invokeCommand<PermissionPendingResponse>("claude_sdk_permission_pending", {
    workspaceId,
    sessionId,
  });
}

/** Update session settings (session-settings spec §4) */
export async function claudeSdkSessionSettingsUpdate(
  workspaceId: string,
  sessionId: string,
  settings: SessionSettingsUpdate,
): Promise<unknown> {
  return invokeCommand<unknown>("claude_sdk_session_settings_update", {
    workspaceId,
    sessionId,
    settings,
  });
}

// --- Session Registry commands (state machine wiring) ---

export type HarnessType = "claude_code" | "open_code";

export interface RegisterSessionParams {
  sessionId: string;
  name: string;
  projectPath: string;
  harness: HarnessType;
}

/**
 * Register a session in the local state machine registry.
 * Must be called after creating a session via the daemon so that
 * stream events can be routed to the state machine.
 */
export async function registerSession(params: RegisterSessionParams): Promise<void> {
  return invokeCommand("register_session", { params });
}

// --- Storage commands (session-persistence §4) ---

/** List all thread summaries */
export async function listThreads(): Promise<ThreadSummary[]> {
  return invokeCommand<ThreadSummary[]>("list_threads");
}

/** Load a thread by ID */
export async function loadThread(threadId: string): Promise<ThreadRecord> {
  return invokeCommand<ThreadRecord>("load_thread", { threadId });
}

/** Save a thread (create or update) */
export async function saveThread(thread: ThreadRecord): Promise<ThreadRecord> {
  return invokeCommand<ThreadRecord>("save_thread", { thread });
}

/** Create a new session for a thread */
export async function createSession(
  threadId: string,
  workspaceRoot: string,
  agentConfig: SessionAgentConfig,
): Promise<SessionRecord> {
  return invokeCommand<SessionRecord>("create_session", {
    threadId,
    workspaceRoot,
    agentConfig,
  });
}

/** Mark a session as ended */
export async function markSessionEnded(
  sessionId: string,
  status: SessionStatus,
): Promise<void> {
  return invokeCommand("mark_session_ended", { sessionId, status });
}

/** Append a message to the conversation log */
export async function appendMessage(message: MessageRecord): Promise<void> {
  return invokeCommand("append_message", { message });
}

/** List all messages for a thread */
export async function listMessages(threadId: string): Promise<MessageRecord[]> {
  return invokeCommand<MessageRecord[]>("list_messages", { threadId });
}

/** Delete a thread by ID */
export async function deleteThread(threadId: string): Promise<void> {
  return invokeCommand("delete_thread", { threadId });
}

/** Rebuild the thread index (§5) */
export async function rebuildIndex(): Promise<ThreadIndex> {
  return invokeCommand<ThreadIndex>("rebuild_index");
}

/**
 * Resume a thread (§5: Resume Flow).
 *
 * Loads the thread, checks if the last session is still running,
 * and either resumes it or creates a new session.
 * Emits `session:resumed` event on success.
 */
export async function resumeThread(
  threadId: string,
  agentConfig: SessionAgentConfig,
): Promise<ResumeResult> {
  return invokeCommand<ResumeResult>("resume_thread", { threadId, agentConfig });
}
