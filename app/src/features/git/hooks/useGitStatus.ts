import { useCallback, useEffect, useState } from "react";
import type { GitStatus } from "../../../types";
import { getGitStatus } from "../../../services/tauri";

type UseGitStatusOptions = {
  sessionId: string | null;
  pollInterval?: number;
};

export type GitStatusState = {
  status: GitStatus | null;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
};

export function useGitStatus({
  sessionId,
  pollInterval,
}: UseGitStatusOptions): GitStatusState {
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!sessionId) {
      setStatus(null);
      return;
    }
    setIsLoading(true);
    try {
      const result = await getGitStatus(sessionId);
      setStatus(result);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

  // Fetch when sessionId changes
  useEffect(() => {
    if (!sessionId) {
      setStatus(null);
      setError(null);
      return;
    }
    void refresh();
  }, [sessionId, refresh]);

  // Optional polling
  useEffect(() => {
    if (!sessionId || !pollInterval) {
      return;
    }
    const interval = setInterval(() => {
      void refresh();
    }, pollInterval);
    return () => {
      clearInterval(interval);
    };
  }, [sessionId, pollInterval, refresh]);

  return {
    status,
    isLoading,
    error,
    refresh,
  };
}
