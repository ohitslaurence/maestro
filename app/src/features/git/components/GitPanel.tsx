import { useCallback, useEffect, useMemo } from "react";
import { useGitStatus } from "../hooks/useGitStatus";
import { useGitDiffs } from "../hooks/useGitDiffs";
import { GitStatusPanel } from "./GitStatusPanel";
import { DiffViewer } from "./DiffViewer";

type GitPanelProps = {
  sessionId: string | null;
  hasGit?: boolean;
};

export function GitPanel({ sessionId, hasGit = true }: GitPanelProps) {
  const {
    status,
    isLoading: statusLoading,
    error: statusError,
  } = useGitStatus({ sessionId: hasGit ? sessionId : null });

  const {
    diffs,
    selectedPath,
    isLoading: diffsLoading,
    selectPath,
  } = useGitDiffs({ sessionId: hasGit ? sessionId : null });

  const diffPaths = useMemo(() => diffs.map((diff) => diff.path), [diffs]);

  const selectRelative = useCallback(
    (direction: 1 | -1) => {
      if (diffPaths.length === 0) {
        return;
      }
      const currentIndex = selectedPath
        ? diffPaths.indexOf(selectedPath)
        : -1;
      const nextIndex =
        currentIndex === -1
          ? 0
          : (currentIndex + direction + diffPaths.length) % diffPaths.length;
      selectPath(diffPaths[nextIndex]);
    },
    [diffPaths, selectedPath, selectPath],
  );

  useEffect(() => {
    if (diffPaths.length === 0) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
        return;
      }

      const target = event.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName.toLowerCase();
        if (
          tag === "input" ||
          tag === "textarea" ||
          target.isContentEditable
        ) {
          return;
        }
      }

      event.preventDefault();
      selectRelative(event.key === "ArrowDown" ? 1 : -1);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [diffPaths, selectRelative]);

  // Find the selected diff
  const selectedDiff = diffs.find((d) => d.path === selectedPath) ?? null;

  // Handle no session selected
  if (!sessionId) {
    return (
      <div className="git-panel git-panel--empty">
        <p>Select a session to view git status</p>
      </div>
    );
  }

  // Handle session without git
  if (!hasGit) {
    return (
      <div className="git-panel git-panel--empty">
        <p>This session is not a git repository</p>
      </div>
    );
  }

  return (
    <div className="git-panel">
      <div className="git-panel__status">
        <GitStatusPanel
          status={status}
          isLoading={statusLoading}
          error={statusError}
          onFileSelect={selectPath}
          selectedPath={selectedPath}
        />
      </div>
      <div className="git-panel__diff">
        <DiffViewer diff={selectedDiff} isLoading={diffsLoading && !!selectedPath} />
      </div>
    </div>
  );
}
