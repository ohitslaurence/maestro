import { useCallback, useState } from "react";
import { useFreshRef, useEffectOnceWhen } from "rooks";
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

  // Keep fresh refs to avoid stale closures in the effect
  const workspaceIdRef = useFreshRef(workspaceId);
  const workspacePathRef = useFreshRef(workspacePath);

  // Run once when we have a workspace and autoConnect is enabled
  useEffectOnceWhen(
    async () => {
      const wsId = workspaceIdRef.current;
      const wsPath = workspacePathRef.current;
      if (!wsId || !wsPath) return;

      // First check status
      try {
        const status = await opencodeStatus(wsId);
        if (status.connected) {
          setIsConnected(true);
          return;
        }
      } catch {
        // Status check failed, proceed to connect
      }

      // Not connected, try to connect
      setIsConnecting(true);
      setError(null);

      try {
        await opencodeConnectWorkspace(wsId, wsPath);
        setIsConnected(true);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        setIsConnected(false);
      } finally {
        setIsConnecting(false);
      }
    },
    Boolean(workspaceId && workspacePath && autoConnect)
  );

  // Reset state when workspace changes
  useEffectOnceWhen(
    () => {
      setIsConnected(false);
      setIsConnecting(false);
      setError(null);
    },
    !workspaceId
  );

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
