import { useCallback, useEffect, useState } from "react";
import { useResizablePanels, ResizeHandle } from "./features/layout";
import { useSessions, SessionList } from "./features/sessions";
import { useTerminalSession, TerminalPanel } from "./features/terminal";
import {
  useDaemonConnection,
  ConnectionStatus,
  SettingsModal,
} from "./features/daemon";

const DEFAULT_TERMINAL_ID = "main";

function App() {
  // Daemon connection state
  const {
    status: connectionStatus,
    host,
    port,
    error: connectionError,
    connect,
    disconnect,
    configure,
  } = useDaemonConnection();

  const [showSettings, setShowSettings] = useState(false);
  const isConnected = connectionStatus === "connected";

  // Session management - only fetch when connected
  const {
    sessions,
    selectedSession,
    isLoading: sessionsLoading,
    error: sessionsError,
    selectSession,
    refresh: refreshSessions,
  } = useSessions({ enabled: isConnected, pollInterval: 5000 });

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
    isVisible: !!selectedSession && isConnected,
  });

  // Find selected session info for display
  const selectedSessionInfo = sessions.find((s) => s.path === selectedSession);

  // Refresh sessions when connection state changes to connected
  useEffect(() => {
    if (isConnected) {
      void refreshSessions();
    }
  }, [isConnected, refreshSessions]);

  // Show settings modal on first run if not configured
  useEffect(() => {
    if (connectionStatus === "disconnected" && !host && !port) {
      setShowSettings(true);
    }
  }, [connectionStatus, host, port]);

  const handleConnectionClick = useCallback(() => {
    setShowSettings(true);
  }, []);

  const handleCloseSettings = useCallback(() => {
    setShowSettings(false);
  }, []);

  return (
    <div className={`container ${isResizing ? "container--resizing" : ""}`}>
      <aside className="sidebar" style={{ width: sidebarWidth }}>
        <div className="sidebar-header">
          <h1>Orchestrator</h1>
          <ConnectionStatus
            status={connectionStatus}
            host={host}
            port={port}
            onClick={handleConnectionClick}
          />
        </div>
        <SessionList
          sessions={sessions}
          selectedSession={selectedSession}
          isLoading={sessionsLoading}
          error={sessionsError}
          disabled={!isConnected}
          onSelectSession={selectSession}
        />
      </aside>
      <ResizeHandle onMouseDown={onSidebarResizeStart} isResizing={isResizing} />
      <main className="main-panel">
        {!isConnected ? (
          <div className="welcome">
            <h2>Connect to Daemon</h2>
            <p>Configure your daemon connection to get started.</p>
            <button
              type="button"
              className="btn btn--primary"
              onClick={handleConnectionClick}
            >
              Configure Connection
            </button>
          </div>
        ) : selectedSession && selectedSessionInfo ? (
          <div className="session-view">
            <h2>{selectedSessionInfo.name}</h2>
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

      <SettingsModal
        isOpen={showSettings}
        onClose={handleCloseSettings}
        status={connectionStatus}
        currentHost={host}
        currentPort={port}
        error={connectionError}
        onConfigure={configure}
        onConnect={connect}
        onDisconnect={disconnect}
      />
    </div>
  );
}

export default App;
