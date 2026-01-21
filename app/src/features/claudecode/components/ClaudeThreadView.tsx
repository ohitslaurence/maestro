import { useCallback, useEffect, useRef, useState } from "react";
import { useOpenCodeThread } from "../../opencode/hooks/useOpenCodeThread";
import { useClaudeSession } from "../hooks/useClaudeSession";
import { ThreadMessages } from "../../opencode/components/ThreadMessages";
import { ThreadComposer } from "../../opencode/components/ThreadComposer";

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
    status,
    processingStartedAt,
    lastDurationMs,
    error: threadError,
  } = useOpenCodeThread({ workspaceId, sessionId, pendingUserMessages });

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
        // Pass activeSessionId explicitly in case state hasn't updated yet
        await prompt(message, activeSessionId);
      } catch (err) {
        console.error("[ClaudeThreadView] Failed to send prompt", err);
        // Remove pending message on error
        setPendingUserMessages(prev => prev.filter(m => m.id !== pendingMsg.id));
      }
    },
    [isConnected, sessionId, create, prompt]
  );

  const handleStop = useCallback(() => {
    void abort();
  }, [abort]);

  const handleConnect = useCallback(() => {
    void connect();
  }, [connect]);

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
  const disabled = !workspaceId || !isConnected;
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
      <ThreadComposer
        onSend={handleSend}
        onStop={handleStop}
        canStop={canStop}
        disabled={disabled}
        isProcessing={isProcessing}
      />
    </div>
  );
}
