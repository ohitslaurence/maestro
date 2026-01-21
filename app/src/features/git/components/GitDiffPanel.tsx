import { memo } from "react";
import type { GitFileStatus } from "../../../types";

type GitDiffPanelProps = {
  branchName: string;
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
  totalAdditions: number;
  totalDeletions: number;
  isLoading: boolean;
};

type FileRowProps = {
  file: GitFileStatus;
  isSelected: boolean;
  onSelect: (path: string) => void;
};

/** Split a file path into directory, basename, and extension */
function splitPath(path: string): { dir: string; base: string; ext: string } {
  const lastSlash = path.lastIndexOf("/");
  const dir = lastSlash >= 0 ? path.slice(0, lastSlash) : "";
  const filename = lastSlash >= 0 ? path.slice(lastSlash + 1) : path;

  const lastDot = filename.lastIndexOf(".");
  if (lastDot > 0) {
    return {
      dir,
      base: filename.slice(0, lastDot),
      ext: filename.slice(lastDot),
    };
  }
  return { dir, base: filename, ext: "" };
}

const FileRow = memo(function FileRow({
  file,
  isSelected,
  onSelect,
}: FileRowProps) {
  const { dir, base, ext } = splitPath(file.path);
  const statusLower = file.status.toLowerCase();

  return (
    <li
      className={`git-diff-file${isSelected ? " git-diff-file--selected" : ""}`}
      onClick={() => onSelect(file.path)}
    >
      <span className={`git-diff-status git-status-${statusLower}`}>
        {file.status[0]}
      </span>
      <div className="git-diff-file-info">
        <span className="git-file-name">
          <span className="git-file-name-base">{base}</span>
          <span className="git-file-name-ext">{ext}</span>
        </span>
        {dir && <span className="git-file-dir">{dir}</span>}
      </div>
      <span className="git-diff-file-stats">
        {file.additions > 0 && (
          <span className="git-additions">+{file.additions}</span>
        )}
        {file.deletions > 0 && (
          <span className="git-deletions">-{file.deletions}</span>
        )}
      </span>
    </li>
  );
});

type FileSectionProps = {
  title: string;
  files: GitFileStatus[];
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
};

function FileSection({
  title,
  files,
  selectedPath,
  onSelectPath,
}: FileSectionProps) {
  if (files.length === 0) {
    return null;
  }

  return (
    <div className="git-diff-section">
      <h4 className="git-diff-section-header">
        {title}
        <span className="git-diff-section-count">{files.length}</span>
      </h4>
      <ul className="git-diff-file-list">
        {files.map((file) => (
          <FileRow
            key={file.path}
            file={file}
            isSelected={selectedPath === file.path}
            onSelect={onSelectPath}
          />
        ))}
      </ul>
    </div>
  );
}

export function GitDiffPanel({
  branchName,
  stagedFiles,
  unstagedFiles,
  selectedPath,
  onSelectPath,
  totalAdditions,
  totalDeletions,
  isLoading,
}: GitDiffPanelProps) {
  const hasChanges = stagedFiles.length > 0 || unstagedFiles.length > 0;

  if (isLoading) {
    return (
      <div className="git-diff-panel">
        <div className="git-diff-panel-header">
          <span className="git-diff-branch-icon">●</span>
          <span className="git-diff-branch-name">Loading...</span>
        </div>
      </div>
    );
  }

  // Handle no commits yet (empty branchName typically indicates this)
  if (!branchName) {
    return (
      <div className="git-diff-panel">
        <div className="git-diff-panel-empty">No commits yet</div>
      </div>
    );
  }

  return (
    <div className="git-diff-panel">
      <div className="git-diff-panel-header">
        <span className="git-diff-branch-icon">●</span>
        <span className="git-diff-branch-name">{branchName}</span>
        {hasChanges && (
          <span className="git-diff-panel-stats">
            <span className="git-additions">+{totalAdditions}</span>
            <span className="git-deletions">-{totalDeletions}</span>
          </span>
        )}
      </div>

      {!hasChanges && (
        <div className="git-diff-panel-empty">Working tree clean</div>
      )}

      <FileSection
        title="Staged"
        files={stagedFiles}
        selectedPath={selectedPath}
        onSelectPath={onSelectPath}
      />

      <FileSection
        title="Changes"
        files={unstagedFiles}
        selectedPath={selectedPath}
        onSelectPath={onSelectPath}
      />
    </div>
  );
}
