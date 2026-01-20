import { useCallback, useEffect, useState } from "react";
import {
  opencodeConnectWorkspace,
  opencodeStatus,
} from "../../../services/tauri";

type UseOpenCodeConnectionOptions = {
  workspaceId: string | null;
  workspacePath: string | null;
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
}: UseOpenCodeConnectionOptions): OpenCodeConnectionState {
  const [isConnected, setIsConnected] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Check connection status on mount/workspace change
  useEffect(() => {
    if (!workspaceId) {
      setIsConnected(false);
      setError(null);
      return;
    }

    let cancelled = false;

    const checkStatus = async () => {
      try {
        const status = await opencodeStatus(workspaceId);
        if (!cancelled) {
          setIsConnected(status.connected);
        }
      } catch (err) {
        if (!cancelled) {
          setIsConnected(false);
        }
      }
    };

    void checkStatus();

    return () => {
      cancelled = true;
    };
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
