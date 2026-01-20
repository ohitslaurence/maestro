import { useCallback, useEffect, useState } from "react";
import { listSessions } from "../../../services/tauri";

type UseSessionsOptions = {
  pollInterval?: number;
};

export type SessionsState = {
  sessions: string[];
  selectedSession: string | null;
  isLoading: boolean;
  error: string | null;
  selectSession: (sessionId: string | null) => void;
  refresh: () => Promise<void>;
};

export function useSessions(options: UseSessionsOptions = {}): SessionsState {
  const { pollInterval } = options;
  const [sessions, setSessions] = useState<string[]>([]);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await listSessions();
      setSessions(result);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const selectSession = useCallback((sessionId: string | null) => {
    setSelectedSession(sessionId);
  }, []);

  // Initial fetch
  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Optional polling
  useEffect(() => {
    if (!pollInterval) {
      return;
    }
    const interval = setInterval(() => {
      void refresh();
    }, pollInterval);
    return () => {
      clearInterval(interval);
    };
  }, [pollInterval, refresh]);

  return {
    sessions,
    selectedSession,
    isLoading,
    error,
    selectSession,
    refresh,
  };
}
