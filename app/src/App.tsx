import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

function App() {
  const [sessions, setSessions] = useState<string[]>([]);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);

  useEffect(() => {
    // Fetch sessions on mount
    invoke<string[]>("list_sessions")
      .then(setSessions)
      .catch(console.error);
  }, []);

  return (
    <div className="container">
      <aside className="sidebar">
        <div className="sidebar-header">
          <h1>Orchestrator</h1>
        </div>
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
                  onClick={() => setSelectedSession(session)}
                >
                  {session}
                </li>
              ))}
            </ul>
          )}
        </nav>
      </aside>
      <main className="main-panel">
        {selectedSession ? (
          <div className="session-view">
            <h2>{selectedSession}</h2>
            <div className="terminal-placeholder">
              Terminal will render here
            </div>
          </div>
        ) : (
          <div className="welcome">
            <h2>Welcome to Orchestrator</h2>
            <p>Select a session from the sidebar to view it.</p>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
