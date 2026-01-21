import { useCallback, useEffect, useMemo, useState } from "react";
import { useGitStatus } from "../hooks/useGitStatus";
import { useGitDiffs } from "../hooks/useGitDiffs";
import { useDiffStyle } from "../hooks/useDiffStyle";
import { GitDiffPanel } from "./GitDiffPanel";
import { GitDiffViewer } from "./GitDiffViewer";

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
    error: diffsError,
    selectPath,
  } = useGitDiffs({ sessionId: hasGit ? sessionId : null });

  const { diffStyle, setDiffStyle } = useDiffStyle();

  // scrollRequestId increments when user clicks a file to trigger scroll-to-file (§5.2)
  const [scrollRequestId, setScrollRequestId] = useState(0);

  // Combine staged + unstaged files for keyboard navigation (§7.6)
  const allPaths = useMemo(() => {
    const stagedPaths = (status?.stagedFiles ?? []).map((f) => f.path);
    const unstagedPaths = (status?.unstagedFiles ?? []).map((f) => f.path);
    return [...stagedPaths, ...unstagedPaths];
  }, [status?.stagedFiles, status?.unstagedFiles]);

  // Handle file selection from panel - increment scrollRequestId (§5.2)
  const handleSelectPath = useCallback(
    (path: string) => {
      selectPath(path);
      setScrollRequestId((prev) => prev + 1);
    },
    [selectPath],
  );

  // Handle active path change from viewer scroll sync (§5.3)
  const handleActivePathChange = useCallback(
    (path: string) => {
      // Update selection without triggering scroll-to-file
      selectPath(path);
    },
    [selectPath],
  );

  // Keyboard navigation (§7.6) - includes j/k and Home/End
  useEffect(() => {
    if (allPaths.length === 0) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
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

      const currentIndex = selectedPath ? allPaths.indexOf(selectedPath) : -1;
      let nextIndex: number | null = null;

      switch (event.key) {
        case "ArrowDown":
        case "j":
          nextIndex =
            currentIndex === -1 ? 0 : (currentIndex + 1) % allPaths.length;
          break;
        case "ArrowUp":
        case "k":
          nextIndex =
            currentIndex === -1
              ? allPaths.length - 1
              : (currentIndex - 1 + allPaths.length) % allPaths.length;
          break;
        case "Home":
          nextIndex = 0;
          break;
        case "End":
          nextIndex = allPaths.length - 1;
          break;
      }

      if (nextIndex !== null) {
        event.preventDefault();
        handleSelectPath(allPaths[nextIndex]);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [allPaths, selectedPath, handleSelectPath]);

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
        <GitDiffPanel
          branchName={status?.branchName ?? ""}
          stagedFiles={status?.stagedFiles ?? []}
          unstagedFiles={status?.unstagedFiles ?? []}
          selectedPath={selectedPath}
          onSelectPath={handleSelectPath}
          totalAdditions={status?.totalAdditions ?? 0}
          totalDeletions={status?.totalDeletions ?? 0}
          isLoading={statusLoading}
        />
      </div>
      <div className="git-panel__diff">
        <GitDiffViewer
          diffs={diffs}
          selectedPath={selectedPath}
          scrollRequestId={scrollRequestId}
          isLoading={diffsLoading}
          error={diffsError ?? statusError ?? null}
          diffStyle={diffStyle}
          onDiffStyleChange={setDiffStyle}
          onActivePathChange={handleActivePathChange}
        />
      </div>
    </div>
  );
}
