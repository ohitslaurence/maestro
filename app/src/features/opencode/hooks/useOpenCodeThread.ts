import { useCallback, useEffect, useRef, useState } from "react";
import type { OpenCodeThreadItem, OpenCodeThreadStatus } from "../../../types";
import { subscribeOpenCodeEvents } from "../../../services/events";

type UseOpenCodeThreadOptions = {
  workspaceId: string | null;
  sessionId: string | null;
};

export type OpenCodeThreadState = {
  items: OpenCodeThreadItem[];
  status: OpenCodeThreadStatus;
  processingStartedAt: number | null;
  error?: string;
};

// Raw part types from OpenCode message-v2 events
type RawPart = {
  type: string;
  [key: string]: unknown;
};

type RawMessage = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  time?: { created?: number; completed?: number };
  parts?: RawPart[];
  content?: string;
};

function generateId(): string {
  return Math.random().toString(36).slice(2, 11);
}

export function useOpenCodeThread({
  workspaceId,
  sessionId,
}: UseOpenCodeThreadOptions): OpenCodeThreadState {
  const [items, setItems] = useState<OpenCodeThreadItem[]>([]);
  const [status, setStatus] = useState<OpenCodeThreadStatus>("idle");
  const [processingStartedAt, setProcessingStartedAt] = useState<number | null>(null);
  const [error, setError] = useState<string | undefined>();

  // Track messages by id for updates
  const messagesRef = useRef<Map<string, RawMessage>>(new Map());

  // Convert raw message to thread items
  const convertMessageToItems = useCallback((msg: RawMessage): OpenCodeThreadItem[] => {
    const result: OpenCodeThreadItem[] = [];

    // If it's a user message with content
    if (msg.role === "user" && msg.content) {
      result.push({
        id: msg.id,
        kind: "user-message",
        text: msg.content,
      });
      return result;
    }

    // Process parts for assistant messages
    if (msg.parts && Array.isArray(msg.parts)) {
      for (const part of msg.parts) {
        const partId = `${msg.id}-${generateId()}`;

        switch (part.type) {
          case "text":
            if (typeof part.content === "string" && part.content.trim()) {
              result.push({
                id: partId,
                kind: "assistant-message",
                text: part.content,
              });
            }
            break;

          case "reasoning":
            if (typeof part.content === "string") {
              result.push({
                id: partId,
                kind: "reasoning",
                text: part.content,
                time: part.time as { start: number; end?: number } | undefined,
              });
            }
            break;

          case "tool": {
            const toolStatus = deriveToolStatus(part);
            result.push({
              id: partId,
              kind: "tool",
              tool: (part.tool as string) || "unknown",
              callId: (part.callID as string) || partId,
              status: toolStatus,
              title: part.title as string | undefined,
              input: (part.input as Record<string, unknown>) || {},
              output: part.output as string | undefined,
              error: part.error as string | undefined,
            });
            break;
          }

          case "patch":
            result.push({
              id: partId,
              kind: "patch",
              hash: (part.hash as string) || "",
              files: (part.files as string[]) || [],
            });
            break;

          case "step-finish":
            if (part.cost !== undefined || part.tokens) {
              result.push({
                id: partId,
                kind: "step-finish",
                cost: (part.cost as number) || 0,
                tokens: (part.tokens as { input: number; output: number; reasoning: number }) || {
                  input: 0,
                  output: 0,
                  reasoning: 0,
                },
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

          default:
            // Unknown part type, skip
            break;
        }
      }
    }

    return result;
  }, []);

  // Rebuild items from all messages
  const rebuildItems = useCallback(() => {
    const messages = Array.from(messagesRef.current.values());
    // Sort by creation time
    messages.sort((a, b) => {
      const aTime = a.time?.created ?? 0;
      const bTime = b.time?.created ?? 0;
      return aTime - bTime;
    });

    const newItems: OpenCodeThreadItem[] = [];
    for (const msg of messages) {
      newItems.push(...convertMessageToItems(msg));
    }
    setItems(newItems);

    // Determine processing status
    const hasIncompleteAssistant = messages.some(
      (m) => m.role === "assistant" && !m.time?.completed
    );
    if (hasIncompleteAssistant) {
      setStatus("processing");
      const assistantMsg = messages.find(
        (m) => m.role === "assistant" && !m.time?.completed
      );
      setProcessingStartedAt(assistantMsg?.time?.created ?? Date.now());
    } else {
      setStatus("idle");
      setProcessingStartedAt(null);
    }
  }, [convertMessageToItems]);

  // Reset state when session changes
  useEffect(() => {
    messagesRef.current.clear();
    setItems([]);
    setStatus("idle");
    setProcessingStartedAt(null);
    setError(undefined);
  }, [workspaceId, sessionId]);

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
      const innerEvent = event.event as { type: string; properties?: unknown };
      const eventType = innerEvent?.type;

      console.log("[opencode-thread] Event type:", eventType);

      // Handle message events (actual conversation messages)
      if (eventType === "message" || eventType === "message.updated") {
        const msg = innerEvent.properties as RawMessage;
        console.log("[opencode-thread] Message:", msg?.id, msg?.role);

        if (!msg?.id) {
          return;
        }

        // Filter by session if specified
        if (sessionId && msg.sessionID !== sessionId) {
          return;
        }

        // Update or add message
        messagesRef.current.set(msg.id, msg);
        rebuildItems();
      } else if (eventType === "session.error") {
        const props = innerEvent.properties as { error?: string };
        setError(props?.error || "Unknown error");
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

function deriveToolStatus(part: RawPart): OpenCodeThreadItem & { kind: "tool" } extends { status: infer S } ? S : never {
  if (part.error) return "error";
  if (part.output !== undefined) return "completed";
  if (part.time && typeof part.time === "object" && "start" in part.time) {
    const timeObj = part.time as { start?: number; end?: number };
    if (timeObj.end) return "completed";
    return "running";
  }
  return "pending";
}
