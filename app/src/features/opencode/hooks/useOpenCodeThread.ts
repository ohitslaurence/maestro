import { useCallback, useEffect, useRef, useState } from "react";
import type { OpenCodeThreadItem, OpenCodeThreadStatus } from "../../../types";
import { subscribeOpenCodeEvents } from "../../../services/events";

type PendingUserMessage = {
  id: string;
  text: string;
  timestamp: number;
};

type UseOpenCodeThreadOptions = {
  workspaceId: string | null;
  sessionId: string | null;
  pendingUserMessages?: PendingUserMessage[];
};

export type OpenCodeThreadState = {
  items: OpenCodeThreadItem[];
  status: OpenCodeThreadStatus;
  processingStartedAt: number | null;
  error?: string;
};

// Part data from message.part.updated events
type PartData = {
  id: string;
  messageID: string;
  sessionID: string;
  type: string;
  text?: string;
  content?: string;
  tool?: string;
  callID?: string;
  title?: string;
  input?: Record<string, unknown>;
  output?: string;
  error?: string;
  hash?: string;
  files?: string[];
  cost?: number;
  tokens?: { input: number; output: number; reasoning: number; cache?: { read: number; write: number } };
  time?: { start?: number; end?: number };
};

// Message metadata from message.updated events
type MessageInfo = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  time?: { created?: number; completed?: number };
  summary?: { title?: string };
};

// Internal tracked message
type TrackedMessage = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  time?: { created?: number; completed?: number };
  userText?: string; // For user messages
  parts: Map<string, PartData>; // For assistant messages
};

