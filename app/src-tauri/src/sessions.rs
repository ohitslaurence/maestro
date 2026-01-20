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

// Future commands:
// - spawn_session(harness: AgentHarness, project_path: String)
// - attach_session(session_id: String)
// - stop_session(session_id: String)
// - get_session_output(session_id: String)
