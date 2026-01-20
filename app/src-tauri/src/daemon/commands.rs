use std::sync::Arc;

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoArgs {
    pub session_id: String,
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

// Helper to use serde_json::Value without importing in this file
use serde_json::Value;
