import { useCallback } from "react";
import { useOpenCodeThread } from "../hooks/useOpenCodeThread";
import { useOpenCodeSession } from "../hooks/useOpenCodeSession";
import { ThreadMessages } from "./ThreadMessages";
import { ThreadComposer } from "./ThreadComposer";

type ThreadViewProps = {
  workspaceId: string | null;
};

export function ThreadView({ workspaceId }: ThreadViewProps) {
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
    error,
  } = useOpenCodeThread({ workspaceId, sessionId });

  const handleSend = useCallback(
    async (message: string) => {
      // Create session if needed
      let activeSessionId = sessionId;
      if (!activeSessionId) {
        activeSessionId = await create();
        if (!activeSessionId) {
          console.error("[ThreadView] Failed to create session");
          return;
        }
      }
      // Wait for session to be set before prompting
      // Since create() updates sessionId via setSessionId,
      // we need to use the returned id directly
      if (activeSessionId) {
        try {
          // Use direct prompt call with the session id we have
          await prompt(message);
        } catch (err) {
          console.error("[ThreadView] Failed to send prompt", err);
        }
      }
    },
    [sessionId, create, prompt]
  );

  const handleStop = useCallback(() => {
    void abort();
  }, [abort]);

  const isProcessing = status === "processing" || isPrompting;
  const canStop = status === "processing";
  const disabled = !workspaceId;

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
