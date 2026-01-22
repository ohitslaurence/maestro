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
pub const OPENCODE_ERROR: &str = "opencode_error";
pub const OPENCODE_NOT_CONNECTED: &str = "opencode_not_connected";
pub const CLAUDE_SDK_ERROR: &str = "claude_sdk_error";
pub const CLAUDE_SDK_NOT_CONNECTED: &str = "claude_sdk_not_connected";

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

// OpenCode method names
pub const METHOD_OPENCODE_CONNECT_WORKSPACE: &str = "opencode_connect_workspace";
pub const METHOD_OPENCODE_DISCONNECT_WORKSPACE: &str = "opencode_disconnect_workspace";
pub const METHOD_OPENCODE_STATUS: &str = "opencode_status";
pub const METHOD_OPENCODE_SESSION_LIST: &str = "opencode_session_list";
pub const METHOD_OPENCODE_SESSION_CREATE: &str = "opencode_session_create";
pub const METHOD_OPENCODE_SESSION_PROMPT: &str = "opencode_session_prompt";
pub const METHOD_OPENCODE_SESSION_ABORT: &str = "opencode_session_abort";
pub const METHOD_OPENCODE_SESSION_MESSAGES: &str = "opencode_session_messages";

// Claude SDK method names
pub const METHOD_CLAUDE_SDK_CONNECT_WORKSPACE: &str = "claude_sdk_connect_workspace";
pub const METHOD_CLAUDE_SDK_DISCONNECT_WORKSPACE: &str = "claude_sdk_disconnect_workspace";
pub const METHOD_CLAUDE_SDK_STATUS: &str = "claude_sdk_status";
pub const METHOD_CLAUDE_SDK_SESSION_LIST: &str = "claude_sdk_session_list";
pub const METHOD_CLAUDE_SDK_SESSION_CREATE: &str = "claude_sdk_session_create";
pub const METHOD_CLAUDE_SDK_SESSION_PROMPT: &str = "claude_sdk_session_prompt";
pub const METHOD_CLAUDE_SDK_SESSION_ABORT: &str = "claude_sdk_session_abort";
pub const METHOD_CLAUDE_SDK_SESSION_MESSAGES: &str = "claude_sdk_session_messages";
pub const METHOD_CLAUDE_SDK_MODELS: &str = "claude_sdk_models";
// Claude SDK permission methods (dynamic-tool-approvals spec §4)
pub const METHOD_CLAUDE_SDK_PERMISSION_REPLY: &str = "claude_sdk_permission_reply";
pub const METHOD_CLAUDE_SDK_PERMISSION_PENDING: &str = "claude_sdk_permission_pending";
// Claude SDK session settings methods (session-settings spec §4)
pub const METHOD_CLAUDE_SDK_SESSION_SETTINGS_UPDATE: &str = "claude_sdk_session_settings_update";

// Event names
pub const EVENT_TERMINAL_OUTPUT: &str = "terminal_output";
pub const EVENT_TERMINAL_EXITED: &str = "terminal_exited";
#[allow(dead_code)]
pub const EVENT_OPENCODE: &str = "opencode:event";

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

// --- OpenCode request params ---

