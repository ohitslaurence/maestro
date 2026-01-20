import { useCallback, useEffect, useRef, useState } from "react";
import { useResizablePanels, ResizeHandle, TabBar } from "./features/layout";
import type { Tab } from "./features/layout";
import { useSessions, SessionList } from "./features/sessions";
import { useTerminalSession, TerminalPanel } from "./features/terminal";
import { GitPanel } from "./features/git";
import {
  useDaemonConnection,
  useDaemonProfiles,
  ConnectionMenu,
  SettingsModal,
} from "./features/daemon";
import { ThreadView } from "./features/opencode";

const DEFAULT_TERMINAL_ID = "main";

type MainPanelTab = "terminal" | "git" | "agent";

const TABS: Tab[] = [
  { id: "terminal", label: "Terminal" },
  { id: "git", label: "Git" },
  { id: "agent", label: "Agent" },
];

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
  const [activeTab, setActiveTab] = useState<MainPanelTab>("terminal");
  const isConnected = connectionStatus === "connected";
  const autoRestoreRef = useRef(false);

  const {
    profiles,
    rememberLastUsed,
    lastUsedProfile,
    upsertProfile,
    removeProfile,
    markLastUsed,
    setRememberLastUsed,
  } = useDaemonProfiles();

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

  // Terminal state - keep terminal alive even when on git tab
  const {
    status: terminalStatus,
    message: terminalMessage,
    containerRef: terminalContainerRef,
  } = useTerminalSession({
    sessionId: selectedSession,
    terminalId: selectedSession ? DEFAULT_TERMINAL_ID : null,
    isVisible: !!selectedSession && isConnected && activeTab === "terminal",
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

  const handleConnectProfile = useCallback(
    async (profile: {
      name?: string;
      host: string;
      port: number;
      token: string;
    }) => {
      await configure(profile.host, profile.port, profile.token);
      await connect();
      const saved = upsertProfile(profile);
      markLastUsed(saved.id);
    },
    [configure, connect, upsertProfile, markLastUsed],
  );

  const handleSaveProfile = useCallback(
    (input: {
      id?: string | null;
      name?: string;
      host: string;
      port: number;
      token: string;
    }) => {
      const saved = upsertProfile(input, input.id ?? null);
      markLastUsed(saved.id);
      return saved;
    },
    [upsertProfile, markLastUsed],
  );

  const handleCloseSettings = useCallback(() => {
    setShowSettings(false);
  }, []);

  const handleTabChange = useCallback((tabId: string) => {
    setActiveTab(tabId as MainPanelTab);
  }, []);

  useEffect(() => {
    if (autoRestoreRef.current) {
      return;
    }

    if (!rememberLastUsed || !lastUsedProfile) {
      autoRestoreRef.current = true;
      return;
    }

    if (connectionStatus !== "disconnected") {
      autoRestoreRef.current = true;
      return;
    }

    if (host || port) {
      autoRestoreRef.current = true;
      return;
    }

    autoRestoreRef.current = true;
    void handleConnectProfile(lastUsedProfile);
  }, [
    rememberLastUsed,
    lastUsedProfile,
    connectionStatus,
    host,
    port,
    handleConnectProfile,
  ]);

  return (
    <div className={`container ${isResizing ? "container--resizing" : ""}`}>
      <aside className="sidebar" style={{ width: sidebarWidth }}>
        <div className="sidebar-header">
          <h1>Maestro</h1>
        </div>
        <SessionList
          sessions={sessions}
          selectedSession={selectedSession}
          isLoading={sessionsLoading}
          error={sessionsError}
          disabled={!isConnected}
          onSelectSession={selectSession}
        />
        <div className="sidebar-footer">
          <ConnectionMenu
            status={connectionStatus}
            host={host}
            port={port}
            profiles={profiles}
            onConnectProfile={handleConnectProfile}
            onDisconnect={disconnect}
            onManage={handleConnectionClick}
          />
        </div>
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
            <div className="session-header">
              <h2>{selectedSessionInfo.name}</h2>
              <TabBar
                tabs={TABS}
                activeTab={activeTab}
                onTabChange={handleTabChange}
              />
            </div>
            <div className="session-content">
              {activeTab === "terminal" && (
                <TerminalPanel
                  containerRef={terminalContainerRef}
                  status={terminalStatus}
                  message={terminalMessage}
                />
              )}
              {activeTab === "git" && (
                <GitPanel sessionId={selectedSession} />
              )}
              {activeTab === "agent" && (
                <ThreadView workspaceId={selectedSession} />
              )}
            </div>
          </div>
        ) : (
          <div className="welcome">
            <h2>Welcome to Maestro</h2>
            <p>Select a session from the sidebar to view it.</p>
          </div>
        )}
      </main>

      <SettingsModal
        key={showSettings ? "settings-open" : "settings-closed"}
        isOpen={showSettings}
        onClose={handleCloseSettings}
        status={connectionStatus}
        currentHost={host}
        currentPort={port}
        error={connectionError}
        onConfigure={configure}
        onConnect={connect}
        onDisconnect={disconnect}
        profiles={profiles}
        rememberLastUsed={rememberLastUsed}
        onRememberLastUsedChange={setRememberLastUsed}
        onConnectProfile={handleConnectProfile}
        onRemoveProfile={removeProfile}
        onSaveProfile={handleSaveProfile}
      />
    </div>
  );
}

export default App;
