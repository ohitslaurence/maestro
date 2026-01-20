use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

/// Represents an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub name: String,
    pub harness: AgentHarness,
    pub project_path: String,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    pub path: String,
    pub status: String,
    pub additions: i32,
    pub deletions: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileDiff {
    pub path: String,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLogEntry {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLogResponse {
    pub total: i32,
    pub entries: Vec<GitLogEntry>,
    pub ahead: i32,
    pub behind: i32,
    pub ahead_entries: Vec<GitLogEntry>,
    pub behind_entries: Vec<GitLogEntry>,
    pub upstream: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch_name: String,
    pub files: Vec<GitFileStatus>,
    pub staged_files: Vec<GitFileStatus>,
    pub unstaged_files: Vec<GitFileStatus>,
    pub total_additions: i32,
    pub total_deletions: i32,
}

/// Supported agent harnesses (extensible)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHarness {
    ClaudeCode,
    OpenCode,
    // Future harnesses can be added here
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Idle,
    Stopped,
}

// ============================================================================
// Git Helper Functions
// ============================================================================

/// Get the repository path for a session.
/// TODO: Look up session's project_path from session store.
/// For now, returns current working directory.
fn get_session_path(_session_id: &str) -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))
}

/// Parse git status --porcelain=v1 output into staged and unstaged file lists.
/// Format: XY filename
/// X = index (staged) status, Y = worktree (unstaged) status
/// ' ' = unmodified, M = modified, A = added, D = deleted, ? = untracked
fn parse_porcelain_status(output: &[u8]) -> (Vec<GitFileStatus>, Vec<GitFileStatus>) {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();

    let output_str = String::from_utf8_lossy(output);
    for line in output_str.lines() {
        if line.len() < 3 {
            continue;
        }

        let index_status = line.chars().next().unwrap_or(' ');
        let worktree_status = line.chars().nth(1).unwrap_or(' ');
        let path = line[3..].to_string();

        // Staged changes (index column)
        if index_status != ' ' && index_status != '?' {
            staged.push(GitFileStatus {
                path: path.clone(),
                status: status_char_to_string(index_status),
                additions: 0,
                deletions: 0,
            });
        }

        // Unstaged changes (worktree column) or untracked files
        if worktree_status != ' ' {
            unstaged.push(GitFileStatus {
                path,
                status: if index_status == '?' {
                    "untracked".to_string()
                } else {
                    status_char_to_string(worktree_status)
                },
                additions: 0,
                deletions: 0,
            });
        }
    }

    (staged, unstaged)
}

fn status_char_to_string(c: char) -> String {
    match c {
        'M' => "modified",
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'U' => "unmerged",
        '?' => "untracked",
        _ => "unknown",
    }
    .to_string()
}

/// Parse git diff --numstat output to get additions/deletions per file.
/// Format: additions<TAB>deletions<TAB>filename
fn parse_numstat(output: &[u8]) -> std::collections::HashMap<String, (i32, i32)> {
    let mut stats = std::collections::HashMap::new();
    let output_str = String::from_utf8_lossy(output);

    for line in output_str.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let additions = parts[0].parse::<i32>().unwrap_or(0);
            let deletions = parts[1].parse::<i32>().unwrap_or(0);
            let path = parts[2].to_string();
            stats.insert(path, (additions, deletions));
        }
    }

    stats
}

/// Get upstream tracking info (ahead, behind, upstream branch name).
fn get_upstream_status(repo_path: &PathBuf) -> (i32, i32, Option<String>) {
    // Get upstream branch name
    let upstream_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(repo_path)
        .output();

    let upstream = match upstream_output {
        Ok(output) if output.status.success() => {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        }
        _ => return (0, 0, None),
    };

    // Get ahead/behind counts
    let count_output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "HEAD...@{upstream}"])
        .current_dir(repo_path)
        .output();

    let (ahead, behind) = match count_output {
        Ok(output) if output.status.success() => {
            let counts = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = counts.trim().split('\t').collect();
            if parts.len() == 2 {
                (
                    parts[0].parse::<i32>().unwrap_or(0),
                    parts[1].parse::<i32>().unwrap_or(0),
                )
            } else {
                (0, 0)
            }
        }
        _ => (0, 0),
    };

    (ahead, behind, upstream)
}

