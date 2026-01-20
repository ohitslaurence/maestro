import { useMemo, useState } from "react";
import type { GitFileDiff } from "../../../types";

type DiffViewerProps = {
  diff: GitFileDiff | null;
  isLoading: boolean;
};

type DiffLine = {
  type: "context" | "addition" | "deletion" | "header";
  content: string;
  oldLineNumber?: number;
  newLineNumber?: number;
};

type DiffRow = {
  type: DiffLine["type"];
  content?: string;
  left?: DiffLine;
  right?: DiffLine;
};

function parseDiff(diffText: string): DiffLine[] {
  const lines = diffText.split("\n");
  const parsed: DiffLine[] = [];
  let oldLine = 0;
  let newLine = 0;

  for (const line of lines) {
    if (line.startsWith("@@")) {
      // Parse hunk header: @@ -start,count +start,count @@
      const match = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (match) {
        oldLine = parseInt(match[1], 10);
        newLine = parseInt(match[2], 10);
      }
      parsed.push({ type: "header", content: line });
    } else if (line.startsWith("+++") || line.startsWith("---")) {
      parsed.push({ type: "header", content: line });
    } else if (line.startsWith("+")) {
      parsed.push({
        type: "addition",
        content: line.slice(1),
        newLineNumber: newLine,
      });
      newLine++;
    } else if (line.startsWith("-")) {
      parsed.push({
        type: "deletion",
        content: line.slice(1),
        oldLineNumber: oldLine,
      });
      oldLine++;
    } else if (line.startsWith(" ")) {
      parsed.push({
        type: "context",
        content: line.slice(1),
        oldLineNumber: oldLine,
        newLineNumber: newLine,
      });
      oldLine++;
      newLine++;
    } else if (line.length > 0) {
      parsed.push({ type: "context", content: line });
    }
  }

  return parsed;
}

function toRows(lines: DiffLine[]): DiffRow[] {
  return lines.map((line) => {
    if (line.type === "header") {
      return { type: "header", content: line.content };
    }

    if (line.type === "addition") {
      return { type: "addition", right: line };
    }

    if (line.type === "deletion") {
      return { type: "deletion", left: line };
    }

    return { type: "context", left: line, right: line };
  });
}

export function DiffViewer({ diff, isLoading }: DiffViewerProps) {
  const [mode, setMode] = useState<"split" | "unified">("split");

  const lines = useMemo(() => (diff ? parseDiff(diff.diff) : []), [diff]);
  const rows = useMemo(() => toRows(lines), [lines]);

  if (isLoading) {
    return (
      <div className="diff-viewer">
        <p className="diff-loading">Loading diff...</p>
      </div>
    );
  }

  if (!diff) {
    return (
      <div className="diff-viewer">
        <p className="diff-empty">Select a file to view diff</p>
      </div>
    );
  }

  const handleToggleMode = () => {
    setMode((prev) => (prev === "split" ? "unified" : "split"));
  };

  return (
    <div className={`diff-viewer diff-viewer--${mode}`}>
      <div className="diff-header">
        <span className="diff-path">{diff.path}</span>
        <div className="diff-header__actions">
          <button
            type="button"
            className="btn btn--ghost btn--xs"
            onClick={handleToggleMode}
          >
            {mode === "split" ? "Unified" : "Side-by-side"}
          </button>
        </div>
      </div>
      <div className="diff-content">
        {mode === "split" ? (
          <div className="diff-table">
            {rows.map((row, index) => {
              if (row.type === "header") {
                return (
                  <div key={index} className="diff-row diff-row--header">
                    <span className="diff-header-text">{row.content}</span>
                  </div>
                );
              }

              return (
                <div key={index} className={`diff-row diff-row--${row.type}`}>
                  <div className="diff-cell diff-cell--left">
                    <span className="diff-line-number">
                      {row.left?.oldLineNumber ?? ""}
                    </span>
                    <span className="diff-line-content">
                      {row.left?.content ?? ""}
                    </span>
                  </div>
                  <div className="diff-cell diff-cell--right">
                    <span className="diff-line-number">
                      {row.right?.newLineNumber ?? ""}
                    </span>
                    <span className="diff-line-content">
                      {row.right?.content ?? ""}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
        ) : (
          <pre>
            {lines.map((line, i) => (
              <div key={i} className={`diff-line diff-line--${line.type}`}>
                <span className="diff-line-number">
                  {line.newLineNumber ?? line.oldLineNumber ?? ""}
                </span>
                <span className="diff-line-content">{line.content}</span>
              </div>
            ))}
          </pre>
        )}
      </div>
    </div>
  );
}
