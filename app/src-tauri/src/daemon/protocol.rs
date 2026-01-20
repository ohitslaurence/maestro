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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success { id: u64, result: Value },
    Error { id: u64, error: RpcError },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

/// Server→Client event (no id field)
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Event {
    pub method: String,
    pub params: Value,
}

/// Raw incoming message - could be response or event
#[allow(dead_code)]
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

// OpenCode method names
pub const METHOD_OPENCODE_CONNECT_WORKSPACE: &str = "opencode_connect_workspace";
pub const METHOD_OPENCODE_DISCONNECT_WORKSPACE: &str = "opencode_disconnect_workspace";
pub const METHOD_OPENCODE_STATUS: &str = "opencode_status";
pub const METHOD_OPENCODE_SESSION_LIST: &str = "opencode_session_list";
pub const METHOD_OPENCODE_SESSION_CREATE: &str = "opencode_session_create";
pub const METHOD_OPENCODE_SESSION_PROMPT: &str = "opencode_session_prompt";
pub const METHOD_OPENCODE_SESSION_ABORT: &str = "opencode_session_abort";

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

// --- OpenCode request params ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeConnectParams {
    pub workspace_id: String,
    pub workspace_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeWorkspaceParams {
    pub workspace_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeSessionCreateParams {
    pub workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeSessionPromptParams {
    pub workspace_id: String,
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeSessionAbortParams {
    pub workspace_id: String,
    pub session_id: String,
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
    #[serde(default, alias = "branch_name")]
    pub branch_name: String,
    #[serde(default, alias = "staged_files")]
    pub staged_files: Vec<GitFileStatus>,
    #[serde(default, alias = "unstaged_files")]
    pub unstaged_files: Vec<GitFileStatus>,
    #[serde(default, alias = "total_additions")]
    pub total_additions: i32,
    #[serde(default, alias = "total_deletions")]
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

// --- OpenCode response types ---

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeConnectResult {
    pub workspace_id: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeStatusResult {
    pub connected: bool,
    pub base_url: Option<String>,
}

// --- Event params (for forwarding to frontend) ---

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminalOutputParams {
    pub session_id: String,
    pub terminal_id: String,
    pub data: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminalExitedParams {
    pub session_id: String,
    pub terminal_id: String,
    pub exit_code: Option<i32>,
}

// Event method names
pub const EVENT_TERMINAL_OUTPUT: &str = "terminal_output";
pub const EVENT_TERMINAL_EXITED: &str = "terminal_exited";
pub const EVENT_OPENCODE: &str = "opencode:event";

#[cfg(test)]
mod tests {
    use super::{
        GitDiffResult, IncomingMessage, Request, Response, METHOD_GIT_STATUS, EVENT_TERMINAL_OUTPUT,
    };
    use serde_json::json;

    #[test]
    fn request_omits_params_when_none() {
        let request = Request {
            id: 1,
            method: METHOD_GIT_STATUS,
            params: None,
        };

        let value = serde_json::to_value(request).expect("request to serialize");
        assert_eq!(value.get("id"), Some(&json!(1)));
        assert_eq!(value.get("method"), Some(&json!(METHOD_GIT_STATUS)));
        assert!(value.get("params").is_none());
    }

    #[test]
    fn response_parses_success_and_error() {
        let success: Response = serde_json::from_str(r#"{"id":1,"result":{"ok":true}}"#)
            .expect("success response to parse");
        match success {
            Response::Success { id, result } => {
                assert_eq!(id, 1);
                assert_eq!(result.get("ok"), Some(&json!(true)));
            }
            _ => panic!("expected success response"),
        }

        let error: Response = serde_json::from_str(
            r#"{"id":2,"error":{"code":"auth_failed","message":"nope"}}"#,
        )
        .expect("error response to parse");
        match error {
            Response::Error { id, error } => {
                assert_eq!(id, 2);
                assert_eq!(error.code, "auth_failed");
                assert_eq!(error.message, "nope");
            }
            _ => panic!("expected error response"),
        }
    }

    #[test]
    fn incoming_message_parses_event() {
        let event: IncomingMessage = serde_json::from_str(
            r#"{"method":"terminal_output","params":{"session_id":"s","terminal_id":"t","data":"hi"}}"#,
        )
        .expect("event to parse");

        match event {
            IncomingMessage::Event(parsed) => {
                assert_eq!(parsed.method, EVENT_TERMINAL_OUTPUT);
                assert_eq!(parsed.params.get("data"), Some(&json!("hi")));
            }
            _ => panic!("expected event message"),
        }
    }

    #[test]
    fn git_diff_result_defaults_truncated_files() {
        let result: GitDiffResult =
            serde_json::from_str(r#"{"files":[],"truncated":false}"#)
                .expect("diff result to parse");
        assert!(result.truncated_files.is_empty());
    }
}
