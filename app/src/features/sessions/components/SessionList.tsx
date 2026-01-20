type SessionListProps = {
  sessions: string[];
  selectedSession: string | null;
  isLoading: boolean;
  error: string | null;
  onSelectSession: (sessionId: string) => void;
};

export function SessionList({
  sessions,
  selectedSession,
  isLoading,
  error,
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

  return (
    <nav className="session-list">
      <h2>Sessions</h2>
      {sessions.length === 0 ? (
        <p className="empty">No active sessions</p>
      ) : (
        <ul>
          {sessions.map((session) => (
            <li
              key={session}
              className={selectedSession === session ? "selected" : ""}
              onClick={() => onSelectSession(session)}
            >
              {session}
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
