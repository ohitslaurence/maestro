use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use super::client::DaemonState;
use super::config::DaemonConfig;
use super::protocol::*;

// --- Connection Management Commands ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub connected: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    pub connected: bool,
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[tauri::command]
pub async fn daemon_connect(state: State<'_, Arc<DaemonState>>) -> Result<ConnectResult, String> {
    state.connect().await?;
    Ok(ConnectResult { connected: true })
}

#[tauri::command]
pub async fn daemon_disconnect(state: State<'_, Arc<DaemonState>>) -> Result<(), String> {
    state.disconnect().await;
    Ok(())
}

#[tauri::command]
pub async fn daemon_status(state: State<'_, Arc<DaemonState>>) -> Result<StatusResult, String> {
    let connected = state.is_connected().await;
    let config = state.get_config().await;

    Ok(StatusResult {
        connected,
        host: config.as_ref().map(|c| c.host.clone()),
        port: config.as_ref().map(|c| c.port),
    })
}

#[tauri::command]
pub async fn daemon_configure(
    host: String,
    port: u16,
    token: String,
    state: State<'_, Arc<DaemonState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Disconnect if connected
    state.disconnect().await;

    // Save new config
    let config = DaemonConfig { host, port, token };
    config.save(&app).await?;
    state.set_config(Some(config)).await;

    Ok(())
}

// --- Session Commands (Proxy to Daemon) ---

#[tauri::command]
pub async fn list_sessions(state: State<'_, Arc<DaemonState>>) -> Result<Vec<SessionInfo>, String> {
    state
        .call::<(), Vec<SessionInfo>>(METHOD_LIST_SESSIONS, None)
        .await
}

#[tauri::command]
pub async fn session_info(
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<SessionInfoResult, String> {
    state
        .call(METHOD_SESSION_INFO, Some(SessionIdParams { session_id }))
        .await
}

// --- Terminal Commands (Proxy to Daemon) ---

#[tauri::command]
pub async fn terminal_open(
    session_id: String,
    terminal_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<DaemonState>>,
) -> Result<TerminalOpenResult, String> {
    state
        .call(
            METHOD_TERMINAL_OPEN,
            Some(TerminalOpenParams {
                session_id,
                terminal_id,
                cols,
                rows,
            }),
        )
        .await
}

#[tauri::command]
pub async fn terminal_write(
    session_id: String,
    terminal_id: String,
    data: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<(), String> {
    state
        .call::<_, Value>(
            METHOD_TERMINAL_WRITE,
            Some(TerminalWriteParams {
                session_id,
                terminal_id,
                data,
            }),
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn terminal_resize(
    session_id: String,
    terminal_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<DaemonState>>,
) -> Result<(), String> {
    state
        .call::<_, Value>(
            METHOD_TERMINAL_RESIZE,
            Some(TerminalResizeParams {
                session_id,
                terminal_id,
                cols,
                rows,
            }),
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn terminal_close(
    session_id: String,
    terminal_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<(), String> {
    state
        .call::<_, Value>(
            METHOD_TERMINAL_CLOSE,
            Some(TerminalCloseParams {
                session_id,
                terminal_id,
            }),
        )
        .await?;
    Ok(())
}

// --- Git Commands (Proxy to Daemon) ---

#[tauri::command]
pub async fn git_status(
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<GitStatusResult, String> {
    state
        .call(METHOD_GIT_STATUS, Some(SessionIdParams { session_id }))
        .await
}

#[tauri::command]
pub async fn git_diff(
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<GitDiffResult, String> {
    state
        .call(METHOD_GIT_DIFF, Some(SessionIdParams { session_id }))
        .await
}

#[tauri::command]
pub async fn git_log(
    session_id: String,
    limit: Option<u32>,
    state: State<'_, Arc<DaemonState>>,
) -> Result<GitLogResult, String> {
    state
        .call(METHOD_GIT_LOG, Some(GitLogParams { session_id, limit }))
        .await
}

// --- OpenCode Commands (Proxy to Daemon) ---

#[tauri::command]
pub async fn opencode_connect_workspace(
    workspace_id: String,
    workspace_path: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<OpenCodeConnectResult, String> {
    state
        .call(
            METHOD_OPENCODE_CONNECT_WORKSPACE,
            Some(OpenCodeConnectParams {
                workspace_id,
                workspace_path,
            }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_disconnect_workspace(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_DISCONNECT_WORKSPACE,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_status(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<OpenCodeStatusResult, String> {
    state
        .call(
            METHOD_OPENCODE_STATUS,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_session_list(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_SESSION_LIST,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_session_create(
    workspace_id: String,
    title: Option<String>,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_SESSION_CREATE,
            Some(OpenCodeSessionCreateParams { workspace_id, title }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_session_prompt(
    workspace_id: String,
    session_id: String,
    message: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_SESSION_PROMPT,
            Some(OpenCodeSessionPromptParams {
                workspace_id,
                session_id,
                message,
            }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_session_abort(
    workspace_id: String,
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_SESSION_ABORT,
            Some(OpenCodeSessionAbortParams {
                workspace_id,
                session_id,
            }),
        )
        .await
}

#[tauri::command]
pub async fn opencode_session_messages(
    workspace_id: String,
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_OPENCODE_SESSION_MESSAGES,
            Some(OpenCodeSessionMessagesParams {
                workspace_id,
                session_id,
            }),
        )
        .await
}

// --- Claude SDK Commands (Proxy to Daemon) ---
// Per claude-sdk-server spec §4, mirrors OpenCode API shape

#[tauri::command]
pub async fn claude_sdk_connect_workspace(
    workspace_id: String,
    workspace_path: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<OpenCodeConnectResult, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_CONNECT_WORKSPACE,
            Some(OpenCodeConnectParams {
                workspace_id,
                workspace_path,
            }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_disconnect_workspace(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_DISCONNECT_WORKSPACE,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_status(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<OpenCodeStatusResult, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_STATUS,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_session_list(
    workspace_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_SESSION_LIST,
            Some(OpenCodeWorkspaceParams { workspace_id }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_session_create(
    workspace_id: String,
    title: Option<String>,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_SESSION_CREATE,
            Some(OpenCodeSessionCreateParams { workspace_id, title }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_session_prompt(
    workspace_id: String,
    session_id: String,
    message: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_SESSION_PROMPT,
            Some(OpenCodeSessionPromptParams {
                workspace_id,
                session_id,
                message,
            }),
        )
        .await
}

#[tauri::command]
pub async fn claude_sdk_session_abort(
    workspace_id: String,
    session_id: String,
    state: State<'_, Arc<DaemonState>>,
) -> Result<Value, String> {
    state
        .call(
            METHOD_CLAUDE_SDK_SESSION_ABORT,
            Some(OpenCodeSessionAbortParams {
                workspace_id,
                session_id,
            }),
        )
        .await
}

// Helper to use serde_json::Value without importing in this file
use serde_json::Value;
