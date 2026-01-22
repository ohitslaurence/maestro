import { useCallback, useEffect, useRef, useState } from "react";
import { useOpenCodeThread } from "../../opencode/hooks/useOpenCodeThread";
import { useClaudeSession } from "../hooks/useClaudeSession";
import { useComposerOptions } from "../hooks/useComposerOptions";
import { usePermissions } from "../hooks/usePermissions";
import { useSessionSettings } from "../hooks/useSessionSettings";
import { useAgentSession, isAgentWorking } from "../../../hooks/useAgentSession";
import { ThreadMessages } from "../../opencode/components/ThreadMessages";
import { ThreadComposer } from "../../opencode/components/ThreadComposer";
import { ComposerOptions } from "./ComposerOptions";
import { PermissionModal } from "./PermissionModal";
import { SessionSettingsButton } from "./SessionSettingsButton";
import { SessionSettingsModal } from "./SessionSettingsModal";

type PendingUserMessage = {
  id: string;
  text: string;
  timestamp: number;
};

type ClaudeThreadViewProps = {
  workspaceId: string | null;
};

/**
 * Claude SDK version of ThreadView.
 *
 * Uses useClaudeSession for connection/session management and
 * useOpenCodeThread for event processing (Claude SDK server emits
 * OpenCode-compatible events per spec).
 */