#[derive(Debug, Deserialize)]
pub struct OpenCodeConnectParams {
    pub workspace_id: String,
    pub workspace_path: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeWorkspaceParams {
    pub workspace_id: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeSessionCreateParams {
    pub workspace_id: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeSessionPromptParams {
    pub workspace_id: String,
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeSessionAbortParams {
    pub workspace_id: String,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenCodeSessionMessagesParams {
    pub workspace_id: String,
    pub session_id: String,
}

// --- Claude SDK session messages params (claude-session-history spec §4) ---

#[derive(Debug, Deserialize)]
pub struct ClaudeSdkSessionMessagesParams {
    pub workspace_id: String,
    pub session_id: String,
    /// Optional limit on messages returned (default 100, max 500)
    #[serde(default)]
    pub limit: Option<u32>,
}

// --- Claude SDK request params (composer-options spec §4) ---

#[derive(Debug, Deserialize)]
pub struct ClaudeSdkSessionPromptParams {
    pub workspace_id: String,
    pub session_id: String,
    pub message: String,
    /// Per-message thinking budget override (composer-options spec §3, §4)
    #[serde(default)]
    pub max_thinking_tokens: Option<u32>,
}

// --- Claude SDK permission params (dynamic-tool-approvals spec §4) ---

/// Reply to a pending permission request
#[derive(Debug, Deserialize)]
pub struct ClaudeSdkPermissionReplyParams {
    pub workspace_id: String,
    pub request_id: String,
    /// Reply type: "once" (allow once), "always" (always allow), "reject" (deny)
    pub reply: String,
    /// Optional message for deny feedback
    #[serde(default)]
    pub message: Option<String>,
}

/// Get pending permission requests
#[derive(Debug, Deserialize)]
pub struct ClaudeSdkPermissionPendingParams {
    pub workspace_id: String,
    /// Optional session_id filter
    #[serde(default)]
    pub session_id: Option<String>,
}

// --- Claude SDK session settings params (session-settings spec §4) ---

/// Update session settings
#[derive(Debug, Deserialize)]
pub struct ClaudeSdkSessionSettingsUpdateParams {
    pub workspace_id: String,
    pub session_id: String,
    /// Partial settings to merge (session-settings spec §4.1)
    pub settings: Value,
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

// --- OpenCode response types ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeConnectResult {
    pub workspace_id: String,
    pub base_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeStatusResult {
    pub connected: bool,
    pub base_url: Option<String>,
}

/// Claude SDK server status for restart resilience (spec §4)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClaudeSdkServerStatus {
    /// Process spawned, awaiting health check
    Starting,
    /// Health check passed
    Ready,
    /// Restart threshold exceeded
    Error { message: String },
}

/// Response for claude_sdk_status (spec §4)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSdkStatusResult {
    pub connected: bool,
    pub base_url: Option<String>,
    pub status: Option<ClaudeSdkServerStatus>,
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

#[cfg(test)]
mod tests {
    use super::{
        ErrorResponse, Event, GitDiffResult, Request, RpcError, SuccessResponse, AUTH_FAILED,
        EVENT_TERMINAL_OUTPUT,
    };
    use serde_json::json;

    #[test]
    fn request_defaults_params_to_null() {
        let request: Request = serde_json::from_str(r#"{"id":1,"method":"auth"}"#)
            .expect("request to parse");
        assert_eq!(request.id, 1);
        assert_eq!(request.method, "auth");
        assert_eq!(request.params, json!(null));
    }

    #[test]
    fn success_response_serializes_result() {
        let response = SuccessResponse::new(2, json!({"ok": true}));
        let value = serde_json::to_value(response).expect("response to serialize");
        assert_eq!(value.get("id"), Some(&json!(2)));
        assert_eq!(value.get("result"), Some(&json!({"ok": true})));
    }

    #[test]
    fn error_response_serializes_error() {
        let response = ErrorResponse::new(3, AUTH_FAILED, "nope");
        let value = serde_json::to_value(response).expect("error to serialize");
        assert_eq!(value.get("id"), Some(&json!(3)));
        let error = value.get("error").expect("error field");
        assert_eq!(error.get("code"), Some(&json!(AUTH_FAILED)));
        assert_eq!(error.get("message"), Some(&json!("nope")));
    }

    #[test]
    fn event_serializes_params() {
        let event = Event::new(EVENT_TERMINAL_OUTPUT, json!({"data": "hi"}));
        let value = serde_json::to_value(event).expect("event to serialize");
        assert_eq!(value.get("method"), Some(&json!(EVENT_TERMINAL_OUTPUT)));
        assert_eq!(value.get("params"), Some(&json!({"data": "hi"})));
    }

    #[test]
    fn git_diff_result_omits_empty_truncated_files() {
        let result = GitDiffResult {
            files: vec![],
            truncated: false,
            truncated_files: vec![],
        };
        let value = serde_json::to_value(result).expect("diff result to serialize");
        assert!(value.get("truncated_files").is_none());
    }

    #[test]
    fn rpc_error_serializes_fields() {
        let error = RpcError {
            code: AUTH_FAILED,
            message: "nope".to_string(),
        };
        let value = serde_json::to_value(error).expect("rpc error to serialize");
        assert_eq!(value.get("code"), Some(&json!(AUTH_FAILED)));
        assert_eq!(value.get("message"), Some(&json!("nope")));
    }
}
