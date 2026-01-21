use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent_state::{
    emit_hook_lifecycle, emit_session_error, emit_state_changed, emit_tool_lifecycle, AgentError,
    AgentEvent, AgentState, AgentStateKind, ErrorSource, HookRunRecord, InvalidTransition,
    ToolRunRecord, TransitionResult,
};

// ============================================================================
// Unified Streaming Event Schema (specs/streaming-event-schema.md §3)
// ============================================================================

/// Current schema version for streaming events.
pub const STREAM_SCHEMA_VERSION: &str = "1.0";

/// Channel name for streaming events (§4).
pub const STREAM_EVENT_CHANNEL: &str = "agent:stream_event";

/// Event types for streaming events (§3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventType {
    TextDelta,
    ToolCallDelta,
    ToolCallCompleted,
    Completed,
    Error,
    Status,
    ThinkingDelta,
    ArtifactDelta,
    Metadata,
}

// ============================================================================
// Payload Types (§3)
// ============================================================================

/// Payload for text_delta events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDeltaPayload {
    pub text: String,
    pub role: String, // Always "assistant"
}

/// Payload for tool_call_delta events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDeltaPayload {
    pub call_id: String,
    pub tool_name: String,
    pub arguments_delta: String,
}

/// Status of a completed tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Completed,
    Failed,
    Canceled,
}

/// Payload for tool_call_completed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallCompletedPayload {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub output: String,
    pub status: ToolCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Reason for stream completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    Stop,
    Length,
    ToolError,
    UserAbort,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

/// Payload for completed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedPayload {
    pub reason: CompletionReason,
    pub usage: TokenUsage,
}

/// Error codes for stream errors (§6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamErrorCode {
    ProviderError,
    StreamGap,
    ProtocolError,
    ToolError,
    SessionAborted,
}

/// Payload for error events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: StreamErrorCode,
    pub message: String,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Agent processing state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProcessingState {
    Idle,
    Processing,
    Waiting,
    Aborted,
}

/// Payload for status events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub state: AgentProcessingState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Payload for thinking_delta events (optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingDeltaPayload {
    pub text: String,
}

/// Payload for artifact_delta events (optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDeltaPayload {
    pub artifact_id: String,
    pub artifact_type: String,
    pub content_delta: String,
}

/// Payload for metadata events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataPayload {
    pub model: String,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
}

/// All payload types as a tagged enum for type-safe serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEventPayload {
    TextDelta(TextDeltaPayload),
    ToolCallDelta(ToolCallDeltaPayload),
    ToolCallCompleted(ToolCallCompletedPayload),
    Completed(CompletedPayload),
    Error(ErrorPayload),
    Status(StatusPayload),
    ThinkingDelta(ThinkingDeltaPayload),
    ArtifactDelta(ArtifactDeltaPayload),
    Metadata(MetadataPayload),
}

// ============================================================================
// StreamEvent Envelope (§3)
// ============================================================================

/// Unified streaming event envelope.
///
/// Per spec §3: All streaming events from harnesses are normalized to this format.
/// The envelope provides ordering metadata (`streamId`, `seq`) and the `type`/`payload`
/// discriminated union for event-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamEvent {
    /// Schema version, always "1.0".
    pub schema_version: String,
    /// Unique identifier for this event.
    pub event_id: String,
    /// Maestro session ID.
    pub session_id: String,
    /// Harness that produced this event (e.g., "claude_code", "open_code").
    pub harness: String,
    /// Provider that generated the response (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Stable identifier for the current assistant response stream.
    pub stream_id: String,
    /// Monotonically increasing sequence number per streamId.
    pub seq: u64,
    /// Unix epoch milliseconds when event was created.
    pub timestamp_ms: u64,
    /// Event type discriminator.
    #[serde(rename = "type")]
    pub event_type: StreamEventType,
    /// Type-specific payload data.
    pub payload: serde_json::Value,
    /// Provider message ID (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Parent message ID for threading (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
}

impl StreamEvent {
    /// Create a new StreamEvent with required fields.
    ///
    /// Generates a unique `event_id` and sets `timestamp_ms` to current time.
    pub fn new(
        session_id: String,
        harness: String,
        stream_id: String,
        seq: u64,
        event_type: StreamEventType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            schema_version: STREAM_SCHEMA_VERSION.to_string(),
            event_id: format!("evt_{}", uuid::Uuid::new_v4()),
            session_id,
            harness,
            provider: None,
            stream_id,
            seq,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            event_type,
            payload,
            message_id: None,
            parent_message_id: None,
        }
    }

    /// Set the provider field.
    pub fn with_provider(mut self, provider: String) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set the message_id field.
    pub fn with_message_id(mut self, message_id: String) -> Self {
        self.message_id = Some(message_id);
        self
    }

    /// Set the parent_message_id field.
    pub fn with_parent_message_id(mut self, parent_message_id: String) -> Self {
        self.parent_message_id = Some(parent_message_id);
        self
    }
}

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

