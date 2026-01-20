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
        />
      </div>
      <div className="git-panel__diff">
        <DiffViewer diff={selectedDiff} isLoading={diffsLoading && !!selectedPath} />
      </div>
    </div>
  );
}
