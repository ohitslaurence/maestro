use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- JSON-RPC types ---

#[derive(Debug, Serialize)]
pub struct Request {
    pub id: u64,
    pub method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success { id: u64, result: Value },
    Error { id: u64, error: RpcError },
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

/// Server→Client event (no id field)
#[derive(Debug, Deserialize)]
pub struct Event {
    pub method: String,
    pub params: Value,
}

/// Raw incoming message - could be response or event
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    Response(Response),
    Event(Event),
}

// --- Method names ---

pub const METHOD_AUTH: &str = "auth";
pub const METHOD_LIST_SESSIONS: &str = "list_sessions";
pub const METHOD_SESSION_INFO: &str = "session_info";
pub const METHOD_TERMINAL_OPEN: &str = "terminal_open";
pub const METHOD_TERMINAL_WRITE: &str = "terminal_write";
pub const METHOD_TERMINAL_RESIZE: &str = "terminal_resize";
pub const METHOD_TERMINAL_CLOSE: &str = "terminal_close";
pub const METHOD_GIT_STATUS: &str = "git_status";
pub const METHOD_GIT_DIFF: &str = "git_diff";
pub const METHOD_GIT_LOG: &str = "git_log";

// --- Request params ---

#[derive(Debug, Serialize)]
pub struct AuthParams {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct SessionIdParams {
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalOpenParams {
    pub session_id: String,
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Serialize)]
pub struct TerminalWriteParams {
    pub session_id: String,
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalResizeParams {
    pub session_id: String,
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Serialize)]
pub struct TerminalCloseParams {
    pub session_id: String,
    pub terminal_id: String,
}

#[derive(Debug, Serialize)]
pub struct GitLogParams {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

// --- Response types ---

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionInfoResult {
    pub path: String,
    pub name: String,
    pub has_git: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TerminalOpenResult {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    pub path: String,
    pub status: String,
    pub additions: i32,
    pub deletions: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResult {
    pub branch_name: String,
    pub staged_files: Vec<GitFileStatus>,
    pub unstaged_files: Vec<GitFileStatus>,
    pub total_additions: i32,
    pub total_deletions: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitFileDiff {
    pub path: String,
    pub diff: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitDiffResult {
    pub files: Vec<GitFileDiff>,
    pub truncated: bool,
    #[serde(default)]
    pub truncated_files: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitLogEntry {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitLogResult {
    pub entries: Vec<GitLogEntry>,
    pub ahead: i32,
    pub behind: i32,
    pub upstream: Option<String>,
}

// --- Event params (for forwarding to frontend) ---

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminalOutputParams {
    pub session_id: String,
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminalExitedParams {
    pub session_id: String,
    pub terminal_id: String,
    pub exit_code: Option<i32>,
}

// Event method names
pub const EVENT_TERMINAL_OUTPUT: &str = "terminal_output";
pub const EVENT_TERMINAL_EXITED: &str = "terminal_exited";