// ============================================================================
// Tool Lifecycle Event Emission (§4)
// ============================================================================

/// Process an event and emit tool lifecycle events as appropriate.
///
/// Per spec §4 (Event Emission Ordering):
/// - Emit `tool_lifecycle` completion before `state_changed` to `PostToolsHook` or `CallingLlm`.
/// - Events fire for both start and completion.
///
/// This function wraps `process_event` and handles emission for tool-related events.
pub fn process_event_with_tool_emission<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    entry: &mut SessionEntry,
    event: &AgentEvent,
) -> Result<EventProcessingResult, InvalidTransition> {
    // Clone session_id to avoid borrow conflicts
    let session_id = entry.session.id.clone();
    let result = process_event(entry, event);

    match result {
        Ok(processed) => {
            // Emit lifecycle events for tools/hooks first (ordering requirement)
            match event {
                AgentEvent::ToolStarted { run_id, .. } | AgentEvent::ToolCompleted { run_id, .. } => {
                    if let Some(record) = find_tool_run(&entry.state.tool_runs, run_id) {
                        emit_tool_lifecycle(app, &session_id, record);
                    }
                }
                AgentEvent::HookStarted { run_id, .. }
                | AgentEvent::HookCompleted { run_id, .. } => {
                    if let Some(record) = find_hook_run(&entry.state.hook_runs, run_id) {
                        emit_hook_lifecycle(app, &session_id, record);
                    }
                }
                _ => {}
            }

            if processed.transition.new_kind == AgentStateKind::Error {
                if let Some(error) = &entry.state.last_error {
                    emit_session_error(app, &session_id, error);
                }
            }

            if let Some(reason) = processed.transition.reason {
                if processed.previous_kind != processed.transition.new_kind {
                    emit_state_changed(
                        app,
                        &session_id,
                        processed.previous_kind,
                        processed.transition.new_kind,
                        reason,
                        entry.state.active_stream_id.clone(),
                    );
                }
            }

            Ok(processed)
        }
        Err(err) => {
            let error = AgentError {
                code: "state_transition_invalid".to_string(),
                message: err.to_string(),
                retryable: false,
                source: ErrorSource::Orchestrator,
            };
            emit_session_error(app, &session_id, &error);
            Err(err)
        }
    }
}

/// Find a tool run record by run_id.
fn find_tool_run<'a>(tool_runs: &'a [ToolRunRecord], run_id: &str) -> Option<&'a ToolRunRecord> {
    tool_runs.iter().find(|r| r.run_id == run_id)
}

