use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent_state::{
    AgentEvent, AgentState, AgentStateKind, InvalidTransition, TransitionResult,
};

/// Represents an agent session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub name: String,
    pub harness: AgentHarness,
    pub project_path: String,
    pub status: SessionStatus,
    /// Current state machine state kind (see §3 of agent-state-machine spec).
    pub agent_state: AgentStateKind,
}

// ============================================================================
// Session Registry (§2)
// ============================================================================

/// Internal session entry holding full state machine state.
#[derive(Debug)]
pub struct SessionEntry {
    pub session: AgentSession,
    pub state: AgentState,
}

/// Session registry: holds all active sessions with their state machines.
/// Sessions are isolated; events for a session are processed in arrival order.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: HashMap<String, SessionEntry>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Insert a new session into the registry.
    pub fn insert(&mut self, id: String, entry: SessionEntry) {
        self.sessions.insert(id, entry);
    }

    /// Get a session by ID.
    pub fn get(&self, id: &str) -> Option<&SessionEntry> {
        self.sessions.get(id)
    }

    /// Get a mutable reference to a session by ID.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut SessionEntry> {
        self.sessions.get_mut(id)
    }

    /// Remove a session from the registry.
    pub fn remove(&mut self, id: &str) -> Option<SessionEntry> {
        self.sessions.remove(id)
    }

    /// List all sessions.
    pub fn list(&self) -> Vec<&AgentSession> {
        self.sessions.values().map(|e| &e.session).collect()
    }
}

/// Thread-safe session registry for use with Tauri state.
pub type SharedSessionRegistry = Arc<RwLock<SessionRegistry>>;

/// Create a new shared session registry.
pub fn new_session_registry() -> SharedSessionRegistry {
    Arc::new(RwLock::new(SessionRegistry::new()))
}

// ============================================================================
// Session Event Processing (§2, §4)
// ============================================================================

/// Result of processing an event through the session's state machine.
#[derive(Debug)]
pub struct EventProcessingResult {
    pub transition: TransitionResult,
    pub previous_kind: AgentStateKind,
}

/// Process an event for a session. This is the main entry point for the session event loop.
///
/// Per spec §2:
/// - All state transitions happen in `handle_event` and are synchronous.
/// - I/O occurs outside the state machine.
/// - `AgentAction` is advisory; callers must emit events for success/failure outcomes.
///
/// Returns the transition result or an error if the transition is invalid.
pub fn process_event(
    entry: &mut SessionEntry,
    event: &AgentEvent,
) -> Result<EventProcessingResult, InvalidTransition> {
    let previous_kind = entry.state.kind;
    let transition = entry.state.handle_event(event, &entry.session.id)?;

    // Sync the AgentStateKind to the session summary for UI consumption
    entry.session.agent_state = entry.state.kind;

    // Update session status based on state kind
    entry.session.status = match entry.state.kind {
        AgentStateKind::Idle | AgentStateKind::Starting => SessionStatus::Idle,
        AgentStateKind::Stopped => SessionStatus::Stopped,
        _ => SessionStatus::Running,
    };

    Ok(EventProcessingResult {
        transition,
        previous_kind,
    })
}

