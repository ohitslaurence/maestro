use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request from client
#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC success response
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub id: u64,
    pub result: Value,
}

/// JSON-RPC error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub id: u64,
    pub error: RpcError,
}

/// Error details
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: &'static str,
    pub message: String,
}

/// Server→Client event (no id)
#[derive(Debug, Serialize)]
pub struct Event {
    pub method: &'static str,
    pub params: Value,
}

// Error codes
pub const AUTH_REQUIRED: &str = "auth_required";
pub const AUTH_FAILED: &str = "auth_failed";
pub const INVALID_PARAMS: &str = "invalid_params";
pub const SESSION_NOT_FOUND: &str = "session_not_found";
pub const TERMINAL_NOT_FOUND: &str = "terminal_not_found";
pub const TERMINAL_EXISTS: &str = "terminal_exists";
pub const GIT_ERROR: &str = "git_error";
pub const INTERNAL_ERROR: &str = "internal_error";

// Method names
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

// Event names
pub const EVENT_TERMINAL_OUTPUT: &str = "terminal_output";
pub const EVENT_TERMINAL_EXITED: &str = "terminal_exited";

// --- Request params ---

#[derive(Debug, Deserialize)]
pub struct AuthParams {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionIdParams {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalOpenParams {
    pub session_id: String,
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize)]
pub struct TerminalWriteParams {
    pub session_id: String,
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Deserialize)]
pub struct TerminalResizeParams {
    pub session_id: String,
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize)]
pub struct TerminalCloseParams {
    pub session_id: String,
    pub terminal_id: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLogParams {
    pub session_id: String,
    pub limit: Option<u32>,
}

// --- Response types ---

#[derive(Debug, Serialize)]
pub struct AuthResult {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct SessionInfoResult {
    pub path: String,
    pub name: String,
    pub has_git: bool,
}

#[derive(Debug, Serialize)]
pub struct TerminalOpenResult {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitFileStatus {
    pub path: String,
    pub status: String,
    pub additions: i32,
    pub deletions: i32,
}

#[derive(Debug, Serialize)]
pub struct GitStatusResult {
    pub branch_name: String,
    pub staged_files: Vec<GitFileStatus>,
    pub unstaged_files: Vec<GitFileStatus>,
    pub total_additions: i32,
    pub total_deletions: i32,
}

#[derive(Debug, Serialize)]
pub struct GitFileDiff {
    pub path: String,
    pub diff: String,
}

#[derive(Debug, Serialize)]
pub struct GitDiffResult {
    pub files: Vec<GitFileDiff>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub truncated_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GitLogEntry {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct GitLogResult {
    pub entries: Vec<GitLogEntry>,
    pub ahead: i32,
    pub behind: i32,
    pub upstream: Option<String>,
}

// --- Event params ---

#[derive(Debug, Serialize)]
pub struct TerminalOutputParams {
    pub session_id: String,
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Serialize)]
pub struct TerminalExitedParams {
    pub session_id: String,
    pub terminal_id: String,
    pub exit_code: Option<i32>,
}

// --- Helpers ---

impl SuccessResponse {
    pub fn new<T: Serialize>(id: u64, result: T) -> Self {
        Self {
            id,
            result: serde_json::to_value(result).unwrap_or(Value::Null),
        }
    }
}

impl ErrorResponse {
    pub fn new(id: u64, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            error: RpcError {
                code,
                message: message.into(),
            },
        }
    }
}

impl Event {
    pub fn new<T: Serialize>(method: &'static str, params: T) -> Self {
        Self {
            method,
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        }
    }
}
