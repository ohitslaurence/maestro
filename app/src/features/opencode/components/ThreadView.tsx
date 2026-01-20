import { useCallback, useEffect } from "react";
import { useOpenCodeThread } from "../hooks/useOpenCodeThread";
import { useOpenCodeSession } from "../hooks/useOpenCodeSession";
import { useOpenCodeConnection } from "../hooks/useOpenCodeConnection";
import { ThreadMessages } from "./ThreadMessages";
import { ThreadComposer } from "./ThreadComposer";

type ThreadViewProps = {
  workspaceId: string | null;
};

export function ThreadView({ workspaceId }: ThreadViewProps) {
  const {
    isConnected,
    isConnecting,
    error: connectionError,
    connect,
  } = useOpenCodeConnection({
    workspaceId,
    workspacePath: workspaceId, // path is used as both id and path
  });

  const {
    sessionId,
    create,
    prompt,
    abort,
    isPrompting,
  } = useOpenCodeSession({ workspaceId });

  const {
    items,
    status,
    processingStartedAt,
    error: threadError,
  } = useOpenCodeThread({ workspaceId, sessionId });

  // Auto-connect when workspace changes
  useEffect(() => {
    if (workspaceId && !isConnected && !isConnecting && !connectionError) {
      void connect();
    }
  }, [workspaceId, isConnected, isConnecting, connectionError, connect]);

  const handleSend = useCallback(
    async (message: string) => {
      if (!isConnected) {
        console.warn("[ThreadView] Cannot send: not connected");
        return;
      }

      // Create session if needed
      let activeSessionId = sessionId;
      if (!activeSessionId) {
        activeSessionId = await create();
        if (!activeSessionId) {
          console.error("[ThreadView] Failed to create session");
          return;
        }
      }

      if (activeSessionId) {
        try {
          await prompt(message);
        } catch (err) {
          console.error("[ThreadView] Failed to send prompt", err);
        }
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
          Connecting to OpenCode...
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
