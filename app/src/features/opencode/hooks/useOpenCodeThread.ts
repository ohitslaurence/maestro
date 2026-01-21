import { useCallback, useEffect, useRef, useState } from "react";
import type { OpenCodeThreadItem, OpenCodeThreadStatus } from "../../../types";
import { subscribeOpenCodeEvents } from "../../../services/events";

// === Constants for normalization ===
const MAX_ITEMS_PER_THREAD = 500;
const MAX_ITEM_TEXT = 20000;
const TOOL_OUTPUT_RECENT_ITEMS = 40;

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
  lastDurationMs: number | null;
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
  // Ordering fields
  order: number; // Timestamp-based order for deterministic sorting
  arrivalIndex: number; // Stable index within arrival order
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

// === Streaming text merge ===
// Intelligently merge streaming deltas with existing text
// Handles cases where delta might be the full text, a suffix, or overlap
function mergeStreamingText(existing: string, delta: string): string {
  if (!delta) return existing;
  if (!existing) return delta;
  if (delta === existing) return existing;

  // Delta contains all of existing - use delta
  if (delta.startsWith(existing)) return delta;

  // Existing contains all of delta - keep existing
  if (existing.startsWith(delta)) return existing;

  // Try to find overlap at boundary
  const maxOverlap = Math.min(existing.length, delta.length);
  for (let length = maxOverlap; length > 0; length--) {
    if (existing.endsWith(delta.slice(0, length))) {
      return `${existing}${delta.slice(length)}`;
    }
  }

  // No overlap found - append
  return `${existing}${delta}`;
}

// === Text truncation ===
function truncateText(text: string, maxLength = MAX_ITEM_TEXT): string {
  if (text.length <= maxLength) return text;
  const sliceLength = Math.max(0, maxLength - 3);
  return `${text.slice(0, sliceLength)}...`;
}

// === Item normalization ===
function normalizeItem(item: OpenCodeThreadItem): OpenCodeThreadItem {
  switch (item.kind) {
    case "user-message":
    case "assistant-message":
      return { ...item, text: truncateText(item.text) };
    case "reasoning":
      return { ...item, text: truncateText(item.text) };
    case "tool":
      return {
        ...item,
        title: item.title ? truncateText(item.title, 200) : item.title,
        output: item.output ? truncateText(item.output) : item.output,
        error: item.error ? truncateText(item.error, 2000) : item.error,
      };
    default:
      return item;
  }
}

// === Thread item preparation (bounds + normalization) ===
function prepareThreadItems(items: OpenCodeThreadItem[]): OpenCodeThreadItem[] {
  // Normalize all items
  const normalized = items.map(normalizeItem);

  // Limit total items (keep most recent)
  const limited = normalized.length > MAX_ITEMS_PER_THREAD
    ? normalized.slice(-MAX_ITEMS_PER_THREAD)
    : normalized;

  // For older tool items, aggressively truncate output
  const cutoff = Math.max(0, limited.length - TOOL_OUTPUT_RECENT_ITEMS);
  return limited.map((item, index) => {
    if (index >= cutoff || item.kind !== "tool") return item;
    const output = item.output ? truncateText(item.output, 1000) : item.output;
    if (output === item.output) return item;
    return { ...item, output };
  });
}

// === Shallow compare for reconciliation ===
function itemsEqual(a: OpenCodeThreadItem, b: OpenCodeThreadItem): boolean {
  if (a.id !== b.id || a.kind !== b.kind) return false;

  switch (a.kind) {
    case "user-message":
    case "assistant-message":
      return a.text === (b as typeof a).text;
    case "reasoning":
      return a.text === (b as typeof a).text;
    case "tool":
      return a.status === (b as typeof a).status &&
             a.output === (b as typeof a).output &&
             a.error === (b as typeof a).error;
    case "patch":
      return a.hash === (b as typeof a).hash;
    case "step-finish":
      return a.cost === (b as typeof a).cost;
    default:
      return false;
  }
}

