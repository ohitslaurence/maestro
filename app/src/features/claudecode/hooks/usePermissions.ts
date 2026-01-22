import { useCallback, useEffect, useRef, useState } from "react";
import type { PermissionRequest, PermissionReply, OpenCodeEvent } from "../../../types";
import { subscribeOpenCodeEvents } from "../../../services/events";
import {
  claudeSdkPermissionReply,
  claudeSdkPermissionPending,
} from "../../../services/tauri";

/**
 * Return type for the usePermissions hook (dynamic-tool-approvals spec §UI Components).
 */
export type UsePermissionsReturn = {
  /** Queue of pending permission requests */
  pendingQueue: PermissionRequest[];
  /** Current request to display (first in queue) */
  currentRequest: PermissionRequest | null;
  /** Reply to the current permission request */
  reply: (reply: PermissionReply, message?: string) => Promise<void>;
  /** Dismiss the current request (deny with default message) */
  dismiss: () => void;
  /** Whether a reply is in progress */
  isReplying: boolean;
  /** Error from last reply attempt */
  error: string | null;
};

type UsePermissionsOptions = {
  workspaceId: string | null;
  sessionId: string | null;
};

/**
 * Hook for managing permission requests (dynamic-tool-approvals spec §UI Components, §5).
 *
 * Handles:
 * - Queue-based state for concurrent permissions
 * - SSE event subscription for permission.asked events
 * - Reply callback that calls the permission reply endpoint
 * - Pending permission fetch on reconnect/mount
 * - Automatic queue management when permissions are replied
 */
export function usePermissions({
  workspaceId,
  sessionId,
}: UsePermissionsOptions): UsePermissionsReturn {
  const [pendingQueue, setPendingQueue] = useState<PermissionRequest[]>([]);
  const [isReplying, setIsReplying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Refs for values accessed in callbacks to avoid stale closures
  const workspaceIdRef = useRef(workspaceId);
  const sessionIdRef = useRef(sessionId);

  useEffect(() => {
    workspaceIdRef.current = workspaceId;
  }, [workspaceId]);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  // Track previous session to detect session switches
  const prevSessionRef = useRef<{ workspaceId: string | null; sessionId: string | null }>({
    workspaceId: null,
    sessionId: null,
  });

  // Fetch pending permissions on mount/reconnect (§5)
  useEffect(() => {
    const prev = prevSessionRef.current;
    const workspaceChanged = workspaceId !== prev.workspaceId;
    const sessionSwitched = prev.sessionId !== null && sessionId !== null && prev.sessionId !== sessionId;

    // Update ref for next comparison
    prevSessionRef.current = { workspaceId, sessionId };

    // Clear queue when workspace changes or switching sessions
    if (workspaceChanged || sessionSwitched) {
      setPendingQueue([]);
      setError(null);
    }

    // Fetch pending permissions for this session
    if (workspaceId && sessionId) {
      let cancelled = false;

      claudeSdkPermissionPending(workspaceId, sessionId)
        .then((response) => {
          if (cancelled) return;
          if (response.requests && response.requests.length > 0) {
            setPendingQueue((q) => {
              // Merge with existing queue, avoiding duplicates
              const existingIds = new Set(q.map((r) => r.id));
              const newRequests = response.requests.filter((r) => !existingIds.has(r.id));
              return [...q, ...newRequests];
            });
          }
        })
        .catch((err) => {
          if (cancelled) return;
          console.warn("[usePermissions] Failed to fetch pending permissions:", err);
        });

      return () => {
        cancelled = true;
      };
    }
  }, [workspaceId, sessionId]);

  // Subscribe to permission events (§UI Components)
  useEffect(() => {
    if (!workspaceId) return;

    const unsubscribe = subscribeOpenCodeEvents((event: OpenCodeEvent) => {
      // Filter by workspace
      if (event.workspaceId !== workspaceIdRef.current) return;

      // Handle permission.asked event
      if (event.eventType === "permission.asked") {
        const payload = event.event as { request?: PermissionRequest };
        if (payload.request) {
          // Filter by session if we have one
          const currentSessionId = sessionIdRef.current;
          if (currentSessionId && payload.request.sessionId !== currentSessionId) {
            return;
          }

          setPendingQueue((q) => {
            // Avoid duplicates
            if (q.some((r) => r.id === payload.request!.id)) {
              return q;
            }
            return [...q, payload.request!];
          });
        }
      }

      // Handle permission.replied event (remove from queue)
      if (event.eventType === "permission.replied") {
        const payload = event.event as { requestId?: string };
        if (payload.requestId) {
          setPendingQueue((q) => q.filter((r) => r.id !== payload.requestId));
        }
      }
    });

    return unsubscribe;
  }, [workspaceId]);

  // Reply callback (§UI Components)
  const reply = useCallback(
    async (replyType: PermissionReply, message?: string) => {
      const current = pendingQueue[0];
      if (!current) return;

      const wsId = workspaceIdRef.current;
      if (!wsId) return;

      setIsReplying(true);
      setError(null);

      try {
        const response = await claudeSdkPermissionReply(wsId, current.id, replyType, message);
        if (!response.success) {
          setError(response.error || "Failed to send reply");
        }
        // Note: SSE permission.replied event will remove from queue
      } catch (err) {
        const errMessage = err instanceof Error ? err.message : String(err);
        setError(errMessage);
        console.error("[usePermissions] Reply failed:", err);
      } finally {
        setIsReplying(false);
      }
    },
    [pendingQueue]
  );

  // Dismiss callback - deny with default message (§UI Components)
  const dismiss = useCallback(() => {
    if (pendingQueue.length > 0) {
      void reply("deny", "Dismissed");
    }
  }, [pendingQueue, reply]);

  return {
    pendingQueue,
    currentRequest: pendingQueue[0] ?? null,
    reply,
    dismiss,
    isReplying,
    error,
  };
}
