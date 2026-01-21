// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from "vscode";
import { EventEmitter } from "events";
import type { AcpClient } from "../acp/acpClient";
import type { Logger } from "../logging";

export interface LoomSession {
  id: string;
  title: string;
  cwd: string;
  createdAt: number;
  lastUsedAt: number;
  isActive: boolean;
}

export interface WorkspaceSessionsState {
  activeSessionId?: string;
  sessions: LoomSession[];
}

const STATE_KEY = "loom.sessions";

export class SessionManager extends EventEmitter {
  private sessions: Map<string, LoomSession> = new Map();
  private activeSessionId: string | undefined;

  constructor(
    private readonly workspaceState: vscode.Memento,
    private readonly acpClient: AcpClient,
    private readonly logger: Logger
  ) {
    super();
    this.loadFromState();
  }

  loadFromState(): void {
    const state = this.workspaceState.get<WorkspaceSessionsState>(STATE_KEY);
    if (state) {
      this.sessions.clear();
      for (const session of state.sessions) {
        // Handle legacy sessions that don't have cwd
        if (!session.cwd) {
          const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
          session.cwd = workspaceRoot || process.cwd();
        }
        this.sessions.set(session.id, session);
      }
      this.activeSessionId = state.activeSessionId;
      this.logger.info("Loaded sessions from state", {
        sessionCount: this.sessions.size,
        activeSessionId: this.activeSessionId,
      });
    } else {
      this.logger.info("No existing sessions found in state", {
        sessionCount: 0,
        activeSessionId: undefined,
      });
    }
  }

  async saveToState(): Promise<void> {
    const state: WorkspaceSessionsState = {
      activeSessionId: this.activeSessionId,
      sessions: Array.from(this.sessions.values()),
    };
    await this.workspaceState.update(STATE_KEY, state);
    this.logger.info("Saved sessions to state", {
      sessionCount: this.sessions.size,
    });
  }

  getActiveSession(): LoomSession | undefined {
    if (!this.activeSessionId) {
      return undefined;
    }
    return this.sessions.get(this.activeSessionId);
  }

  getAllSessions(): LoomSession[] {
    return Array.from(this.sessions.values()).sort(
      (a, b) => b.lastUsedAt - a.lastUsedAt
    );
  }

  async createNewSession(cwd: string): Promise<LoomSession> {
    const sessionId = await this.acpClient.newSession(cwd);
    const now = Date.now();
    const session: LoomSession = {
      id: sessionId,
      title: "New Session",
      cwd,
      createdAt: now,
      lastUsedAt: now,
      isActive: true,
    };

    if (this.activeSessionId) {
      const previousSession = this.sessions.get(this.activeSessionId);
      if (previousSession) {
        previousSession.isActive = false;
      }
    }

    this.sessions.set(session.id, session);
    this.activeSessionId = session.id;

    this.logger.info("Created new session", {
      cwd,
      sessionId: session.id,
      title: session.title,
    });

    await this.saveToState();
    this.emit("sessionCreated", session);
    this.emit("activeSessionChanged", session);

    return session;
  }

  async setActiveSession(sessionId: string): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      this.logger.warn("Attempted to set non-existent session as active", {
        sessionId,
      });
      return;
    }

    if (this.activeSessionId) {
      const previousSession = this.sessions.get(this.activeSessionId);
      if (previousSession) {
        previousSession.isActive = false;
      }
    }

    try {
      await this.acpClient.loadSession(sessionId, session.cwd);
      this.logger.info("Set active session", {
        sessionId,
        success: true,
      });
    } catch (error) {
      this.logger.warn("Failed to load session from server, continuing anyway", {
        sessionId,
        fallback: true,
        error: error instanceof Error ? error.message : String(error),
      });
    }

    session.isActive = true;
    session.lastUsedAt = Date.now();
    this.activeSessionId = sessionId;

    await this.saveToState();
    this.emit("activeSessionChanged", session);
  }

  updateSessionTitle(sessionId: string, title: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.title = title;
      this.saveToState().catch((err) => {
        this.logger.warn("Failed to persist session title update", {
          sessionId,
          error: err instanceof Error ? err.message : String(err),
        });
      });
      this.emit("sessionUpdated", session);
    }
  }

  updateSessionLastUsed(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.lastUsedAt = Date.now();
      this.saveToState().catch((err) => {
        this.logger.warn("Failed to persist session lastUsedAt", {
          sessionId,
          error: err instanceof Error ? err.message : String(err),
        });
      });
      this.emit("sessionUpdated", session);
    }
  }

  deleteSession(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (!session) {
      return;
    }

    this.sessions.delete(sessionId);

    if (this.activeSessionId === sessionId) {
      this.activeSessionId = undefined;
    }

    this.logger.info("Deleted session", {
      sessionId,
    });

    this.saveToState().catch((err) => {
      this.logger.warn("Failed to persist session deletion", {
        sessionId,
        error: err instanceof Error ? err.message : String(err),
      });
    });

    this.emit("sessionDeleted", sessionId);
  }

  async ensureActiveSession(cwd: string): Promise<LoomSession> {
    const activeSession = this.getActiveSession();
    if (activeSession) {
      return activeSession;
    }
    return this.createNewSession(cwd);
  }

  deriveSessionTitleFromMessage(text: string): string {
    const firstLine = text.split("\n")[0] || "";
    return firstLine.slice(0, 50);
  }
}
