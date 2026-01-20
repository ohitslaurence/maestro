import type { SessionInfo } from "../../../types";

type SessionListProps = {
  sessions: SessionInfo[];
  selectedSession: string | null;
  isLoading: boolean;
  error: string | null;
  disabled?: boolean;
  onSelectSession: (sessionPath: string) => void;
};

export function SessionList({
  sessions,
  selectedSession,
  isLoading,
  error,
  disabled = false,
  onSelectSession,
}: SessionListProps) {
  if (isLoading) {
    return (
      <nav className="session-list">
        <h2>Sessions</h2>
        <p className="empty">Loading...</p>
      </nav>
    );
  }

  if (error) {
    return (
      <nav className="session-list">
        <h2>Sessions</h2>
        <p className="empty error">Failed to load sessions</p>
      </nav>
    );
  }

  if (disabled) {
    return (
      <nav className="session-list session-list--disabled">
        <h2>Sessions</h2>
        <p className="empty">Connect to daemon to view sessions</p>
      </nav>
    );
  }

  return (
    <nav className="session-list">
      <h2>Sessions</h2>
      {sessions.length === 0 ? (
        <p className="empty">No active sessions</p>
      ) : (
        <ul>
          {sessions.map((session) => (
            <li
              key={session.path}
              className={selectedSession === session.path ? "selected" : ""}
              onClick={() => onSelectSession(session.path)}
              title={session.path}
            >
              {session.name}
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
