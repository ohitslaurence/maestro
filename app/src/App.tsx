import { useResizablePanels, ResizeHandle } from "./features/layout";
import { useSessions, SessionList } from "./features/sessions";
import { useTerminalSession, TerminalPanel } from "./features/terminal";

const DEFAULT_TERMINAL_ID = "main";

function App() {
  // Session management
  const {
    sessions,
    selectedSession,
    isLoading: sessionsLoading,
    error: sessionsError,
    selectSession,
  } = useSessions();

  // Layout state
  const { sidebarWidth, isResizing, onSidebarResizeStart } = useResizablePanels();

  // Terminal state
  const {
    status: terminalStatus,
    message: terminalMessage,
    containerRef: terminalContainerRef,
  } = useTerminalSession({
    sessionId: selectedSession,
    terminalId: selectedSession ? DEFAULT_TERMINAL_ID : null,
    isVisible: !!selectedSession,
  });

  return (
    <div className={`container ${isResizing ? "container--resizing" : ""}`}>
      <aside className="sidebar" style={{ width: sidebarWidth }}>
        <div className="sidebar-header">
          <h1>Orchestrator</h1>
        </div>
        <SessionList
          sessions={sessions}
          selectedSession={selectedSession}
          isLoading={sessionsLoading}
          error={sessionsError}
          onSelectSession={selectSession}
        />
      </aside>
      <ResizeHandle onMouseDown={onSidebarResizeStart} isResizing={isResizing} />
      <main className="main-panel">
        {selectedSession ? (
          <div className="session-view">
            <h2>{selectedSession}</h2>
            <TerminalPanel
              containerRef={terminalContainerRef}
              status={terminalStatus}
              message={terminalMessage}
            />
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
