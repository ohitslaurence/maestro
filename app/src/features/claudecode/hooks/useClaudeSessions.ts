import { useCallback, useEffect, useRef, useState } from "react";
import { claudeSdkSessionList, claudeSdkSessionMessages } from "../../../services/tauri";

/**
 * Session summary as returned by the Claude SDK server (§3 ClaudeSessionSummary).
 */
export type ClaudeSessionSummary = {
  id: string;
  /** Auto-generated from first user message (truncated to 80 chars); empty string if no messages yet */
  title: string;
  parentID?: string;
  /** Epoch milliseconds, UTC */
  time: { created: number; updated: number };
  settings: { maxTurns: number; systemPrompt: { mode: string }; disallowedTools?: string[] };
};

/**
 * Message info as returned by the history endpoint (§3 ClaudeMessageInfo).
 */
export type ClaudeMessageInfo = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  /** Epoch milliseconds, UTC */
  time: { created: number; completed?: number };
  summary?: { title?: string; body?: string | null };
  /** Parts included with history; empty array for messages with no parts yet */
  parts?: ClaudePart[];
};

/**
 * Part data from history (§3 ClaudePart).
 */
export type ClaudePart = {
  id: string;
  messageID: string;
  type: "text" | "reasoning" | "tool" | "step-start" | "step-finish" | "retry" | "agent" | "compaction";
  text?: string;
  tool?: string;
  input?: unknown;
  output?: unknown;
  error?: unknown;
  time?: { start?: number; end?: number };
};

type UseClaudeSessionsOptions = {
  workspaceId: string | null;
  isConnected: boolean;
};

export type ClaudeSessionsState = {
  /** List of sessions for this workspace */
  sessions: ClaudeSessionSummary[];
  /** Currently selected session ID */
  selectedSessionId: string | null;
  /** Whether sessions are loading */
  isLoading: boolean;
  /** Error from last fetch */
  error: string | null;
  /** Loaded message history for selected session */
  history: ClaudeMessageInfo[] | null;
  /** Whether history is loading */
  isLoadingHistory: boolean;
  /** Error from last history fetch */
  historyError: string | null;
  /** Select a session and load its history */
  selectSession: (sessionId: string | null) => void;
  /** Refresh the session list */
  refresh: () => Promise<void>;
};

/**
 * Hook for managing Claude session list and selection (claude-session-history spec §2, §5).
 *
 * Handles:
 * 1. Fetching session list from Claude SDK server
 * 2. Session selection state
 * 3. Loading message history for selected session
 * 4. AbortController for concurrent selection (§5 Concurrent Selection)
 */
export function useClaudeSessions({
  workspaceId,
  isConnected,
}: UseClaudeSessionsOptions): ClaudeSessionsState {
  const [sessions, setSessions] = useState<ClaudeSessionSummary[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<ClaudeMessageInfo[] | null>(null);
  const [isLoadingHistory, setIsLoadingHistory] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);

  // AbortController for concurrent selection (§5 Concurrent Selection)
  const historyAbortRef = useRef<AbortController | null>(null);

  // Fetch session list
  const fetchSessions = useCallback(async () => {
    if (!workspaceId || !isConnected) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await claudeSdkSessionList(workspaceId);
      if (Array.isArray(result)) {
        setSessions(result as ClaudeSessionSummary[]);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setIsLoading(false);
    }
  }, [workspaceId, isConnected]);

  // Load history for a session
  const loadHistory = useCallback(async (sessionId: string, signal: AbortSignal) => {
    if (!workspaceId) {
      return;
    }

    setIsLoadingHistory(true);
    setHistoryError(null);

    try {
      const result = await claudeSdkSessionMessages(workspaceId, sessionId);

      // Check if aborted before updating state
      if (signal.aborted) {
        return;
      }

      if (Array.isArray(result)) {
        setHistory(result as ClaudeMessageInfo[]);
      } else {
        setHistory([]);
      }
    } catch (err) {
      // Don't set error if aborted
      if (signal.aborted) {
        return;
      }
      const message = err instanceof Error ? err.message : String(err);
      setHistoryError(message);
    } finally {
      if (!signal.aborted) {
        setIsLoadingHistory(false);
      }
    }
  }, [workspaceId]);

  // Select a session and load its history (§5 Main Flow, §5 Concurrent Selection)
  const selectSession = useCallback((sessionId: string | null) => {
    // Cancel any in-flight history request (§5 Concurrent Selection)
    if (historyAbortRef.current) {
      historyAbortRef.current.abort();
      historyAbortRef.current = null;
    }

    setSelectedSessionId(sessionId);

    if (!sessionId) {
      setHistory(null);
      setHistoryError(null);
      return;
    }

    // Create new AbortController for this request
    const abortController = new AbortController();
    historyAbortRef.current = abortController;

    void loadHistory(sessionId, abortController.signal);
  }, [loadHistory]);

  // Refresh session list
  const refresh = useCallback(async () => {
    await fetchSessions();
  }, [fetchSessions]);

  // Auto-fetch sessions when connected
  useEffect(() => {
    if (isConnected && workspaceId) {
      void fetchSessions();
    }
  }, [isConnected, workspaceId, fetchSessions]);

  // Reset state when workspace changes
  useEffect(() => {
    if (!workspaceId) {
      setSessions([]);
      setSelectedSessionId(null);
      setHistory(null);
      setError(null);
      setHistoryError(null);
    }
  }, [workspaceId]);

  // Cleanup abort controller on unmount
  useEffect(() => {
    return () => {
      if (historyAbortRef.current) {
        historyAbortRef.current.abort();
      }
    };
  }, []);

  return {
    sessions,
    selectedSessionId,
    isLoading,
    error,
    history,
    isLoadingHistory,
    historyError,
    selectSession,
    refresh,
  };
}
