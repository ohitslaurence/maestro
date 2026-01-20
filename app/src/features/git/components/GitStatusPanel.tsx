import type { GitStatus, GitFileStatus } from "../../../types";

type GitStatusPanelProps = {
  status: GitStatus | null;
  isLoading: boolean;
  error: string | null;
  onFileSelect?: (path: string) => void;
};

function FileList({
  files,
  title,
  onSelect,
}: {
  files: GitFileStatus[];
  title: string;
  onSelect?: (path: string) => void;
}) {
  if (files.length === 0) {
    return null;
  }

  return (
    <div className="git-file-list">
      <h4>{title}</h4>
      <ul>
        {files.map((file) => (
          <li
            key={file.path}
            className="git-file"
            onClick={() => onSelect?.(file.path)}
          >
            <span className={`git-status-badge git-status-${file.status.toLowerCase()}`}>
              {file.status[0]}
            </span>
            <span className="git-file-path">{file.path}</span>
            <span className="git-file-stats">
              {file.additions > 0 && (
                <span className="git-additions">+{file.additions}</span>
              )}
              {file.deletions > 0 && (
                <span className="git-deletions">-{file.deletions}</span>
              )}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function GitStatusPanel({
  status,
  isLoading,
  error,
  onFileSelect,
}: GitStatusPanelProps) {
  if (isLoading) {
    return (
      <div className="git-status-panel">
        <p className="git-loading">Loading git status...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="git-status-panel">
        <p className="git-error">{error}</p>
      </div>
    );
  }

  if (!status) {
    return (
      <div className="git-status-panel">
        <p className="git-empty">Select a session to view git status</p>
      </div>
    );
  }

  const hasChanges = status.stagedFiles.length > 0 || status.unstagedFiles.length > 0;

  return (
    <div className="git-status-panel">
      <div className="git-branch">
        <span className="git-branch-icon">&#9673;</span>
        <span className="git-branch-name">{status.branchName}</span>
      </div>

      {!hasChanges && (
        <p className="git-clean">Working tree clean</p>
      )}

      <FileList
        files={status.stagedFiles}
        title="Staged"
        onSelect={onFileSelect}
      />

      <FileList
        files={status.unstagedFiles}
        title="Changes"
        onSelect={onFileSelect}
      />

      {hasChanges && (
        <div className="git-summary">
          <span className="git-additions">+{status.totalAdditions}</span>
          <span className="git-deletions">-{status.totalDeletions}</span>
        </div>
      )}
    </div>
  );
}
