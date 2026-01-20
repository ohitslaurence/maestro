use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::protocol::{GitDiffResult, GitFileDiff, GitFileStatus, GitLogEntry, GitLogResult, GitStatusResult};

/// Max diff size before truncation (1MB)
const MAX_DIFF_SIZE: usize = 1_000_000;

/// Check if path is a git repository
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get git status for a repository
pub fn get_status(path: &Path) -> Result<GitStatusResult, String> {
    if !is_git_repo(path) {
        return Ok(GitStatusResult {
            branch_name: String::new(),
            staged_files: vec![],
            unstaged_files: vec![],
            total_additions: 0,
            total_deletions: 0,
        });
    }

    // Get branch name
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to get branch: {e}"))?;

    let branch_name = String::from_utf8_lossy(&branch_output.stdout)
        .trim()
        .to_string();

    // Get status with porcelain format
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to get status: {e}"))?;

    let (mut staged_files, mut unstaged_files) = parse_porcelain_status(&status_output.stdout);

    // Get diff stats for staged files
    let staged_stats_output = Command::new("git")
        .args(["diff", "--cached", "--numstat"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to get staged stats: {e}"))?;

    let staged_stats = parse_numstat(&staged_stats_output.stdout);

    // Get diff stats for unstaged files
    let unstaged_stats_output = Command::new("git")
        .args(["diff", "--numstat"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to get unstaged stats: {e}"))?;

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

    Ok(GitStatusResult {
        branch_name,
        staged_files,
        unstaged_files,
        total_additions,
        total_deletions,
    })
}

/// Get git diffs with truncation for large files
pub fn get_diff(path: &Path) -> Result<GitDiffResult, String> {
    if !is_git_repo(path) {
        return Ok(GitDiffResult {
            files: vec![],
            truncated: false,
            truncated_files: vec![],
        });
    }

    let mut files = Vec::new();
    let mut truncated_files = Vec::new();
    let mut total_size = 0usize;
    let mut truncated = false;

    // Get staged changed files
    let staged_files_output = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to list staged files: {e}"))?;

    let staged_paths: Vec<String> = String::from_utf8_lossy(&staged_files_output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Get diff for each staged file
    for file_path in staged_paths {
        if truncated {
            truncated_files.push(file_path);
            continue;
        }

        let diff_output = Command::new("git")
            .args(["diff", "--cached", "--", &file_path])
            .current_dir(path)
            .output()
            .map_err(|e| format!("Failed to get diff for {file_path}: {e}"))?;

        let diff = String::from_utf8_lossy(&diff_output.stdout).to_string();
        let diff_size = diff.len();

        if total_size + diff_size > MAX_DIFF_SIZE {
            truncated = true;
            truncated_files.push(file_path);
        } else {
            total_size += diff_size;
            files.push(GitFileDiff {
                path: file_path,
                diff,
            });
        }
    }

    // Get unstaged changed files
    let unstaged_files_output = Command::new("git")
        .args(["diff", "--name-only"])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to list unstaged files: {e}"))?;

    let unstaged_paths: Vec<String> = String::from_utf8_lossy(&unstaged_files_output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Get diff for each unstaged file (if not already in staged list)
    for file_path in unstaged_paths {
        if files.iter().any(|d| d.path == file_path) {
            continue;
        }

        if truncated {
            if !truncated_files.contains(&file_path) {
                truncated_files.push(file_path);
            }
            continue;
        }

        let diff_output = Command::new("git")
            .args(["diff", "--", &file_path])
            .current_dir(path)
            .output()
            .map_err(|e| format!("Failed to get diff for {file_path}: {e}"))?;

        let diff = String::from_utf8_lossy(&diff_output.stdout).to_string();
        let diff_size = diff.len();

        if total_size + diff_size > MAX_DIFF_SIZE {
            truncated = true;
            truncated_files.push(file_path);
        } else {
            total_size += diff_size;
            files.push(GitFileDiff {
                path: file_path,
                diff,
            });
        }
    }

    Ok(GitDiffResult {
        files,
        truncated,
        truncated_files,
    })
}

/// Get git log with upstream status
pub fn get_log(path: &Path, limit: u32) -> Result<GitLogResult, String> {
    if !is_git_repo(path) {
        return Ok(GitLogResult {
            entries: vec![],
            ahead: 0,
            behind: 0,
            upstream: None,
        });
    }

    // Get log with custom format (NUL-separated fields)
    let log_output = Command::new("git")
        .args([
            "log",
            &format!("-{limit}"),
            "--format=%H%x00%s%x00%an%x00%at",
        ])
        .current_dir(path)
        .output()
        .map_err(|e| format!("Failed to get log: {e}"))?;

    let entries = parse_log_output(&log_output.stdout);

    // Get upstream status
    let (ahead, behind, upstream) = get_upstream_status(path);

    Ok(GitLogResult {
        entries,
        ahead,
        behind,
        upstream,
    })
}

// --- Internal helpers ---

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

fn parse_numstat(output: &[u8]) -> HashMap<String, (i32, i32)> {
    let mut stats = HashMap::new();
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

fn get_upstream_status(path: &Path) -> (i32, i32, Option<String>) {
    // Get upstream branch name
    let upstream_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(path)
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
        .current_dir(path)
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
