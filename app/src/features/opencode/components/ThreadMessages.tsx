import { useCallback, useEffect, useRef, useState } from "react";
import type { OpenCodeThreadItem, OpenCodeThreadStatus } from "../../../types";
import { MessageRow } from "./MessageRow";
import { ToolRow } from "./ToolRow";
import { ReasoningRow } from "./ReasoningRow";

type ThreadMessagesProps = {
  items: OpenCodeThreadItem[];
  status: OpenCodeThreadStatus;
  processingStartedAt: number | null;
  lastDurationMs: number | null;
};

const AUTO_SCROLL_THRESHOLD = 120;

function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;

  if (minutes > 0) {
    return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
  }
  return `${remainingSeconds}s`;
}

export function ThreadMessages({
  items,
  status,
  processingStartedAt,
  lastDurationMs,
}: ThreadMessagesProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const shouldAutoScrollRef = useRef(true);
  const [elapsedMs, setElapsedMs] = useState(0);

  const handleToggle = useCallback((id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  // Track scroll position to determine if we should auto-scroll
  const handleScroll = useCallback(() => {
    const container = containerRef.current;
    if (!container) return;

    const { scrollTop, scrollHeight, clientHeight } = container;
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    shouldAutoScrollRef.current = distanceFromBottom < AUTO_SCROLL_THRESHOLD;
  }, []);

  // Auto-scroll when items change
  useEffect(() => {
    const container = containerRef.current;
    if (!container || !shouldAutoScrollRef.current) return;

    container.scrollTop = container.scrollHeight;
  }, [items]);

  // Update elapsed time during processing
  useEffect(() => {
    if (status !== "processing" || !processingStartedAt) {
      setElapsedMs(0);
      return;
    }

    // Initial update
    setElapsedMs(Date.now() - processingStartedAt);

    // Update every second
    const interval = setInterval(() => {
      setElapsedMs(Date.now() - processingStartedAt);
    }, 1000);

    return () => clearInterval(interval);
  }, [status, processingStartedAt]);

  const renderItem = (item: OpenCodeThreadItem) => {
    switch (item.kind) {
      case "user-message":
      case "assistant-message":
        return <MessageRow key={item.id} item={item} />;

      case "tool":
        return (
          <ToolRow
            key={item.id}
            item={item}
            isExpanded={expandedIds.has(item.id)}
            onToggle={handleToggle}
          />
        );

      case "reasoning":
        return (
          <ReasoningRow
            key={item.id}
            item={item}
            isExpanded={expandedIds.has(item.id)}
            onToggle={handleToggle}
          />
        );

      case "patch":
        return (
          <div key={item.id} className="oc-patch">
            <span className="oc-patch__icon">📝</span>
            <span className="oc-patch__text">
              Modified {item.files.length} file{item.files.length !== 1 ? "s" : ""}
            </span>
          </div>
        );

      case "step-finish":
        return (
          <div key={item.id} className="oc-step-finish">
            <span className="oc-step-finish__tokens">
              {item.tokens.input + item.tokens.output} tokens
            </span>
            {item.cost > 0 && (
              <span className="oc-step-finish__cost">
                ${item.cost.toFixed(4)}
              </span>
            )}
          </div>
        );

      default:
        return null;
    }
  };

  const isEmpty = items.length === 0;

  // Format the processing indicator text
  const processingText = status === "processing" && elapsedMs > 0
    ? `Working... ${formatDuration(elapsedMs)}`
    : "Working...";

  // Format the "done" indicator if we just finished
  const showDoneIndicator = status === "idle" && lastDurationMs !== null && lastDurationMs > 0;
  const doneText = lastDurationMs !== null ? `Done in ${formatDuration(lastDurationMs)}` : "";

  return (
    <div
      ref={containerRef}
      className="oc-messages"
      onScroll={handleScroll}
    >
      {isEmpty && status === "idle" && !showDoneIndicator && (
        <div className="oc-messages__empty">
          <p>No messages yet. Send a prompt to get started.</p>
        </div>
      )}
      {items.map(renderItem)}
      {status === "processing" && (
        <div className="oc-messages__processing">
          <span className="oc-messages__spinner" />
          <span>{processingText}</span>
        </div>
      )}
      {showDoneIndicator && items.length > 0 && (
        <div className="oc-messages__done">
          <span>{doneText}</span>
        </div>
      )}
      {status === "error" && (
        <div className="oc-messages__error">
          An error occurred
        </div>
      )}
    </div>
  );
}
