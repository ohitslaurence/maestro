use serde::{Deserialize, Serialize};
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

/// List active tmux sessions (interim discovery method)
/// In the future, this will query the daemon for proper session tracking
#[tauri::command]
pub async fn list_sessions() -> Result<Vec<String>, String> {
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

#[tauri::command]
pub async fn get_git_status(_session_id: String) -> Result<GitStatus, String> {
    Ok(GitStatus {
        branch_name: "".to_string(),
        files: vec![],
        staged_files: vec![],
        unstaged_files: vec![],
        total_additions: 0,
        total_deletions: 0,
    })
}

#[tauri::command]
pub async fn get_git_diffs(_session_id: String) -> Result<Vec<GitFileDiff>, String> {
    Ok(vec![])
}

#[tauri::command]
pub async fn get_git_log(
    _session_id: String,
    _limit: Option<u32>,
) -> Result<GitLogResponse, String> {
    Ok(GitLogResponse {
        total: 0,
        entries: vec![],
        ahead: 0,
        behind: 0,
        ahead_entries: vec![],
        behind_entries: vec![],
        upstream: None,
    })
}

// Future commands:
// - spawn_session(harness: AgentHarness, project_path: String)
// - attach_session(session_id: String)
// - stop_session(session_id: String)
// - get_session_output(session_id: String)
