import { useCallback, useEffect, useState } from "react";
import type { SessionInfo } from "../../../types";
import { listSessions } from "../../../services/tauri";

type UseSessionsOptions = {
  pollInterval?: number;
  enabled?: boolean;
};

export type SessionsState = {
  sessions: SessionInfo[];
  selectedSession: string | null;
  isLoading: boolean;
  error: string | null;
  selectSession: (sessionPath: string | null) => void;
  refresh: () => Promise<void>;
};

export function useSessions(options: UseSessionsOptions = {}): SessionsState {
  const { pollInterval, enabled = true } = options;
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!enabled) {
      setSessions([]);
      setIsLoading(false);
      return;
    }

    try {
      const result = await listSessions();
      setSessions(result);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      // Don't show error for expected disconnection states
      if (message === "daemon_disconnected" || message === "daemon_not_configured") {
        setSessions([]);
        setError(null);
      } else {
        setError(message);
      }
    } finally {
      setIsLoading(false);
    }
  }, [enabled]);

  const selectSession = useCallback((sessionPath: string | null) => {
    setSelectedSession(sessionPath);
  }, []);

  // Reset selection when sessions change (session might have been removed)
  useEffect(() => {
    if (selectedSession && !sessions.some((s) => s.path === selectedSession)) {
      setSelectedSession(null);
    }
  }, [sessions, selectedSession]);

  // Initial fetch
  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Optional polling
  useEffect(() => {
    if (!pollInterval || !enabled) {
      return;
    }
    const interval = setInterval(() => {
      void refresh();
    }, pollInterval);
    return () => {
      clearInterval(interval);
    };
  }, [pollInterval, enabled, refresh]);

  return {
    sessions,
    selectedSession,
    isLoading,
    error,
    selectSession,
    refresh,
  };
}
