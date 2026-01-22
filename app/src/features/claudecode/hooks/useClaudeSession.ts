import { useCallback, useEffect, useRef, useState } from "react";
import {
  claudeSdkConnectWorkspace,
  claudeSdkStatus,
  claudeSdkSessionCreate,
  claudeSdkSessionPrompt,
  claudeSdkSessionAbort,
  registerSession,
} from "../../../services/tauri";

type UseClaudeSessionOptions = {
  workspaceId: string | null;
  workspacePath: string | null;
  autoConnect?: boolean;
};

type SessionCreateResult = {
  id: string;
  [key: string]: unknown;
};

export type ClaudeSessionState = {
  // Connection state
  isConnected: boolean;
  isConnecting: boolean;
  connectionError: string | null;
  connect: () => Promise<void>;

  // Session state
  sessionId: string | null;
  setSessionId: (id: string | null) => void;
  create: (title?: string) => Promise<string | null>;
  prompt: (message: string, options?: { sessionId?: string; model?: string; maxThinkingTokens?: number }) => Promise<void>;
  abort: () => Promise<void>;
  isPrompting: boolean;
};

/**
 * Combined hook for Claude SDK session management.
 *
 * Handles:
 * 1. Server connection via daemon (spawns Claude SDK server)
 * 2. Session CRUD (create, prompt, abort)
 *
 * SSE events are handled by useOpenCodeThread which subscribes to
 * the unified agent:stream_event channel (Claude SDK server emits
 * OpenCode-compatible events per spec).
 */
export function useClaudeSession({
  workspaceId,
  workspacePath,
  autoConnect = false,
}: UseClaudeSessionOptions): ClaudeSessionState {
  // Connection state
  const [isConnected, setIsConnected] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const connectedWorkspaceRef = useRef<string | null>(null);

  // Session state
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isPrompting, setIsPrompting] = useState(false);

  // Auto-connect effect - runs once per workspace
  useEffect(() => {
    if (!workspaceId || !workspacePath || !autoConnect) {
      return;
    }

    if (connectedWorkspaceRef.current === workspaceId) {
      return;
    }

    let cancelled = false;

    const checkAndConnect = async () => {
      // First check status
      try {
        const status = await claudeSdkStatus(workspaceId);
        if (cancelled) return;

        if (status.connected) {
          setIsConnected(true);
          connectedWorkspaceRef.current = workspaceId;
          return;
        }
      } catch {
        if (cancelled) return;
        // Status check failed, proceed to connect
      }

      // Mark as attempted before connecting
      connectedWorkspaceRef.current = workspaceId;
      setIsConnecting(true);
      setConnectionError(null);

      try {
        await claudeSdkConnectWorkspace(workspaceId, workspacePath);
        if (!cancelled) {
          setIsConnected(true);
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setConnectionError(message);
          setIsConnected(false);
        }
      } finally {
        if (!cancelled) {
          setIsConnecting(false);
        }
      }
    };

    void checkAndConnect();

    return () => {
      cancelled = true;
    };
  }, [workspaceId, workspacePath, autoConnect]);

  // Reset when workspace is cleared
  useEffect(() => {
    if (!workspaceId) {
      setIsConnected(false);
      setIsConnecting(false);
      setConnectionError(null);
      connectedWorkspaceRef.current = null;
      setSessionId(null);
    }
  }, [workspaceId]);

  const connect = useCallback(async () => {
    if (!workspaceId || !workspacePath) {
      setConnectionError("No workspace selected");
      return;
    }

    setIsConnecting(true);
    setConnectionError(null);

    try {
      await claudeSdkConnectWorkspace(workspaceId, workspacePath);
      setIsConnected(true);
      connectedWorkspaceRef.current = workspaceId;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setConnectionError(message);
      setIsConnected(false);
    } finally {
      setIsConnecting(false);
    }
  }, [workspaceId, workspacePath]);

  const create = useCallback(
    async (title?: string): Promise<string | null> => {
      if (!workspaceId || !workspacePath) {
        console.warn("[claude] Cannot create session: no workspace");
        return null;
      }
      if (!isConnected) {
        console.warn("[claude] Cannot create session: not connected");
        return null;
      }
      try {
        const result = (await claudeSdkSessionCreate(
          workspaceId,
          title
        )) as SessionCreateResult;
        const newId = result.id;

        // Register in local state machine registry so events can be routed
        await registerSession({
          sessionId: newId,
          name: title ?? "Untitled",
          projectPath: workspacePath,
          harness: "claude_code",
        });

        setSessionId(newId);
        return newId;
      } catch (error) {
        console.error("[claude] Failed to create session", error);
        return null;
      }
    },
    [workspaceId, workspacePath, isConnected]
  );

  const prompt = useCallback(
    async (message: string, options?: { sessionId?: string; model?: string; maxThinkingTokens?: number }): Promise<void> => {
      const targetSessionId = options?.sessionId ?? sessionId;
      if (!workspaceId || !targetSessionId) {
        console.warn("[claude] Cannot prompt: no workspace or session");
        return;
      }
      setIsPrompting(true);
      try {
        await claudeSdkSessionPrompt(workspaceId, targetSessionId, message, {
          model: options?.model,
          maxThinkingTokens: options?.maxThinkingTokens,
        });
      } catch (error) {
        console.error("[claude] Failed to send prompt", error);
        throw error;
      } finally {
        setIsPrompting(false);
      }
    },
    [workspaceId, sessionId]
  );

  const abort = useCallback(async (): Promise<void> => {
    if (!workspaceId || !sessionId) {
      console.warn("[claude] Cannot abort: no workspace or session");
      return;
    }
    // Optimistic UI - clear prompting state immediately
    setIsPrompting(false);
    try {
      await claudeSdkSessionAbort(workspaceId, sessionId);
    } catch (error) {
      console.error("[claude] Failed to abort session", error);
      throw error;
    }
  }, [workspaceId, sessionId]);

  return {
    // Connection
    isConnected,
    isConnecting,
    connectionError,
    connect,

    // Session
    sessionId,
    setSessionId,
    create,
    prompt,
    abort,
    isPrompting,
  };
}