export function useOpenCodeThread({
  workspaceId,
  sessionId,
  pendingUserMessages = [],
}: UseOpenCodeThreadOptions): OpenCodeThreadState {
  const [items, setItems] = useState<OpenCodeThreadItem[]>([]);
  const [status, setStatus] = useState<OpenCodeThreadStatus>("idle");
  const [processingStartedAt, setProcessingStartedAt] = useState<number | null>(null);
  const [error, setError] = useState<string | undefined>();

  // Track messages by id
  const messagesRef = useRef<Map<string, TrackedMessage>>(new Map());

  // Convert tracked message to thread items
  const convertMessageToItems = useCallback((msg: TrackedMessage): OpenCodeThreadItem[] => {
    const result: OpenCodeThreadItem[] = [];

    // User message
    if (msg.role === "user" && msg.userText) {
      result.push({
        id: msg.id,
        kind: "user-message",
        text: msg.userText,
      });
      return result;
    }

    // Assistant message - process parts
    const parts = Array.from(msg.parts.values());
    for (const part of parts) {
      switch (part.type) {
        case "text":
          if (part.text?.trim()) {
            result.push({
              id: part.id,
              kind: "assistant-message",
              text: part.text,
            });
          }
          break;

        case "reasoning":
          if (part.content || part.text) {
            result.push({
              id: part.id,
              kind: "reasoning",
              text: part.content || part.text || "",
              time: part.time as { start: number; end?: number } | undefined,
            });
          }
          break;

        case "tool": {
          const toolStatus = deriveToolStatus(part);
          result.push({
            id: part.id,
            kind: "tool",
            tool: part.tool || "unknown",
            callId: part.callID || part.id,
            status: toolStatus,
            title: part.title,
            input: part.input || {},
            output: part.output,
            error: part.error,
          });
          break;
        }

        case "patch":
          result.push({
            id: part.id,
            kind: "patch",
            hash: part.hash || "",
            files: part.files || [],
          });
          break;

        case "step-finish":
          if (part.cost !== undefined || part.tokens) {
            result.push({
              id: part.id,
              kind: "step-finish",
              cost: part.cost || 0,
              tokens: part.tokens || { input: 0, output: 0, reasoning: 0 },
            });
          }
          break;

        // Skip non-display parts
        case "step-start":
        case "file":
        case "agent":
        case "subtask":
        case "retry":
        case "compaction":
        case "snapshot":
          break;
      }
    }

    return result;
  }, []);

  // Rebuild items from all messages
  const rebuildItems = useCallback(() => {
    const messages = Array.from(messagesRef.current.values());

    // Convert pending user messages to tracked messages (if not already in messages)
    const pendingAsTracked: TrackedMessage[] = pendingUserMessages
      .filter(p => !messages.some(m => m.id === p.id))
      .map(p => ({
        id: p.id,
        sessionID: sessionId || "",
        role: "user" as const,
        time: { created: p.timestamp },
        userText: p.text,
        parts: new Map(),
      }));

    const allMessages = [...messages, ...pendingAsTracked];

    // Sort by creation time
    allMessages.sort((a, b) => {
      const aTime = a.time?.created ?? 0;
      const bTime = b.time?.created ?? 0;
      return aTime - bTime;
    });

    const newItems: OpenCodeThreadItem[] = [];
    for (const msg of allMessages) {
      newItems.push(...convertMessageToItems(msg));
    }
    setItems(newItems);

    // Determine processing status
    const hasIncompleteAssistant = allMessages.some(
      (m) => m.role === "assistant" && !m.time?.completed
    );
    if (hasIncompleteAssistant) {
      setStatus("processing");
      const assistantMsg = allMessages.find(
        (m) => m.role === "assistant" && !m.time?.completed
      );
      setProcessingStartedAt(assistantMsg?.time?.created ?? Date.now());
    } else {
      setStatus("idle");
      setProcessingStartedAt(null);
    }
  }, [convertMessageToItems, pendingUserMessages, sessionId]);

  // Reset state when session changes
  useEffect(() => {
    messagesRef.current.clear();
    setItems([]);
    setStatus("idle");
    setProcessingStartedAt(null);
    setError(undefined);
  }, [workspaceId, sessionId]);

  // Rebuild when pending messages change
  useEffect(() => {
    if (pendingUserMessages.length > 0) {
      rebuildItems();
    }
  }, [pendingUserMessages, rebuildItems]);

  // Subscribe to OpenCode events
  useEffect(() => {
    if (!workspaceId) {
      return;
    }

    const unsubscribe = subscribeOpenCodeEvents((event) => {
      // Filter by workspace
      if (event.workspaceId !== workspaceId) {
        return;
      }

      // Extract the actual event type from the nested event object
      const innerEvent = event.event as { type: string; properties?: Record<string, unknown> };
      const eventType = innerEvent?.type;
      const props = innerEvent?.properties;

      // Handle message.part.updated - streaming parts
      if (eventType === "message.part.updated" && props?.part) {
        const part = props.part as PartData;

        // Filter by session
        if (sessionId && part.sessionID !== sessionId) {
          return;
        }

        // Get or create the message
        let msg = messagesRef.current.get(part.messageID);
        if (!msg) {
          msg = {
            id: part.messageID,
            sessionID: part.sessionID,
            role: "assistant", // Parts are always from assistant
            parts: new Map(),
          };
          messagesRef.current.set(part.messageID, msg);
        }

        // Update the part
        msg.parts.set(part.id, part);
        rebuildItems();
        return;
      }

      // Handle message.updated - message metadata
      if (eventType === "message.updated" && props?.info) {
        const info = props.info as MessageInfo;

        // Filter by session
        if (sessionId && info.sessionID !== sessionId) {
          return;
        }

        // Get or create the message
        let msg = messagesRef.current.get(info.id);
        if (!msg) {
          msg = {
            id: info.id,
            sessionID: info.sessionID,
            role: info.role,
            parts: new Map(),
          };
          messagesRef.current.set(info.id, msg);
        }

        // Update metadata
        msg.role = info.role;
        msg.time = info.time;

        // For user messages, extract text from summary
        if (info.role === "user" && info.summary?.title) {
          msg.userText = info.summary.title;
        }

        rebuildItems();
        return;
      }

      // Handle session.error
      if (eventType === "session.error") {
        const errProps = props as { error?: string } | undefined;
        setError(errProps?.error || "Unknown error");
        setStatus("error");
      }
      // Ignore other event types: server.heartbeat, session.created, session.updated, etc.
    });

    return unsubscribe;
  }, [workspaceId, sessionId, rebuildItems]);

  return {
    items,
    status,
    processingStartedAt,
    error,
  };
}

function deriveToolStatus(part: PartData): OpenCodeThreadItem & { kind: "tool" } extends { status: infer S } ? S : never {
  if (part.error) return "error";
  if (part.output !== undefined) return "completed";
  if (part.time?.end) return "completed";
  if (part.time?.start) return "running";
  return "pending";
}
