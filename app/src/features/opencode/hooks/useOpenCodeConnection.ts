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
  const connectAttemptedRef = useRef<string | null>(null);

  // Check connection status and auto-connect on workspace change
  useEffect(() => {
    if (!workspaceId || !workspacePath) {
      setIsConnected(false);
      setIsConnecting(false);
      setError(null);
      connectAttemptedRef.current = null;
      return;
    }

    // Skip if we already attempted for this workspace
    if (connectAttemptedRef.current === workspaceId) {
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
          connectAttemptedRef.current = workspaceId;
          return;
        }
      } catch {
        // Status check failed, proceed to connect if autoConnect
      }

      if (cancelled) return;

      // Not connected, try to connect if autoConnect enabled
      if (autoConnect) {
        connectAttemptedRef.current = workspaceId;
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
      }
    };

    void checkAndConnect();

    return () => {
      cancelled = true;
    };
  }, [workspaceId, workspacePath, autoConnect]);

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
      connectAttemptedRef.current = workspaceId;
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
