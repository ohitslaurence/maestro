import type { GitFileDiff } from "../../../types";

type DiffViewerProps = {
  diff: GitFileDiff | null;
  isLoading: boolean;
};

type DiffLine = {
  type: "context" | "addition" | "deletion" | "header";
  content: string;
  lineNumber?: number;
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
      parsed.push({ type: "addition", content: line.slice(1), lineNumber: newLine });
      newLine++;
    } else if (line.startsWith("-")) {
      parsed.push({ type: "deletion", content: line.slice(1), lineNumber: oldLine });
      oldLine++;
    } else if (line.startsWith(" ")) {
      parsed.push({ type: "context", content: line.slice(1), lineNumber: newLine });
      oldLine++;
      newLine++;
    } else if (line.length > 0) {
      parsed.push({ type: "context", content: line });
    }
  }

  return parsed;
}

export function DiffViewer({ diff, isLoading }: DiffViewerProps) {
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

  const lines = parseDiff(diff.diff);

  return (
    <div className="diff-viewer">
      <div className="diff-header">
        <span className="diff-path">{diff.path}</span>
      </div>
      <div className="diff-content">
        <pre>
          {lines.map((line, i) => (
            <div key={i} className={`diff-line diff-line--${line.type}`}>
              <span className="diff-line-number">
                {line.lineNumber ?? ""}
              </span>
              <span className="diff-line-content">{line.content}</span>
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}
