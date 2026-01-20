import { useCallback } from "react";
import type { OpenCodeThreadItem } from "../../../types";

type ToolItem = Extract<OpenCodeThreadItem, { kind: "tool" }>;

type ToolRowProps = {
  item: ToolItem;
  isExpanded: boolean;
  onToggle: (id: string) => void;
};

function getFirstInputKey(input: Record<string, unknown>): string | null {
  const keys = Object.keys(input);
  if (keys.length === 0) return null;
  const firstKey = keys[0];
  const value = input[firstKey];
  if (typeof value === "string" && value.length < 60) {
    return `${firstKey}: ${value}`;
  }
  return firstKey;
}

export function ToolRow({ item, isExpanded, onToggle }: ToolRowProps) {
  const handleClick = useCallback(() => {
    onToggle(item.id);
  }, [item.id, onToggle]);

  const summary = item.title || getFirstInputKey(item.input) || item.tool;

  return (
    <div className={`oc-tool ${isExpanded ? "oc-tool--expanded" : ""}`}>
      <button type="button" className="oc-tool__header" onClick={handleClick}>
        <span className={`oc-tool__status oc-tool__status--${item.status}`} />
        <span className="oc-tool__name">{item.tool}</span>
        <span className="oc-tool__summary">{summary}</span>
        <span className="oc-tool__chevron">{isExpanded ? "−" : "+"}</span>
      </button>
      {isExpanded && (
        <div className="oc-tool__content">
          <div className="oc-tool__section">
            <div className="oc-tool__section-label">Input</div>
            <pre className="oc-tool__code">
              {JSON.stringify(item.input, null, 2)}
            </pre>
          </div>
          {item.output && (
            <div className="oc-tool__section">
              <div className="oc-tool__section-label">Output</div>
              <pre className="oc-tool__code">{item.output}</pre>
            </div>
          )}
          {item.error && (
            <div className="oc-tool__section oc-tool__section--error">
              <div className="oc-tool__section-label">Error</div>
              <pre className="oc-tool__code">{item.error}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
