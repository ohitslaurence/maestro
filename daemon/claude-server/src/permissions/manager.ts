/**
 * PermissionManager: Manages pending permission requests and session approvals.
 * Reference: specs/dynamic-tool-approvals.md §4 Interfaces
 */

import type { PermissionResult } from "@anthropic-ai/claude-agent-sdk";
import type { PendingPermission, PermissionRequest } from "./types";

const PERMISSION_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes

export type PermissionEventEmitter = {
  emit(eventType: string, payload: { type: string; properties?: Record<string, unknown> }): void;
};

export class PermissionManager {
  /**
   * Pending permissions: sessionId → Map<requestId, PendingPermission>
   */
  private pending = new Map<string, Map<string, PendingPermission>>();

  /**
   * Approved patterns per session: sessionId → Set<pattern>
   * Uses exact string matching, not glob patterns (§3, §10)
   */
  private approved = new Map<string, Set<string>>();

  /**
   * Reverse lookup for O(1) session finding: requestId → sessionId
   */
  private requestToSession = new Map<string, string>();

  /**
   * Event emitter for SSE events
   */
  private emitter: PermissionEventEmitter | null = null;

  /**
   * Set the event emitter for permission events.
   */
  setEmitter(emitter: PermissionEventEmitter): void {
    this.emitter = emitter;
  }

  /**
   * Emit a permission event via SSE.
   */
  private emitEvent(eventType: string, properties?: Record<string, unknown>): void {
    this.emitter?.emit(eventType, { type: eventType, properties });
  }

  /**
   * Request permission for a tool invocation.
   * Returns a Promise that blocks until user replies or timeout.
   */
  async request(
    sessionId: string,
    messageId: string,
    request: Omit<PermissionRequest, "id" | "createdAt" | "sessionId" | "messageId">,
    signal: AbortSignal
  ): Promise<PermissionResult> {
    const id = crypto.randomUUID();
    const fullRequest: PermissionRequest = {
      ...request,
      id,
      sessionId,
      messageId,
      createdAt: Date.now(),
    };

    // Check if already approved by "always" pattern (exact match)
    if (this.isApproved(sessionId, request.patterns)) {
      return { behavior: "allow", updatedInput: request.input };
    }

    // Create blocking Promise
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        this.reply(sessionId, id, "deny", "Permission request timed out");
      }, PERMISSION_TIMEOUT_MS);

      // Store pending request
      if (!this.pending.has(sessionId)) {
        this.pending.set(sessionId, new Map());
      }
      this.pending.get(sessionId)!.set(id, {
        request: fullRequest,
        resolve,
        reject,
        signal,
        timeoutId,
      });
      this.requestToSession.set(id, sessionId);

      // Emit SSE event
      this.emitEvent("permission.asked", { request: fullRequest });

      // Handle abort
      signal.addEventListener(
        "abort",
        () => {
          clearTimeout(timeoutId);
          this.pending.get(sessionId)?.delete(id);
          this.requestToSession.delete(id);
          reject(new Error("Aborted"));
        },
        { once: true }
      );
    });
  }

  /**
   * Reply to a pending permission request.
   * Returns true if the request was found and resolved.
   */
  reply(
    sessionId: string,
    requestId: string,
    reply: "allow" | "deny" | "always",
    message?: string
  ): boolean {
    const sessionPending = this.pending.get(sessionId);
    const pending = sessionPending?.get(requestId);
    if (!pending) return false;

    clearTimeout(pending.timeoutId);

    if (reply === "deny") {
      pending.resolve({
        behavior: "deny",
        message: message || "User denied permission",
      });
    } else {
      if (reply === "always") {
        this.addApprovals(sessionId, pending.request.patterns);
      }
      pending.resolve({
        behavior: "allow",
        updatedInput: pending.request.input,
      });
    }

    sessionPending!.delete(requestId);
    this.requestToSession.delete(requestId);
    this.emitEvent("permission.replied", { sessionId, requestId, reply });

    return true;
  }

  /**
   * Get pending permissions for a session (for reconnection).
   * If sessionId is not provided, returns all pending permissions.
   */
  getPending(sessionId?: string): PermissionRequest[] {
    if (sessionId) {
      const sessionPending = this.pending.get(sessionId);
      return sessionPending
        ? Array.from(sessionPending.values()).map((p) => p.request)
        : [];
    }
    // Return all pending (admin use)
    const all: PermissionRequest[] = [];
    for (const sessionMap of Array.from(this.pending.values())) {
      for (const p of Array.from(sessionMap.values())) {
        all.push(p.request);
      }
    }
    return all;
  }

  /**
   * Find session ID for a request ID (O(1) lookup).
   */
  findSessionForRequest(requestId: string): string | undefined {
    return this.requestToSession.get(requestId);
  }

  /**
   * Check if patterns are approved (exact string match).
   */
  private isApproved(sessionId: string, patterns: string[]): boolean {
    const approved = this.approved.get(sessionId);
    if (!approved) return false;
    return patterns.length > 0 && patterns.every((p) => approved.has(p));
  }

  /**
   * Add patterns to the approved set for a session.
   */
  private addApprovals(sessionId: string, patterns: string[]): void {
    if (!this.approved.has(sessionId)) {
      this.approved.set(sessionId, new Set());
    }
    patterns.forEach((p) => this.approved.get(sessionId)!.add(p));
  }

  /**
   * Clear all pending permissions and approvals for a session.
   * Called when a session ends or disconnects.
   */
  clearSession(sessionId: string): void {
    const sessionPending = this.pending.get(sessionId);
    if (sessionPending) {
      for (const pending of Array.from(sessionPending.values())) {
        clearTimeout(pending.timeoutId);
        pending.reject(new Error("Session cleared"));
      }
      for (const requestId of Array.from(sessionPending.keys())) {
        this.requestToSession.delete(requestId);
      }
    }
    this.pending.delete(sessionId);
    this.approved.delete(sessionId);
  }
}

/**
 * Singleton instance of PermissionManager.
 */
export const permissionManager = new PermissionManager();
