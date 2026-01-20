# Task: Implement Git Commands in Rust Backend

## Objective

Replace stubbed git commands in `sessions.rs` with real implementations using git CLI.

## Current State

Commands registered and typed, but return empty data:
- `get_git_status` - returns empty GitStatus
- `get_git_diffs` - returns empty vec
- `get_git_log` - returns empty GitLogResponse

Frontend wrappers in `services/tauri.ts` and types in `types.ts` already exist.

## Approach

Use `git` CLI rather than `git2` crate:
- Simpler, no native dependencies
- Matches CodexMonitor pattern
- Easier to debug

## Implementation Details

### get_git_status

```rust
#[tauri::command]
pub async fn get_git_status(session_id: String) -> Result<GitStatus, String> {
    // For now, use cwd. Later: look up session's project_path
    let repo_path = std::env::current_dir()
        .map_err(|e| e.to_string())?;

    // Get branch name
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| e.to_string())?;
    let branch_name = String::from_utf8_lossy(&branch.stdout).trim().to_string();

    // Get status with porcelain format for parsing
    // git status --porcelain=v1 -z
    let status = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| e.to_string())?;

    // Parse porcelain output into staged/unstaged files
    // Format: XY filename
    // X = staged status, Y = unstaged status
    let (staged, unstaged) = parse_porcelain_status(&status.stdout)?;

    // Get diff stats for additions/deletions
    // git diff --numstat (unstaged)
    // git diff --cached --numstat (staged)

    Ok(GitStatus {
        branch_name,
        files: vec![], // combined view
        staged_files: staged,
        unstaged_files: unstaged,
        total_additions,
        total_deletions,
    })
}
```

### get_git_diffs

```rust
#[tauri::command]
pub async fn get_git_diffs(session_id: String) -> Result<Vec<GitFileDiff>, String> {
    let repo_path = get_session_path(&session_id)?;

    // Get list of changed files
    let files = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(&repo_path)
        .output()?;

    // For each file, get the diff
    let mut diffs = Vec::new();
    for path in files {
        let diff = Command::new("git")
            .args(["diff", "--", &path])
            .current_dir(&repo_path)
            .output()?;

        diffs.push(GitFileDiff {
            path,
            diff: String::from_utf8_lossy(&diff.stdout).to_string(),
        });
    }

    Ok(diffs)
}
```

### get_git_log

```rust
#[tauri::command]
pub async fn get_git_log(session_id: String, limit: Option<u32>) -> Result<GitLogResponse, String> {
    let repo_path = get_session_path(&session_id)?;
    let limit = limit.unwrap_or(40);

    // Get log with custom format
    // %H = full hash, %s = subject, %an = author, %at = author timestamp
    let log = Command::new("git")
        .args([
            "log",
            &format!("-{}", limit),
            "--format=%H%x00%s%x00%an%x00%at",
        ])
        .current_dir(&repo_path)
        .output()?;

    let entries = parse_log_output(&log.stdout)?;

    // Get ahead/behind counts relative to upstream
    // git rev-list --left-right --count HEAD...@{upstream}
    let (ahead, behind, upstream) = get_upstream_status(&repo_path)?;

    Ok(GitLogResponse {
        total: entries.len() as i32,
        entries,
        ahead,
        behind,
        ahead_entries: vec![], // Can populate if needed
        behind_entries: vec![],
        upstream,
    })
}
```

### Helper Functions

```rust
fn get_session_path(session_id: &str) -> Result<PathBuf, String> {
    // TODO: Look up session's project_path from session store
    // For now, use current directory
    std::env::current_dir().map_err(|e| e.to_string())
}

fn parse_porcelain_status(output: &[u8]) -> Result<(Vec<GitFileStatus>, Vec<GitFileStatus>), String> {
    // Parse git status --porcelain output
    // XY filename
    // X = index status, Y = worktree status
    // ' ' = unmodified, M = modified, A = added, D = deleted, R = renamed, C = copied, U = unmerged, ? = untracked
}

fn get_upstream_status(repo_path: &Path) -> Result<(i32, i32, Option<String>), String> {
    // Get upstream branch name
    let upstream = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(repo_path)
        .output();

    // If no upstream, return zeros
    // Otherwise, get ahead/behind count
}
```

## Implementation Steps

1. Add helper function `get_session_path` (stub for now, returns cwd)
2. Implement `parse_porcelain_status` for git status parsing
3. Implement `get_git_status` with real git calls
4. Implement `get_git_diffs`
5. Add `parse_log_output` helper
6. Implement `get_upstream_status` helper
7. Implement `get_git_log`
8. Test each command via Tauri dev tools

## Test Plan

```bash
cd app
bun run tauri:dev
```

In browser console:
```javascript
await window.__TAURI__.core.invoke('get_git_status', { sessionId: 'test' })
await window.__TAURI__.core.invoke('get_git_diffs', { sessionId: 'test' })
await window.__TAURI__.core.invoke('get_git_log', { sessionId: 'test', limit: 10 })
```

## Acceptance Criteria

- [x] `get_git_status` returns real branch name and file statuses
- [x] `get_git_diffs` returns actual diff content for changed files
- [x] `get_git_log` returns commit history with ahead/behind counts
- [x] Commands work from app directory (uses cwd for now)
- [x] Graceful handling when not in a git repo
- [x] `cargo check` passes in `app/src-tauri/`
