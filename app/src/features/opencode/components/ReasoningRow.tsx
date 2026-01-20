import { memo, useCallback } from "react";
import type { OpenCodeThreadItem } from "../../../types";

type ReasoningItem = Extract<OpenCodeThreadItem, { kind: "reasoning" }>;

type ReasoningRowProps = {
  item: ReasoningItem;
  isExpanded: boolean;
  onToggle: (id: string) => void;
};

function getFirstLine(text: string): string {
  const firstLine = text.split("\n")[0];
  if (firstLine.length > 80) {
    return firstLine.slice(0, 77) + "...";
  }
  return firstLine;
}

export const ReasoningRow = memo(function ReasoningRow({ item, isExpanded, onToggle }: ReasoningRowProps) {
  const handleClick = useCallback(() => {
    onToggle(item.id);
  }, [item.id, onToggle]);

  const preview = getFirstLine(item.text);

  return (
    <div className={`oc-reasoning ${isExpanded ? "oc-reasoning--expanded" : ""}`}>
      <button type="button" className="oc-reasoning__header" onClick={handleClick}>
        <span className="oc-reasoning__icon">💭</span>
        <span className="oc-reasoning__preview">{isExpanded ? "Reasoning" : preview}</span>
        <span className="oc-reasoning__chevron">{isExpanded ? "−" : "+"}</span>
      </button>
      {isExpanded && (
        <div className="oc-reasoning__content">
          <pre className="oc-reasoning__text">{item.text}</pre>
        </div>
      )}
    </div>
  );
});
