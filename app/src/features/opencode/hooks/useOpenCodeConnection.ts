import { useCallback, useEffect, useRef, useState } from "react";
import {
  opencodeConnectWorkspace,
  opencodeStatus,
} from "../../../services/tauri";

type UseOpenCodeConnectionOptions = {
  workspaceId: string | null;
  workspacePath: string | null;
  autoConnect?: boolean;
};

export type OpenCodeConnectionState = {
  isConnected: boolean;
  isConnecting: boolean;
  error: string | null;
  connect: () => Promise<void>;
};

export function useOpenCodeConnection({
  workspaceId,
  workspacePath,
  autoConnect = false,
}: UseOpenCodeConnectionOptions): OpenCodeConnectionState {
  const [isConnected, setIsConnected] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Track which workspace we've already connected to prevent loops
  const connectedWorkspaceRef = useRef<string | null>(null);

  // Auto-connect effect - runs once per workspace
  useEffect(() => {
    // Skip if no workspace, not auto-connect, or already connected to this workspace
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
        const status = await opencodeStatus(workspaceId);
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
      setError(null);

      try {
        await opencodeConnectWorkspace(workspaceId, workspacePath);
        if (!cancelled) {
          setIsConnected(true);
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setError(message);
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
      setError(null);
      connectedWorkspaceRef.current = null;
    }
  }, [workspaceId]);

  const connect = useCallback(async () => {
    if (!workspaceId || !workspacePath) {
      setError("No workspace selected");
      return;
    }

    setIsConnecting(true);
    setError(null);

    try {
      await opencodeConnectWorkspace(workspaceId, workspacePath);
      setIsConnected(true);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setIsConnected(false);
    } finally {
      setIsConnecting(false);
    }
  }, [workspaceId, workspacePath]);

  return {
    isConnected,
    isConnecting,
    error,
    connect,
  };
}
