import { useCallback } from "react";
import type { ClaudeSessionSummary } from "../hooks/useClaudeSessions";

type ClaudeSessionListProps = {
  sessions: ClaudeSessionSummary[];
  selectedSessionId: string | null;
  isLoading: boolean;
  error: string | null;
  onSelect: (sessionId: string | null) => void;
  onNewSession: () => void;
  onRefresh: () => void;
};

/**
 * Session list for Claude SDK sessions (claude-session-history spec §2, §5).
 *
 * Displays:
 * - List of sessions sorted by updated time (most recent first)
 * - Selection state with visual indicator
 * - Empty state with "New Session" CTA (§5 Edge Cases)
 * - Loading and error states
 */
export function ClaudeSessionList({
  sessions,
  selectedSessionId,
  isLoading,
  error,
  onSelect,
  onNewSession,
  onRefresh,
}: ClaudeSessionListProps) {
  const handleSelect = useCallback(
    (sessionId: string) => {
      onSelect(sessionId === selectedSessionId ? null : sessionId);
    },
    [onSelect, selectedSessionId]
  );

  const formatTime = (timestamp: number): string => {
    const date = new Date(timestamp);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();

    if (isToday) {
      return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    }

    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  };

  // Empty state (§5 Edge Cases)
  if (!isLoading && sessions.length === 0) {
    return (
      <div className="claude-session-list claude-session-list--empty">
        <p className="claude-session-list__empty-text">No sessions yet</p>
        <button
          type="button"
          className="claude-session-list__new-btn"
          onClick={onNewSession}
        >
          New Session
        </button>
      </div>
    );
  }

  return (
    <div className="claude-session-list">
      <div className="claude-session-list__header">
        <span className="claude-session-list__title">Sessions</span>
        <div className="claude-session-list__actions">
          <button
            type="button"
            className="claude-session-list__refresh-btn"
            onClick={onRefresh}
            disabled={isLoading}
            title="Refresh sessions"
          >
            {isLoading ? "..." : "\u21BB"}
          </button>
          <button
            type="button"
            className="claude-session-list__new-btn"
            onClick={onNewSession}
          >
            + New
          </button>
        </div>
      </div>

      {error && (
        <div className="claude-session-list__error">
          {error}
          <button
            type="button"
            className="claude-session-list__retry-btn"
            onClick={onRefresh}
          >
            Retry
          </button>
        </div>
      )}

      <ul className="claude-session-list__items">
        {sessions.map((session) => (
          <li key={session.id}>
            <button
              type="button"
              className={`claude-session-list__item ${
                selectedSessionId === session.id
                  ? "claude-session-list__item--selected"
                  : ""
              }`}
              onClick={() => handleSelect(session.id)}
            >
              <span className="claude-session-list__item-title">
                {session.title || "Untitled Session"}
              </span>
              <span className="claude-session-list__item-time">
                {formatTime(session.time.updated)}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