fn find_hook_run<'a>(hook_runs: &'a [HookRunRecord], run_id: &str) -> Option<&'a HookRunRecord> {
    hook_runs.iter().find(|r| r.run_id == run_id)
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
    use crate::agent_state::{AgentEvent, ToolRunRecord, ToolRunStatus};

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

    // ========================================================================
    // Tool Lifecycle Emission Tests (§4)
    // ========================================================================

    #[test]
    fn find_tool_run_finds_existing_record() {
        let records = vec![
            ToolRunRecord {
                run_id: "toolrun_1".to_string(),
                call_id: "call_1".to_string(),
                tool_name: "edit_file".to_string(),
                mutating: true,
                status: ToolRunStatus::Running,
                started_at_ms: 1000,
                finished_at_ms: None,
                attempt: 1,
                error: None,
            },
            ToolRunRecord {
                run_id: "toolrun_2".to_string(),
                call_id: "call_2".to_string(),
                tool_name: "read_file".to_string(),
                mutating: false,
                status: ToolRunStatus::Queued,
                started_at_ms: 0,
                finished_at_ms: None,
                attempt: 1,
                error: None,
            },
        ];

        let found = find_tool_run(&records, "toolrun_1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().tool_name, "edit_file");

        let found2 = find_tool_run(&records, "toolrun_2");
        assert!(found2.is_some());
        assert_eq!(found2.unwrap().tool_name, "read_file");
    }

    #[test]
    fn find_tool_run_returns_none_for_missing() {
        let records = vec![ToolRunRecord {
            run_id: "toolrun_1".to_string(),
            call_id: "call_1".to_string(),
            tool_name: "edit_file".to_string(),
            mutating: true,
            status: ToolRunStatus::Running,
            started_at_ms: 1000,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        }];

        let found = find_tool_run(&records, "toolrun_nonexistent");
        assert!(found.is_none());
    }

    #[test]
    fn process_event_updates_tool_run_status_on_started() {
        let session_id = "sess_test".to_string();
        let mut entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Running,
                agent_state: AgentStateKind::ExecutingTools,
            },
            state: AgentState {
                kind: AgentStateKind::ExecutingTools,
                tool_runs: vec![ToolRunRecord {
                    run_id: "toolrun_1".to_string(),
                    call_id: "call_1".to_string(),
                    tool_name: "edit_file".to_string(),
                    mutating: true,
                    status: ToolRunStatus::Queued,
                    started_at_ms: 0,
                    finished_at_ms: None,
                    attempt: 1,
                    error: None,
                }],
                ..Default::default()
            },
        };

        let event = AgentEvent::ToolStarted {
            session_id: session_id.clone(),
            run_id: "toolrun_1".to_string(),
        };

        let result = process_event(&mut entry, &event);
        assert!(result.is_ok());

        // Verify tool run status was updated to Running
        let tool_run = find_tool_run(&entry.state.tool_runs, "toolrun_1");
        assert!(tool_run.is_some());
        assert_eq!(tool_run.unwrap().status, ToolRunStatus::Running);
    }

    #[test]
    fn process_event_updates_tool_run_status_on_completed() {
        let session_id = "sess_test".to_string();
        let mut entry = SessionEntry {
            session: AgentSession {
                id: session_id.clone(),
                name: "test".to_string(),
                harness: AgentHarness::ClaudeCode,
                project_path: "/tmp/test".to_string(),
                status: SessionStatus::Running,
                agent_state: AgentStateKind::ExecutingTools,
            },
            state: AgentState {
                kind: AgentStateKind::ExecutingTools,
                tool_runs: vec![ToolRunRecord {
                    run_id: "toolrun_1".to_string(),
                    call_id: "call_1".to_string(),
                    tool_name: "read_file".to_string(),
                    mutating: false,
                    status: ToolRunStatus::Running,
                    started_at_ms: 1000,
                    finished_at_ms: None,
                    attempt: 1,
                    error: None,
                }],
                ..Default::default()
            },
        };

        let event = AgentEvent::ToolCompleted {
            session_id: session_id.clone(),
            run_id: "toolrun_1".to_string(),
            status: ToolRunStatus::Succeeded,
        };

        let result = process_event(&mut entry, &event);
        assert!(result.is_ok());

        // Verify tool run status was updated to Succeeded
        let tool_run = find_tool_run(&entry.state.tool_runs, "toolrun_1");
        assert!(tool_run.is_some());
        assert_eq!(tool_run.unwrap().status, ToolRunStatus::Succeeded);
    }

    // ========================================================================
    // StreamEvent Serialization Tests (§3)
    // ========================================================================

    #[test]
    fn stream_event_serializes_with_camel_case_fields() {
        let payload = serde_json::json!({ "text": "Hello", "role": "assistant" });
        let event = StreamEvent {
            schema_version: STREAM_SCHEMA_VERSION.to_string(),
            event_id: "evt_test123".to_string(),
            session_id: "sess_abc".to_string(),
            harness: "claude_code".to_string(),
            provider: Some("anthropic".to_string()),
            stream_id: "stream_xyz".to_string(),
            seq: 0,
            timestamp_ms: 1700000000000,
            event_type: StreamEventType::TextDelta,
            payload,
            message_id: None,
            parent_message_id: None,
        };

        let json = serde_json::to_string(&event).unwrap();

        // Verify camelCase field names per spec §3
        assert!(json.contains("\"schemaVersion\":\"1.0\""));
        assert!(json.contains("\"eventId\":\"evt_test123\""));
        assert!(json.contains("\"sessionId\":\"sess_abc\""));
        assert!(json.contains("\"streamId\":\"stream_xyz\""));
        assert!(json.contains("\"timestampMs\":1700000000000"));
        assert!(json.contains("\"type\":\"text_delta\""));
        assert!(json.contains("\"provider\":\"anthropic\""));

        // Optional fields should be absent when None
        assert!(!json.contains("messageId"));
        assert!(!json.contains("parentMessageId"));
    }

    #[test]
    fn stream_event_new_generates_event_id_and_timestamp() {
        let payload = serde_json::json!({ "text": "test", "role": "assistant" });
        let event = StreamEvent::new(
            "sess_test".to_string(),
            "claude_code".to_string(),
            "stream_1".to_string(),
            0,
            StreamEventType::TextDelta,
            payload,
        );

        assert!(event.event_id.starts_with("evt_"));
        assert!(event.timestamp_ms > 0);
        assert_eq!(event.schema_version, "1.0");
        assert_eq!(event.seq, 0);
    }

    #[test]
    fn stream_event_builder_methods_work() {
        let payload = serde_json::json!({ "text": "test", "role": "assistant" });
        let event = StreamEvent::new(
            "sess_test".to_string(),
            "claude_code".to_string(),
            "stream_1".to_string(),
            0,
            StreamEventType::TextDelta,
            payload,
        )
        .with_provider("anthropic".to_string())
        .with_message_id("msg_123".to_string())
        .with_parent_message_id("msg_000".to_string());

        assert_eq!(event.provider, Some("anthropic".to_string()));
        assert_eq!(event.message_id, Some("msg_123".to_string()));
        assert_eq!(event.parent_message_id, Some("msg_000".to_string()));
    }

    #[test]
    fn stream_event_type_serializes_as_snake_case() {
        // Per spec §3: Event types are snake_case strings
        assert_eq!(
            serde_json::to_string(&StreamEventType::TextDelta).unwrap(),
            "\"text_delta\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::ToolCallDelta).unwrap(),
            "\"tool_call_delta\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::ToolCallCompleted).unwrap(),
            "\"tool_call_completed\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::Status).unwrap(),
            "\"status\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::ThinkingDelta).unwrap(),
            "\"thinking_delta\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::ArtifactDelta).unwrap(),
            "\"artifact_delta\""
        );
        assert_eq!(
            serde_json::to_string(&StreamEventType::Metadata).unwrap(),
            "\"metadata\""
        );
    }

    #[test]
    fn text_delta_payload_serializes_correctly() {
        let payload = TextDeltaPayload {
            text: "Hello world".to_string(),
            role: "assistant".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(json, r#"{"text":"Hello world","role":"assistant"}"#);
    }

    #[test]
    fn tool_call_delta_payload_serializes_with_camel_case() {
        let payload = ToolCallDeltaPayload {
            call_id: "tool-1".to_string(),
            tool_name: "edit_file".to_string(),
            arguments_delta: "{\"path\":".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"callId\":\"tool-1\""));
        assert!(json.contains("\"toolName\":\"edit_file\""));
        assert!(json.contains("\"argumentsDelta\":\"{\\\"path\\\":\""));
    }

    #[test]
    fn completed_payload_serializes_with_usage() {
        let payload = CompletedPayload {
            reason: CompletionReason::Stop,
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                reasoning_tokens: Some(10),
            },
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"reason\":\"stop\""));
        assert!(json.contains("\"inputTokens\":100"));
        assert!(json.contains("\"outputTokens\":50"));
        assert!(json.contains("\"reasoningTokens\":10"));
    }

    #[test]
    fn error_payload_serializes_correctly() {
        let payload = ErrorPayload {
            code: StreamErrorCode::ProviderError,
            message: "Rate limit exceeded".to_string(),
            recoverable: true,
            details: Some(serde_json::json!({"retry_after": 60})),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"code\":\"provider_error\""));
        assert!(json.contains("\"message\":\"Rate limit exceeded\""));
        assert!(json.contains("\"recoverable\":true"));
        assert!(json.contains("\"details\":{\"retry_after\":60}"));
    }

    #[test]
    fn status_payload_serializes_correctly() {
        let payload = StatusPayload {
            state: AgentProcessingState::Processing,
            detail: Some("Generating response".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"state\":\"processing\""));
        assert!(json.contains("\"detail\":\"Generating response\""));
    }

    #[test]
    fn metadata_payload_serializes_correctly() {
        let payload = MetadataPayload {
            model: "claude-3-5".to_string(),
            latency_ms: 1234,
            provider_request_id: Some("req_abc123".to_string()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"model\":\"claude-3-5\""));
        assert!(json.contains("\"latencyMs\":1234"));
        assert!(json.contains("\"providerRequestId\":\"req_abc123\""));
    }

    #[test]
    fn stream_event_deserializes_correctly() {
        let json = r#"{
            "schemaVersion": "1.0",
            "eventId": "evt_123",
            "sessionId": "sess_abc",
            "harness": "claude_code",
            "streamId": "stream_1",
            "seq": 5,
            "timestampMs": 1700000000000,
            "type": "text_delta",
            "payload": {"text": "Hello", "role": "assistant"}
        }"#;

        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.schema_version, "1.0");
        assert_eq!(event.event_id, "evt_123");
        assert_eq!(event.session_id, "sess_abc");
        assert_eq!(event.harness, "claude_code");
        assert_eq!(event.stream_id, "stream_1");
        assert_eq!(event.seq, 5);
        assert_eq!(event.timestamp_ms, 1700000000000);
        assert_eq!(event.event_type, StreamEventType::TextDelta);
        assert!(event.provider.is_none());
        assert!(event.message_id.is_none());
    }
}

// Future commands:
// - spawn_session(harness: AgentHarness, project_path: String)
// - attach_session(session_id: String)
// - stop_session(session_id: String)
// - get_session_output(session_id: String)
