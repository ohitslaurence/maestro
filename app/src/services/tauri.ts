import { invoke } from "@tauri-apps/api/core";
import type {
  AgentHarness,
  AgentSession,
  GitFileDiff,
  GitLogResponse,
  GitStatus,
  TerminalSession,
} from "../types";

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

/** List active agent sessions. */
export async function listSessions(): Promise<string[]> {
  return invokeCommand<string[]>("list_sessions");
}

/** Spawn a new agent session for a project. */
export async function spawnSession(
  harness: AgentHarness,
  projectPath: string,
): Promise<AgentSession> {
  return invokeCommand<AgentSession>("spawn_session", {
    harness,
    projectPath,
  });
}

/** Stop a running agent session. */
export async function stopSession(sessionId: string): Promise<void> {
  return invokeCommand("stop_session", { sessionId });
}

/** Open a terminal stream for a session. */
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

/** Write data into a terminal stream. */
export async function writeTerminal(
  sessionId: string,
  terminalId: string,
  data: string,
): Promise<void> {
  return invokeCommand("terminal_write", { sessionId, terminalId, data });
}

/** Resize a terminal stream. */
export async function resizeTerminal(
  sessionId: string,
  terminalId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invokeCommand("terminal_resize", { sessionId, terminalId, cols, rows });
}

/** Close a terminal stream. */
export async function closeTerminal(
  sessionId: string,
  terminalId: string,
): Promise<void> {
  return invokeCommand("terminal_close", { sessionId, terminalId });
}

/** Retrieve git status for a session workspace. */
export async function getGitStatus(sessionId: string): Promise<GitStatus> {
  return invokeCommand<GitStatus>("get_git_status", { sessionId });
}

/** Retrieve git diffs for a session workspace. */
export async function getGitDiffs(sessionId: string): Promise<GitFileDiff[]> {
  return invokeCommand<GitFileDiff[]>("get_git_diffs", { sessionId });
}

/** Retrieve git log entries for a session workspace. */
export async function getGitLog(
  sessionId: string,
  limit = 40,
): Promise<GitLogResponse> {
  return invokeCommand<GitLogResponse>("get_git_log", { sessionId, limit });
}