export function useOpenCodeThread({
  workspaceId,
  sessionId,
  pendingUserMessages = [],
}: UseOpenCodeThreadOptions): OpenCodeThreadState {
  const [items, setItems] = useState<OpenCodeThreadItem[]>([]);
  const [status, setStatus] = useState<OpenCodeThreadStatus>("idle");
  const [processingStartedAt, setProcessingStartedAt] = useState<number | null>(null);
  const [lastDurationMs, setLastDurationMs] = useState<number | null>(null);
  const [error, setError] = useState<string | undefined>();

  // Track messages by id
  const messagesRef = useRef<Map<string, TrackedMessage>>(new Map());

  // Track previous items for efficient updates
  const prevItemsRef = useRef<OpenCodeThreadItem[]>([]);

  // Global part arrival counter for stable ordering
  const partArrivalCounterRef = useRef(0);

  // Refs for values accessed in event handlers to avoid stale closures
  const processingStartedAtRef = useRef(processingStartedAt);
  const pendingUserMessagesRef = useRef(pendingUserMessages);
  useEffect(() => {
    processingStartedAtRef.current = processingStartedAt;
  }, [processingStartedAt]);
  useEffect(() => {
    pendingUserMessagesRef.current = pendingUserMessages;
  }, [pendingUserMessages]);

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

    // Assistant message - process parts in deterministic order
    const parts = Array.from(msg.parts.values());

    // Sort by order (timestamp-based) then by arrivalIndex for stability
    parts.sort((a, b) => {
      const orderDiff = a.order - b.order;
      if (orderDiff !== 0) return orderDiff;
      return a.arrivalIndex - b.arrivalIndex;
    });

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

    // Apply normalization and bounds
    const preparedItems = prepareThreadItems(newItems);

    // Preserve references for unchanged items to help React reconciliation
    const prevItems = prevItemsRef.current;
    const prevById = new Map(prevItems.map(item => [item.id, item]));

    const mergedItems = preparedItems.map(newItem => {
      const prevItem = prevById.get(newItem.id);
      // If prev exists and is equal, reuse the old reference
      if (prevItem && itemsEqual(prevItem, newItem)) {
        return prevItem;
      }
      return newItem;
    });

    prevItemsRef.current = mergedItems;
    setItems(mergedItems);

    // Determine processing status from message state
    const hasIncompleteAssistant = allMessages.some(
      (m) => m.role === "assistant" && !m.time?.completed
    );
    if (hasIncompleteAssistant) {
      // Don't override if we already have a processingStartedAt
      if (!processingStartedAtRef.current) {
        const assistantMsg = allMessages.find(
          (m) => m.role === "assistant" && !m.time?.completed
        );
        setProcessingStartedAt(assistantMsg?.time?.created ?? Date.now());
      }
      setStatus("processing");
    }
    // Note: don't set idle here - let session.status events handle that
  }, [convertMessageToItems, pendingUserMessages, sessionId]);

  // Track previous session to detect real session switches
  const prevSessionRef = useRef<{ workspaceId: string | null; sessionId: string | null }>({
    workspaceId: null,
    sessionId: null,
  });

  // Reset state only when switching between different sessions (not null → session)
  useEffect(() => {
    const prev = prevSessionRef.current;
    const workspaceChanged = workspaceId !== prev.workspaceId;
    const sessionSwitched = prev.sessionId !== null && sessionId !== null && prev.sessionId !== sessionId;

    // Update ref for next comparison
    prevSessionRef.current = { workspaceId, sessionId };

    // Only clear when workspace changes or switching between two different sessions
    if (workspaceChanged || sessionSwitched) {
      messagesRef.current.clear();
      prevItemsRef.current = [];
      partArrivalCounterRef.current = 0;
      setItems([]);
      setStatus("idle");
      setProcessingStartedAt(null);
      setLastDurationMs(null);
      setError(undefined);
    }
  }, [workspaceId, sessionId]);

  // Keep rebuildItems in a ref to avoid subscription churn
  const rebuildItemsRef = useRef(rebuildItems);
  useEffect(() => {
    rebuildItemsRef.current = rebuildItems;
  }, [rebuildItems]);

  // Rebuild when pending messages change and set immediate processing state
  useEffect(() => {
    if (pendingUserMessages.length > 0) {
      rebuildItemsRef.current();
      // Set processing immediately when user sends a message (before events arrive)
      setStatus("processing");
      if (!processingStartedAtRef.current) {
        setProcessingStartedAt(Date.now());
      }
    }
  }, [pendingUserMessages]);

  // Subscribe to OpenCode events - stable deps to avoid churn
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

      // Handle message.part.updated - streaming parts with delta accumulation
      if (eventType === "message.part.updated" && props?.part) {
        const rawPart = props.part as Omit<PartData, "order" | "arrivalIndex"> & { order?: number; arrivalIndex?: number };
        const delta = props.delta as string | undefined;

        // Filter by session (use current sessionId from closure, but accept if no session set yet)
        if (sessionId && rawPart.sessionID !== sessionId) {
          return;
        }

        // Get or create the message
        let msg = messagesRef.current.get(rawPart.messageID);
        if (!msg) {
          msg = {
            id: rawPart.messageID,
            sessionID: rawPart.sessionID,
            role: "assistant", // Parts are always from assistant
            parts: new Map(),
          };
          messagesRef.current.set(rawPart.messageID, msg);
        }

        // Get existing part or create new one
        const existingPart = msg.parts.get(rawPart.id);
        const arrivalIndex = existingPart?.arrivalIndex ?? partArrivalCounterRef.current++;
        const order = rawPart.time?.start ?? existingPart?.order ?? Date.now();

        // Build the updated part with streaming merge
        const updatedPart: PartData = {
          ...rawPart,
          order,
          arrivalIndex,
          // Merge streaming text if delta is present
          text: existingPart && delta
            ? mergeStreamingText(existingPart.text || "", delta)
            : rawPart.text ?? existingPart?.text,
          content: existingPart && delta && rawPart.type === "reasoning"
            ? mergeStreamingText(existingPart.content || "", delta)
            : rawPart.content ?? existingPart?.content,
          output: existingPart && delta && rawPart.type === "tool"
            ? mergeStreamingText(existingPart.output || "", delta)
            : rawPart.output ?? existingPart?.output,
        };

        msg.parts.set(rawPart.id, updatedPart);
        rebuildItemsRef.current();
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

        rebuildItemsRef.current();
        return;
      }

      // Handle session.error
      if (eventType === "session.error") {
        const errProps = props as { error?: string } | undefined;
        setError(errProps?.error || "Unknown error");
        setStatus("error");
        return;
      }

      // Handle session.status - provides immediate processing feedback
      if (eventType === "session.status") {
        const statusProps = props as { sessionID?: string; status?: { type?: string } } | undefined;
        // Filter by session if we have one
        if (sessionId && statusProps?.sessionID !== sessionId) {
          return;
        }
        const statusType = statusProps?.status?.type;
        if (statusType === "busy") {
          setStatus("processing");
          if (!processingStartedAtRef.current) {
            setProcessingStartedAt(Date.now());
          }
        } else if (statusType === "idle") {
          // Calculate duration before clearing
          const startedAt = processingStartedAtRef.current;
          if (startedAt) {
            setLastDurationMs(Date.now() - startedAt);
          }
          // Only set to idle if we don't have pending messages
          if (pendingUserMessagesRef.current.length === 0) {
            setStatus("idle");
            setProcessingStartedAt(null);
          }
        }
        return;
      }

      // Handle session.idle - alternative idle signal
      if (eventType === "session.idle") {
        const idleProps = props as { sessionID?: string } | undefined;
        if (sessionId && idleProps?.sessionID !== sessionId) {
          return;
        }
        // Calculate duration before clearing
        const startedAt = processingStartedAtRef.current;
        if (startedAt) {
          setLastDurationMs(Date.now() - startedAt);
        }
        if (pendingUserMessagesRef.current.length === 0) {
          setStatus("idle");
          setProcessingStartedAt(null);
        }
        return;
      }
      // Ignore other event types: server.heartbeat, session.created, session.updated, etc.
    });

    return unsubscribe;
  }, [workspaceId, sessionId]); // Removed rebuildItems from deps - using ref instead

  return {
    items,
    status,
    processingStartedAt,
    lastDurationMs,
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
