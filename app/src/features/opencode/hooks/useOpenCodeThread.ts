import { useCallback, useEffect, useRef, useState } from "react";
import type { OpenCodeThreadItem } from "../../../types";
import { subscribeStreamEvents } from "../../../services/events";
import { opencodeSessionMessages } from "../../../services/tauri";
import {
  type StreamEvent,
  type StreamBuffer,
  createStreamBuffer,
  isTextDelta,
  isToolCallDelta,
  isToolCallCompleted,
  isCompleted,
  isError,
  isThinkingDelta,
} from "../../../types/streaming";

// === Constants for normalization ===
const MAX_ITEMS_PER_THREAD = 500;
const MAX_ITEM_TEXT = 20000;
const TOOL_OUTPUT_RECENT_ITEMS = 40;

// === Stream buffering constants (§5) ===
const STREAM_GAP_TIMEOUT_MS = 5_000;

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

/**
 * Thread state returned by useOpenCodeThread.
 *
 * Per state-machine-wiring.md §2, §5: This hook focuses on message/thread payloads.
 * Working/idle status is derived from useAgentSession in the UI layer.
 */
export type OpenCodeThreadState = {
  items: OpenCodeThreadItem[];
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

// Internal tracked message
type TrackedMessage = {
  id: string;
  sessionID: string;
  role: "user" | "assistant";
  time?: { created?: number; completed?: number };
  userText?: string; // For user messages
  parts: Map<string, PartData>; // For assistant messages
};

// API response types for history loading
type ApiPartResponse = {
  id: string;
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

type ApiMessageResponse = {
  id: string;
  sessionID?: string;
  role: "user" | "assistant";
  time?: { created?: number; completed?: number };
  summary?: { title?: string };
  parts?: ApiPartResponse[];
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
  const [processingStartedAt, setProcessingStartedAt] = useState<number | null>(null);
  const [lastDurationMs, setLastDurationMs] = useState<number | null>(null);
  const [error, setError] = useState<string | undefined>();

  // Track messages by id
  const messagesRef = useRef<Map<string, TrackedMessage>>(new Map());

  // Track previous items for efficient updates
  const prevItemsRef = useRef<OpenCodeThreadItem[]>([]);

  // Global part arrival counter for stable ordering
  const partArrivalCounterRef = useRef(0);

  // Stream buffers by streamId for seq ordering (§5)
  const streamBuffersRef = useRef<Map<string, StreamBuffer>>(new Map());
  // Gap timeout handles by streamId
  const gapTimeoutsRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

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

    // De-dupe pending messages against server messages by content + timestamp
    // A pending message is considered a duplicate if there's a server message with:
    // 1. Same role (user)
    // 2. Same text content (or close match)
    // 3. Timestamp within a reasonable window (30 seconds)
    const DEDUPE_WINDOW_MS = 30_000;
    const isPendingDuplicate = (pending: PendingUserMessage) => {
      return messages.some(m => {
        if (m.role !== "user") return false;
        if (m.userText !== pending.text) return false;
        const serverTime = m.time?.created ?? 0;
        const timeDiff = Math.abs(serverTime - pending.timestamp);
        return timeDiff < DEDUPE_WINDOW_MS;
      });
    };

    // Convert pending user messages to tracked messages (if not duplicated)
    const pendingAsTracked: TrackedMessage[] = pendingUserMessages
      .filter(p => !isPendingDuplicate(p))
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

    // Collect user message texts for deduplication
    const userMessageTexts = new Set(
      newItems
        .filter((item): item is OpenCodeThreadItem & { kind: "user-message" } =>
          item.kind === "user-message"
        )
        .map(item => item.text.trim().toLowerCase())
    );

    // Filter out assistant messages that exactly match user messages (likely echo/duplication)
    const dedupedItems = newItems.filter(item => {
      if (item.kind !== "assistant-message") return true;
      const normalizedText = item.text.trim().toLowerCase();
      // Skip assistant messages that are exact matches of user messages
      return !userMessageTexts.has(normalizedText);
    });

    // Apply normalization and bounds
    const preparedItems = prepareThreadItems(dedupedItems);

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

    // Track processing timing from incomplete assistant messages
    const hasIncompleteAssistant = allMessages.some(
      (m) => m.role === "assistant" && !m.time?.completed
    );
    if (hasIncompleteAssistant && !processingStartedAtRef.current) {
      const assistantMsg = allMessages.find(
        (m) => m.role === "assistant" && !m.time?.completed
      );
      setProcessingStartedAt(assistantMsg?.time?.created ?? Date.now());
    }
  }, [convertMessageToItems, pendingUserMessages, sessionId]);

  // Track previous session to detect real session switches
  const prevSessionRef = useRef<{ workspaceId: string | null; sessionId: string | null }>({
    workspaceId: null,
    sessionId: null,
  });

  // Load history from API and merge into tracked messages
  const loadHistory = useCallback(async (wsId: string, sessId: string) => {
    try {
      const response = await opencodeSessionMessages(wsId, sessId) as ApiMessageResponse[];
      if (!Array.isArray(response)) return;

      for (const apiMsg of response) {
        // Skip if we already have this message with parts
        const existing = messagesRef.current.get(apiMsg.id);
        if (existing && existing.parts.size > 0) continue;

        const tracked: TrackedMessage = {
          id: apiMsg.id,
          sessionID: apiMsg.sessionID || sessId,
          role: apiMsg.role,
          time: apiMsg.time,
          userText: apiMsg.role === "user" ? apiMsg.summary?.title : undefined,
          parts: new Map(),
        };

        // Convert API parts to tracked parts
        if (apiMsg.parts && Array.isArray(apiMsg.parts)) {
          for (const apiPart of apiMsg.parts) {
            const order = apiPart.time?.start ?? apiMsg.time?.created ?? Date.now();
            const arrivalIndex = partArrivalCounterRef.current++;
            tracked.parts.set(apiPart.id, {
              id: apiPart.id,
              messageID: apiMsg.id,
              sessionID: apiMsg.sessionID || sessId,
              type: apiPart.type,
              text: apiPart.text,
              content: apiPart.content,
              tool: apiPart.tool,
              callID: apiPart.callID,
              title: apiPart.title,
              input: apiPart.input,
              output: apiPart.output,
              error: apiPart.error,
              hash: apiPart.hash,
              files: apiPart.files,
              cost: apiPart.cost,
              tokens: apiPart.tokens,
              time: apiPart.time,
              order,
              arrivalIndex,
            });
          }
        }

        messagesRef.current.set(apiMsg.id, tracked);
      }

      rebuildItemsRef.current();
    } catch (err) {
      console.warn("[useOpenCodeThread] Failed to load history:", err);
    }
  }, []);

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
      // Clear stream buffers and gap timeouts
      streamBuffersRef.current.clear();
      for (const timeout of gapTimeoutsRef.current.values()) {
        clearTimeout(timeout);
      }
      gapTimeoutsRef.current.clear();
      setItems([]);
      setProcessingStartedAt(null);
      setLastDurationMs(null);
      setError(undefined);
    }

    // Load history for new session
    if (workspaceId && sessionId) {
      loadHistory(workspaceId, sessionId);
    }
  }, [workspaceId, sessionId, loadHistory]);

  // Keep rebuildItems in a ref to avoid subscription churn
  const rebuildItemsRef = useRef(rebuildItems);
  useEffect(() => {
    rebuildItemsRef.current = rebuildItems;
  }, [rebuildItems]);

  // Rebuild when pending messages change and track timing
  useEffect(() => {
    if (pendingUserMessages.length > 0) {
      rebuildItemsRef.current();
      // Track processing start time when user sends a message
      if (!processingStartedAtRef.current) {
        setProcessingStartedAt(Date.now());
      }
    }
  }, [pendingUserMessages]);

  // Process a single StreamEvent and update tracked messages
  const processStreamEvent = useCallback((event: StreamEvent) => {
    const messageId = event.messageId ?? event.streamId;
    const order = event.timestampMs;
    const arrivalIndex = partArrivalCounterRef.current++;

    // Get or create the message
    let msg = messagesRef.current.get(messageId);
    if (!msg) {
      msg = {
        id: messageId,
        sessionID: event.sessionId,
        role: "assistant",
        parts: new Map(),
      };
      messagesRef.current.set(messageId, msg);
    }

    // Handle text_delta (§3)
    if (isTextDelta(event)) {
      const partId = `${messageId}-text`;
      const existingPart = msg.parts.get(partId);
      msg.parts.set(partId, {
        id: partId,
        messageID: messageId,
        sessionID: event.sessionId,
        type: "text",
        text: existingPart
          ? mergeStreamingText(existingPart.text || "", event.payload.text)
          : event.payload.text,
        order: existingPart?.order ?? order,
        arrivalIndex: existingPart?.arrivalIndex ?? arrivalIndex,
      });
      return true;
    }

    // Handle thinking_delta (§3)
    if (isThinkingDelta(event)) {
      const partId = `${messageId}-thinking`;
      const existingPart = msg.parts.get(partId);
      msg.parts.set(partId, {
        id: partId,
        messageID: messageId,
        sessionID: event.sessionId,
        type: "reasoning",
        content: existingPart
          ? mergeStreamingText(existingPart.content || "", event.payload.text)
          : event.payload.text,
        order: existingPart?.order ?? order,
        arrivalIndex: existingPart?.arrivalIndex ?? arrivalIndex,
      });
      return true;
    }

    // Handle tool_call_delta (§3)
    if (isToolCallDelta(event)) {
      const partId = event.payload.callId;
      const existingPart = msg.parts.get(partId);
      msg.parts.set(partId, {
        id: partId,
        messageID: messageId,
        sessionID: event.sessionId,
        type: "tool",
        tool: event.payload.toolName,
        callID: event.payload.callId,
        // Accumulate arguments delta as input preview (will be replaced on completion)
        input: existingPart?.input ?? {},
        order: existingPart?.order ?? order,
        arrivalIndex: existingPart?.arrivalIndex ?? arrivalIndex,
      });
      return true;
    }

    // Handle tool_call_completed (§3)
    if (isToolCallCompleted(event)) {
      const partId = event.payload.callId;
      const existingPart = msg.parts.get(partId);
      msg.parts.set(partId, {
        id: partId,
        messageID: messageId,
        sessionID: event.sessionId,
        type: "tool",
        tool: event.payload.toolName,
        callID: event.payload.callId,
        input: event.payload.arguments,
        output: event.payload.output,
        error: event.payload.errorMessage,
        order: existingPart?.order ?? order,
        arrivalIndex: existingPart?.arrivalIndex ?? arrivalIndex,
      });
      return true;
    }

    // Handle completed (§3) - terminal for streamId
    if (isCompleted(event)) {
      // Add step-finish part with usage info
      const partId = `${messageId}-finish`;
      msg.parts.set(partId, {
        id: partId,
        messageID: messageId,
        sessionID: event.sessionId,
        type: "step-finish",
        tokens: {
          input: event.payload.usage.inputTokens,
          output: event.payload.usage.outputTokens,
          reasoning: event.payload.usage.reasoningTokens ?? 0,
        },
        order,
        arrivalIndex,
      });

      // Mark message as completed
      msg.time = { ...(msg.time || {}), completed: Date.now() };

      // Calculate duration for timing display
      const startedAt = processingStartedAtRef.current;
      if (startedAt) {
        setLastDurationMs(Date.now() - startedAt);
      }
      setProcessingStartedAt(null);
      return true;
    }

    // Handle error (§6)
    if (isError(event)) {
      setError(event.payload.message);
      return true;
    }

    return false; // Unknown event type or status events (handled by useAgentSession)
  }, []);

  // Process buffered events in seq order (§5)
  const flushBuffer = useCallback((buffer: StreamBuffer) => {
    // Sort by seq
    buffer.events.sort((a, b) => a.seq - b.seq);

    let anyUpdates = false;
    const processedSeqs: number[] = [];

    for (const event of buffer.events) {
      // Skip if we've already processed up to this seq
      if (event.seq <= buffer.lastSeq) {
        processedSeqs.push(event.seq);
        continue;
      }

      // Check for gap
      if (event.seq > buffer.lastSeq + 1) {
        // Gap detected, stop processing until gap fills or times out
        break;
      }

      // Process this event
      if (processStreamEvent(event)) {
        anyUpdates = true;
      }
      buffer.lastSeq = event.seq;
      processedSeqs.push(event.seq);
    }

    // Remove processed events
    buffer.events = buffer.events.filter(e => !processedSeqs.includes(e.seq));

    if (anyUpdates) {
      rebuildItemsRef.current();
    }

    return anyUpdates;
  }, [processStreamEvent]);

  // Subscribe to unified StreamEvents (§4: agent:stream_event)
  useEffect(() => {
    if (!sessionId) {
      return;
    }

    const unsubscribe = subscribeStreamEvents((event: StreamEvent) => {
      // Filter by session
      if (event.sessionId !== sessionId) {
        return;
      }

      const streamId = event.streamId;

      // Get or create buffer for this stream (§5)
      let buffer = streamBuffersRef.current.get(streamId);
      if (!buffer) {
        buffer = createStreamBuffer(streamId);
        streamBuffersRef.current.set(streamId, buffer);
      }

      // If stream is completed, ignore further events (§5)
      if (buffer.completed) {
        return;
      }

      // Mark completed if this is a terminal event
      if (isCompleted(event) || isError(event)) {
        buffer.completed = true;
        // Clear any gap timeout
        const timeout = gapTimeoutsRef.current.get(streamId);
        if (timeout) {
          clearTimeout(timeout);
          gapTimeoutsRef.current.delete(streamId);
        }
      }

      // Add to buffer
      buffer.events.push(event);

      // Check for gaps (§5)
      const expectedSeq = buffer.lastSeq + 1;
      if (event.seq > expectedSeq) {
        // Gap detected - track it and set timeout
        if (!buffer.gaps.includes(expectedSeq)) {
          buffer.gaps.push(expectedSeq);
        }

        // Set gap timeout if not already set (§5: 5 second gap timeout)
        if (!gapTimeoutsRef.current.has(streamId)) {
          const timeout = setTimeout(() => {
            // Gap persisted - log and continue processing
            console.warn(
              `[useOpenCodeThread] Stream gap timeout for ${streamId}, skipping seq ${expectedSeq}`
            );
            // Force lastSeq forward to skip the gap
            const buf = streamBuffersRef.current.get(streamId);
            if (buf) {
              buf.lastSeq = event.seq - 1;
              flushBuffer(buf);
            }
            gapTimeoutsRef.current.delete(streamId);
          }, STREAM_GAP_TIMEOUT_MS);
          gapTimeoutsRef.current.set(streamId, timeout);
        }
      } else {
        // No gap - clear any pending timeout
        const timeout = gapTimeoutsRef.current.get(streamId);
        if (timeout) {
          clearTimeout(timeout);
          gapTimeoutsRef.current.delete(streamId);
        }
      }

      // Attempt to flush buffer
      flushBuffer(buffer);
    });

    // Cleanup on unmount or session change
    return () => {
      unsubscribe();
      // Clear all gap timeouts
      for (const timeout of gapTimeoutsRef.current.values()) {
        clearTimeout(timeout);
      }
      gapTimeoutsRef.current.clear();
    };
  }, [sessionId, flushBuffer, processStreamEvent]);

  return {
    items,
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