/// Parse git log output with custom format.
fn parse_log_output(output: &[u8]) -> Vec<GitLogEntry> {
    let mut entries = Vec::new();
    let output_str = String::from_utf8_lossy(output);

    for line in output_str.lines() {
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() >= 4 {
            entries.push(GitLogEntry {
                sha: parts[0].to_string(),
                summary: parts[1].to_string(),
                author: parts[2].to_string(),
                timestamp: parts[3].parse::<i64>().unwrap_or(0),
            });
        }
    }

    entries
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// List active tmux sessions (interim discovery method)
/// In the future, this will query the daemon for proper session tracking
#[allow(dead_code)]
pub async fn list_sessions_local() -> Result<Vec<String>, String> {
    // For now, list tmux sessions as a starting point
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output()
        .map_err(|e| format!("Failed to list tmux sessions: {}", e))?;

    if !output.status.success() {
        // No tmux server running or no sessions
        return Ok(vec![]);
    }

    let sessions: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect();

    Ok(sessions)
}

#[tauri::command]
pub async fn spawn_session(
    harness: AgentHarness,
    project_path: String,
) -> Result<AgentSession, String> {
    let name = project_path
        .rsplit('/')
        .next()
        .unwrap_or("session")
        .to_string();
    Ok(AgentSession {
        id: format!("{}-stub", name),
        name,
        harness,
        project_path,
        status: SessionStatus::Running,
    })
}

#[tauri::command]
pub async fn stop_session(_session_id: String) -> Result<(), String> {
    Ok(())
}

#[allow(dead_code)]
pub async fn get_git_status_local(session_id: String) -> Result<GitStatus, String> {
    let repo_path = get_session_path(&session_id)?;

    // Check if we're in a git repo
    let git_check = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !git_check.status.success() {
        return Ok(GitStatus {
            branch_name: "".to_string(),
            files: vec![],
            staged_files: vec![],
            unstaged_files: vec![],
            total_additions: 0,
            total_deletions: 0,
        });
    }

    // Get branch name
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to get branch: {}", e))?;

    let branch_name = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Get status with porcelain format
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to get status: {}", e))?;

    let (mut staged_files, mut unstaged_files) = parse_porcelain_status(&status_output.stdout);

    // Get diff stats for staged files
    let staged_stats_output = Command::new("git")
        .args(["diff", "--cached", "--numstat"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to get staged stats: {}", e))?;

    let staged_stats = parse_numstat(&staged_stats_output.stdout);

    // Get diff stats for unstaged files
    let unstaged_stats_output = Command::new("git")
        .args(["diff", "--numstat"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to get unstaged stats: {}", e))?;

    let unstaged_stats = parse_numstat(&unstaged_stats_output.stdout);

    // Apply stats to file lists
    for file in &mut staged_files {
        if let Some((add, del)) = staged_stats.get(&file.path) {
            file.additions = *add;
            file.deletions = *del;
        }
    }

    for file in &mut unstaged_files {
        if let Some((add, del)) = unstaged_stats.get(&file.path) {
            file.additions = *add;
            file.deletions = *del;
        }
    }

    // Calculate totals
    let total_additions = staged_files.iter().map(|f| f.additions).sum::<i32>()
        + unstaged_files.iter().map(|f| f.additions).sum::<i32>();
    let total_deletions = staged_files.iter().map(|f| f.deletions).sum::<i32>()
        + unstaged_files.iter().map(|f| f.deletions).sum::<i32>();

    // Combined files list (all changed files)
    let mut files = staged_files.clone();
    for unstaged in &unstaged_files {
        if !files.iter().any(|f| f.path == unstaged.path) {
            files.push(unstaged.clone());
        }
    }

    Ok(GitStatus {
        branch_name,
        files,
        staged_files,
        unstaged_files,
        total_additions,
        total_deletions,
    })
}

#[allow(dead_code)]
pub async fn get_git_diffs_local(session_id: String) -> Result<Vec<GitFileDiff>, String> {
    let repo_path = get_session_path(&session_id)?;

    // Check if we're in a git repo
    let git_check = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !git_check.status.success() {
        return Ok(vec![]);
    }

    let mut diffs = Vec::new();

    // Get list of staged changed files
    let staged_files_output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to list staged files: {}", e))?;

    let staged_files: Vec<String> = String::from_utf8_lossy(&staged_files_output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Get diff for each staged file
    for path in staged_files {
        let diff_output = Command::new("git")
            .args(["diff", "--cached", "--", &path])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| format!("Failed to get diff for {}: {}", path, e))?;

        diffs.push(GitFileDiff {
            path,
            diff: String::from_utf8_lossy(&diff_output.stdout).to_string(),
        });
    }

    // Get list of unstaged changed files
    let unstaged_files_output = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to list unstaged files: {}", e))?;

    let unstaged_files: Vec<String> = String::from_utf8_lossy(&unstaged_files_output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Get diff for each unstaged file (if not already in staged list)
    for path in unstaged_files {
        if diffs.iter().any(|d| d.path == path) {
            continue;
        }

        let diff_output = Command::new("git")
            .args(["diff", "--", &path])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| format!("Failed to get diff for {}: {}", path, e))?;

        diffs.push(GitFileDiff {
            path,
            diff: String::from_utf8_lossy(&diff_output.stdout).to_string(),
        });
    }

    Ok(diffs)
}

#[allow(dead_code)]
pub async fn get_git_log_local(
    session_id: String,
    limit: Option<u32>,
) -> Result<GitLogResponse, String> {
    let repo_path = get_session_path(&session_id)?;
    let limit = limit.unwrap_or(40);

    // Check if we're in a git repo
    let git_check = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !git_check.status.success() {
        return Ok(GitLogResponse {
            total: 0,
            entries: vec![],
            ahead: 0,
            behind: 0,
            ahead_entries: vec![],
            behind_entries: vec![],
            upstream: None,
        });
    }

    // Get log with custom format (NUL-separated fields)
    // %H = full hash, %s = subject, %an = author, %at = author timestamp
    let log_output = Command::new("git")
        .args([
            "log",
            &format!("-{}", limit),
            "--format=%H%x00%s%x00%an%x00%at",
        ])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("Failed to get log: {}", e))?;

    let entries = parse_log_output(&log_output.stdout);

    // Get upstream status
    let (ahead, behind, upstream) = get_upstream_status(&repo_path);

    // Get ahead/behind commit entries if there's an upstream
    let mut ahead_entries = vec![];
    let mut behind_entries = vec![];

    if upstream.is_some() && ahead > 0 {
        let ahead_output = Command::new("git")
            .args([
                "log",
                "@{upstream}..HEAD",
                "--format=%H%x00%s%x00%an%x00%at",
            ])
            .current_dir(&repo_path)
            .output();

        if let Ok(output) = ahead_output {
            ahead_entries = parse_log_output(&output.stdout);
        }
    }

    if upstream.is_some() && behind > 0 {
        let behind_output = Command::new("git")
            .args([
                "log",
                "HEAD..@{upstream}",
                "--format=%H%x00%s%x00%an%x00%at",
            ])
            .current_dir(&repo_path)
            .output();

        if let Ok(output) = behind_output {
            behind_entries = parse_log_output(&output.stdout);
        }
    }

    Ok(GitLogResponse {
        total: entries.len() as i32,
        entries,
        ahead,
        behind,
        ahead_entries,
        behind_entries,
        upstream,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_log_output, parse_numstat, parse_porcelain_status};

    #[test]
    fn parse_porcelain_status_splits_staged_and_unstaged() {
        let input = b"M  staged.txt\n M unstaged.txt\nAM both.txt\n?? new.txt\n";
        let (staged, unstaged) = parse_porcelain_status(input);

        assert_eq!(staged.len(), 2);
        assert!(staged.iter().any(|file| file.path == "staged.txt"));
        assert!(staged.iter().any(|file| file.path == "both.txt"));

        assert_eq!(unstaged.len(), 3);
        assert!(unstaged.iter().any(|file| file.path == "unstaged.txt"));
        assert!(unstaged.iter().any(|file| file.path == "both.txt"));
        assert!(unstaged.iter().any(|file| file.path == "new.txt"));
    }

    #[test]
    fn parse_numstat_handles_missing_numbers() {
        let input = b"10\t2\tfoo.rs\n-\t-\tbin.dat\n";
        let stats = parse_numstat(input);

        assert_eq!(stats.get("foo.rs"), Some(&(10, 2)));
        assert_eq!(stats.get("bin.dat"), Some(&(0, 0)));
    }

    #[test]
    fn parse_log_output_reads_entries() {
        let input = b"abc123\0Fix bug\0Jane\01699999999\nxyz789\0Add feature\0Joe\01680000000\n";
        let entries = parse_log_output(input);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sha, "abc123");
        assert_eq!(entries[0].summary, "Fix bug");
        assert_eq!(entries[0].author, "Jane");
        assert_eq!(entries[0].timestamp, 1699999999);
    }
}

// Future commands:
// - spawn_session(harness: AgentHarness, project_path: String)
// - attach_session(session_id: String)
// - stop_session(session_id: String)
// - get_session_output(session_id: String)
