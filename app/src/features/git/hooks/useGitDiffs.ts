import { useCallback, useEffect, useState } from "react";
import type { GitFileDiff } from "../../../types";
import { getGitDiffs } from "../../../services/tauri";

type UseGitDiffsOptions = {
  sessionId: string | null;
};

export type GitDiffsState = {
  diffs: GitFileDiff[];
  selectedPath: string | null;
  isLoading: boolean;
  error: string | null;
  selectPath: (path: string | null) => void;
  refresh: () => Promise<void>;
};

export function useGitDiffs({ sessionId }: UseGitDiffsOptions): GitDiffsState {
  const [diffs, setDiffs] = useState<GitFileDiff[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!sessionId) {
      setDiffs([]);
      return;
    }
    setIsLoading(true);
    try {
      const result = await getGitDiffs(sessionId);
      setDiffs(result);
      setSelectedPath((previous) => {
        if (result.length === 0) {
          return null;
        }
        if (previous && result.some((diff) => diff.path === previous)) {
          return previous;
        }
        return result[0].path;
      });
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [sessionId]);

  const selectPath = useCallback((path: string | null) => {
    setSelectedPath(path);
  }, []);

  // Fetch when sessionId changes
  useEffect(() => {
    if (!sessionId) {
      setDiffs([]);
      setSelectedPath(null);
      setError(null);
      return;
    }
    void refresh();
  }, [sessionId, refresh]);

  return {
    diffs,
    selectedPath,
    isLoading,
    error,
    selectPath,
    refresh,
  };
}