/// Finalize response processing after stream completes.
/// Called by orchestrator to transition from ProcessingResponse to either Ready or ExecutingTools.
pub fn finalize_response(entry: &mut SessionEntry) -> TransitionResult {
    let result = entry.state.finalize_response(&entry.session.id);
    entry.session.agent_state = entry.state.kind;
    entry.session.status = match entry.state.kind {
        AgentStateKind::Idle | AgentStateKind::Starting => SessionStatus::Idle,
        AgentStateKind::Stopped => SessionStatus::Stopped,
        _ => SessionStatus::Running,
    };
    result
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Generate a session ID per spec §3: `sess_<uuid>`
fn generate_session_id() -> String {
    format!("sess_{}", uuid::Uuid::new_v4())
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
    let id = generate_session_id();

    // Initialize state machine and transition to Starting (per spec §5)
    let mut state = AgentState::default();
    state.start(); // Idle -> Starting

    let session = AgentSession {
        id,
        name,
        harness,
        project_path,
        status: SessionStatus::Idle, // Starting maps to Idle status
        agent_state: state.kind,
    };

    // TODO: Add to shared registry when Tauri state is wired in Phase 2
    // For now, return the session with proper state machine initialization

    Ok(session)
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
    use super::*;
    use crate::agent_state::AgentEvent;

    // ========================================================================
    // Session Registry Tests
    // ========================================================================

    #[test]
    fn session_registry_crud_operations() {
        let mut registry = SessionRegistry::new();
        let session_id = "sess_test_123".to_string();

        let entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Idle,
                agent_state: AgentStateKind::Idle,
            },
            state: AgentState::default(),
        };

        registry.insert(session_id.clone(), entry);
        assert!(registry.get(&session_id).is_some());
        assert_eq!(registry.list().len(), 1);

        let removed = registry.remove(&session_id);
        assert!(removed.is_some());
        assert!(registry.get(&session_id).is_none());
    }

    // ========================================================================
    // Event Processing Tests (§2, §4)
    // ========================================================================

    #[test]
    fn process_event_transitions_ready_to_calling_llm() {
        let session_id = "sess_test".to_string();
        let mut entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Idle,
                agent_state: AgentStateKind::Ready,
            },
            state: AgentState {
                kind: AgentStateKind::Ready,
                ..Default::default()
            },
        };

        let event = AgentEvent::UserInput {
            session_id: session_id.clone(),
            text: "hello".to_string(),
        };

        let result = process_event(&mut entry, &event);
        assert!(result.is_ok());

        let result = result.unwrap();
        assert_eq!(result.previous_kind, AgentStateKind::Ready);
        assert_eq!(result.transition.new_kind, AgentStateKind::CallingLlm);

        // Verify session summary is synced
        assert_eq!(entry.session.agent_state, AgentStateKind::CallingLlm);
        assert_eq!(entry.session.status, SessionStatus::Running);
    }

    #[test]
    fn process_event_invalid_transition_preserves_state() {
        let session_id = "sess_test".to_string();
        let mut entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Idle,
                agent_state: AgentStateKind::Idle,
            },
            state: AgentState::default(), // Idle
        };

        // UserInput is invalid from Idle (need to spawn first)
        let event = AgentEvent::UserInput {
            session_id: session_id.clone(),
            text: "hello".to_string(),
        };

        let result = process_event(&mut entry, &event);
        assert!(result.is_err());

        // State should be unchanged
        assert_eq!(entry.state.kind, AgentStateKind::Idle);
        assert_eq!(entry.session.agent_state, AgentStateKind::Idle);
    }

    #[test]
    fn finalize_response_transitions_to_ready_when_no_tools() {
        let session_id = "sess_test".to_string();
        let mut entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Running,
                agent_state: AgentStateKind::ProcessingResponse,
            },
            state: AgentState {
                kind: AgentStateKind::ProcessingResponse,
                pending_tool_calls: vec![], // No tools
                ..Default::default()
            },
        };

        let result = finalize_response(&mut entry);
        assert_eq!(result.new_kind, AgentStateKind::Ready);
        assert_eq!(entry.session.agent_state, AgentStateKind::Ready);
        assert_eq!(entry.session.status, SessionStatus::Running);
    }

    #[test]
    fn spawn_session_initializes_state_machine() {
        // This test verifies spawn_session creates proper session IDs
        // and initializes the state machine to Starting
        let rt = tokio::runtime::Runtime::new().unwrap();
        let session = rt.block_on(spawn_session(
            AgentHarness::ClaudeCode,
            "/tmp/test-project".to_string(),
        ));

        assert!(session.is_ok());
        let session = session.unwrap();

        // Session ID should match spec format: sess_<uuid>
        assert!(session.id.starts_with("sess_"));
        assert!(session.id.len() > 5); // "sess_" + uuid

        // State should be Starting (after state.start())
        assert_eq!(session.agent_state, AgentStateKind::Starting);
        assert_eq!(session.name, "test-project");
    }

    // ========================================================================
    // Git Parsing Tests (existing)
    // ========================================================================

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