export function ClaudeThreadView({ workspaceId }: ClaudeThreadViewProps) {
  const [pendingUserMessages, setPendingUserMessages] = useState<PendingUserMessage[]>([]);

  const {
    isConnected,
    isConnecting,
    connectionError,
    connect,
    sessionId,
    create,
    prompt,
    abort,
    isPrompting,
  } = useClaudeSession({
    workspaceId,
    workspacePath: workspaceId, // path is used as both id and path
    autoConnect: true,
  });

  const {
    items,
    processingStartedAt,
    lastDurationMs,
    error: threadError,
  } = useOpenCodeThread({ workspaceId, sessionId, pendingUserMessages });

  // Composer options: model selection and thinking mode (composer-options spec §2, §5)
  const {
    models,
    selectedModel,
    setSelectedModel,
    thinkingMode,
    setThinkingMode,
    maxThinkingTokens,
    disabled: composerOptionsDisabled,
  } = useComposerOptions({ workspaceId, isConnected });

  // Permission requests: queue-based handling (dynamic-tool-approvals spec §5, §UI Components)
  const {
    currentRequest: permissionRequest,
    reply: replyToPermission,
    dismiss: dismissPermission,
  } = usePermissions({ workspaceId, sessionId });

  // Session settings (session-settings spec §5, §6)
  const {
    settings: sessionSettings,
    isUpdating: isSettingsUpdating,
    error: settingsError,
    updateSettings,
  } = useSessionSettings({ workspaceId, sessionId });
  const [isSettingsModalOpen, setIsSettingsModalOpen] = useState(false);

  // Use agent state machine for working/idle status (per state-machine-wiring.md §4, §5)
  const { state: agentState } = useAgentSession({ sessionId: sessionId ?? undefined });
  const isWorking = isAgentWorking(agentState.kind);
  const hasAgentError = agentState.kind === "error";

  // Derive status from agent state machine, falling back to thread status for error details
  const status = hasAgentError ? "error" : isWorking ? "processing" : "idle";

  const handleSend = useCallback(
    async (message: string) => {
      if (!isConnected) {
        console.warn("[ClaudeThreadView] Cannot send: not connected");
        return;
      }

      // Create session if needed
      let activeSessionId = sessionId;
      if (!activeSessionId) {
        activeSessionId = await create();
        if (!activeSessionId) {
          console.error("[ClaudeThreadView] Failed to create session");
          return;
        }
      }

      // Add pending user message for immediate UI feedback
      const pendingMsg: PendingUserMessage = {
        id: `pending-${Date.now()}`,
        text: message,
        timestamp: Date.now(),
      };
      setPendingUserMessages(prev => [...prev, pendingMsg]);

      try {
        // Pass activeSessionId and maxThinkingTokens (composer-options spec §5)
        await prompt(message, { sessionId: activeSessionId, maxThinkingTokens });
      } catch (err) {
        console.error("[ClaudeThreadView] Failed to send prompt", err);
        // Remove pending message on error
        setPendingUserMessages(prev => prev.filter(m => m.id !== pendingMsg.id));
      }
    },
    [isConnected, sessionId, create, prompt, maxThinkingTokens]
  );

  const handleStop = useCallback(() => {
    // Optimistic UI - clear pending messages immediately
    setPendingUserMessages([]);
    void abort();
  }, [abort]);

  const handleConnect = useCallback(() => {
    void connect();
  }, [connect]);

  const handleOpenSettings = useCallback(() => {
    setIsSettingsModalOpen(true);
  }, []);

  const handleCloseSettings = useCallback(() => {
    setIsSettingsModalOpen(false);
  }, []);

  // Track previous status to detect transitions
  const prevStatusRef = useRef(status);

  // Clear pending messages when turn completes (status: processing -> idle)
  useEffect(() => {
    const wasProcessing = prevStatusRef.current === "processing";
    const nowIdle = status === "idle";
    prevStatusRef.current = status;

    // If we transitioned from processing to idle, clear pending messages
    if (wasProcessing && nowIdle && pendingUserMessages.length > 0) {
      setPendingUserMessages([]);
    }
  }, [status, pendingUserMessages.length]);

  const isProcessing = status === "processing" || isPrompting;
  const canStop = status === "processing";
  // Disable composer while permission pending (dynamic-tool-approvals spec §5)
  const isPermissionPending = permissionRequest !== null;
  const disabled = !workspaceId || !isConnected || isPermissionPending;
  const error = connectionError || threadError;

  // Show connecting state
  if (workspaceId && isConnecting) {
    return (
      <div className="oc-thread">
        <div className="oc-thread__status">
          <span className="oc-thread__spinner" />
          Connecting to Claude SDK...
        </div>
      </div>
    );
  }

  // Show connect button if not connected
  if (workspaceId && !isConnected && connectionError) {
    return (
      <div className="oc-thread">
        <div className="oc-thread__status oc-thread__status--error">
          <p>{connectionError}</p>
          <button
            type="button"
            className="oc-thread__connect-btn"
            onClick={handleConnect}
          >
            Retry Connection
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="oc-thread">
      {/* Session header with settings button (session-settings spec §Appendix) */}
      <div className="oc-thread__header">
        <SessionSettingsButton
          onClick={handleOpenSettings}
          disabled={!isConnected || !sessionId}
        />
      </div>
      {error && (
        <div className="oc-thread__error">
          {error}
        </div>
      )}
      <ThreadMessages
        items={items}
        status={status}
        processingStartedAt={processingStartedAt}
        lastDurationMs={lastDurationMs}
      />
      <ComposerOptions
        models={models}
        selectedModel={selectedModel}
        onModelSelect={setSelectedModel}
        thinkingMode={thinkingMode}
        onThinkingModeSelect={setThinkingMode}
        disabled={composerOptionsDisabled || isProcessing}
      />
      {/* Visual indicator when awaiting tool approval (dynamic-tool-approvals spec §5, Phase 9) */}
      {isPermissionPending && (
        <div className="oc-thread__permission-pending">
          <span className="oc-thread__permission-pending-icon">⏳</span>
          <span className="oc-thread__permission-pending-text">Awaiting tool approval...</span>
        </div>
      )}
      <ThreadComposer
        onSend={handleSend}
        onStop={handleStop}
        canStop={canStop}
        disabled={disabled}
        isProcessing={isProcessing}
      />
      {/* Permission modal for tool approvals (dynamic-tool-approvals spec §UI Components) */}
      <PermissionModal
        request={permissionRequest}
        onReply={replyToPermission}
        onClose={dismissPermission}
      />
      {/* Session settings modal (session-settings spec §Appendix) */}
      <SessionSettingsModal
        isOpen={isSettingsModalOpen}
        settings={sessionSettings}
        isUpdating={isSettingsUpdating}
        error={settingsError}
        onSave={updateSettings}
        onClose={handleCloseSettings}
      />
    </div>
  );
}
