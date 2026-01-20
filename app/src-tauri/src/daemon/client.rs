use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time::timeout;

use super::config::DaemonConfig;
use super::protocol::*;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const RECONNECT_DELAYS: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

type PendingRequests = HashMap<u64, oneshot::Sender<Result<Value, String>>>;

/// Client connection to the remote daemon
pub struct DaemonClient {
    writer: Mutex<BufWriter<OwnedWriteHalf>>,
    pending: Arc<Mutex<PendingRequests>>,
    next_id: AtomicU64,
    connected: Arc<RwLock<bool>>,
}

/// Shared daemon state for Tauri
pub struct DaemonState {
    pub client: RwLock<Option<Arc<DaemonClient>>>,
    pub config: RwLock<Option<DaemonConfig>>,
    pub app_handle: Mutex<Option<AppHandle>>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            client: RwLock::new(None),
            config: RwLock::new(None),
            app_handle: Mutex::new(None),
        }
    }

    pub async fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    pub async fn emit_debug(&self, message: &str, data: Option<Value>) {
        let app_handle = self.app_handle.lock().await.clone();
        emit_debug(app_handle.as_ref(), message, data);
    }

    pub async fn is_connected(&self) -> bool {
        let client = self.client.read().await;
        if let Some(c) = client.as_ref() {
            *c.connected.read().await
        } else {
            false
        }
    }

    pub async fn get_config(&self) -> Option<DaemonConfig> {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: Option<DaemonConfig>) {
        *self.config.write().await = config;
    }

    /// Connect to daemon using stored config
    pub async fn connect(&self) -> Result<(), String> {
        let config = self.config.read().await.clone();
        let config = config.ok_or("daemon_not_configured")?;

        let app_handle = self.app_handle.lock().await.clone();
        let app_handle = app_handle.ok_or("App handle not set")?;

        emit_debug(
            Some(&app_handle),
            "connect:start",
            Some(json!({
                "host": config.host,
                "port": config.port
            })),
        );

        // Disconnect existing client if any
        self.disconnect().await;

        let client = match DaemonClient::connect(&config, app_handle.clone()).await {
            Ok(client) => {
                emit_debug(Some(&app_handle), "connect:success", None);
                client
            }
            Err(error) => {
                emit_debug(
                    Some(&app_handle),
                    "connect:error",
                    Some(json!({ "error": error })),
                );
                return Err(error);
            }
        };
        let client = Arc::new(client);

        *self.client.write().await = Some(client);

        // Emit connected event
        let _ = app_handle.emit("daemon:connected", serde_json::json!({"connected": true}));

        Ok(())
    }

    /// Disconnect from daemon
    pub async fn disconnect(&self) {
        let mut client = self.client.write().await;
        if let Some(c) = client.take() {
            *c.connected.write().await = false;
        }
        if let Some(app) = self.app_handle.lock().await.as_ref() {
            emit_debug(Some(app), "disconnect:requested", None);
            let _ = app.emit("daemon:disconnected", serde_json::json!({}));
        }
    }

    /// Call a daemon method
    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &'static str,
        params: Option<P>,
    ) -> Result<R, String> {
        let client = self.client.read().await;
        let client = client.as_ref().ok_or("daemon_disconnected")?;
        client.call(method, params).await
    }
}

impl DaemonClient {
    /// Connect to the daemon and authenticate
    pub async fn connect(config: &DaemonConfig, app_handle: AppHandle) -> Result<Self, String> {
        Self::connect_inner(config, Some(app_handle)).await
    }

    #[cfg(test)]
    pub async fn connect_without_app(config: &DaemonConfig) -> Result<Self, String> {
        Self::connect_inner(config, None).await
    }

    async fn connect_inner(
        config: &DaemonConfig,
        app_handle: Option<AppHandle>,
    ) -> Result<Self, String> {
        let addr = format!("{}:{}", config.host, config.port);

        let stream = timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr))
            .await
            .map_err(|_| "daemon_connection_failed: timeout")?
            .map_err(|e| format!("daemon_connection_failed: {e}"))?;

        let (reader, writer) = stream.into_split();
        let reader = BufReader::new(reader);
        let writer = Mutex::new(BufWriter::new(writer));
        let pending: Arc<Mutex<PendingRequests>> = Arc::new(Mutex::new(HashMap::new()));
        let connected = Arc::new(RwLock::new(true));

        let client = Self {
            writer,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            connected: connected.clone(),
        };

        // Start reader task
        Self::spawn_reader(reader, pending, connected.clone(), app_handle);

        // Authenticate
        let auth_result: Value = client
            .call(METHOD_AUTH, Some(AuthParams { token: config.token.clone() }))
            .await?;

        let ok = auth_result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            *client.connected.write().await = false;
            return Err("daemon_auth_failed".to_string());
        }

        Ok(client)
    }

    fn spawn_reader(
        mut reader: BufReader<OwnedReadHalf>,
        pending: Arc<Mutex<PendingRequests>>,
        connected: Arc<RwLock<bool>>,
        app_handle: Option<AppHandle>,
    ) {
        tokio::spawn(async move {
            let mut line = String::new();
            let handle = app_handle.as_ref();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        emit_debug(handle, "connection:closed", None);
                        // EOF - connection closed
                        break;
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        // Try to parse as response (has "id") or event (no "id")
                        if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                            if parsed.get("id").is_some() {
                                // Response
                                Self::handle_response(&pending, &parsed).await;
                            } else if parsed.get("method").is_some() {
                                // Event
                                Self::handle_event(app_handle.as_ref(), &parsed);
                            }
                        } else {
                            let snippet = if trimmed.len() > 200 {
                                format!("{}...", &trimmed[..200])
                            } else {
                                trimmed.to_string()
                            };
                            emit_debug(
                                handle,
                                "protocol:invalid_json",
                                Some(json!({ "snippet": snippet })),
                            );
                        }
                    }
                    Err(error) => {
                        emit_debug(
                            handle,
                            "connection:error",
                            Some(json!({ "error": error.to_string() })),
                        );
                        break;
                    }
                }
            }

            // Mark as disconnected
            *connected.write().await = false;
            if let Some(handle) = app_handle.as_ref() {
                let _ = handle.emit(
                    "daemon:disconnected",
                    serde_json::json!({"reason": "connection_lost"}),
                );
            }

            // Fail all pending requests
            let mut pending = pending.lock().await;
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err("daemon_disconnected".to_string()));
            }
        });
    }

    async fn handle_response(pending: &Mutex<PendingRequests>, parsed: &Value) {
        let id = match parsed.get("id").and_then(|v| v.as_u64()) {
            Some(id) => id,
            None => return,
        };

        let mut pending = pending.lock().await;
        if let Some(sender) = pending.remove(&id) {
            let result = if let Some(result) = parsed.get("result") {
                Ok(result.clone())
            } else if let Some(error) = parsed.get("error") {
                let code = error.get("code").and_then(|v| v.as_str()).unwrap_or("unknown");
                let msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
                Err(format!("{code}: {msg}"))
            } else {
                Err("Invalid response".to_string())
            };
            let _ = sender.send(result);
        }
    }

    fn handle_event(app_handle: Option<&AppHandle>, parsed: &Value) {
        let Some(handle) = app_handle else {
            return;
        };
        let method = match parsed.get("method").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => return,
        };

        let params = parsed.get("params").cloned().unwrap_or(Value::Null);

        match method {
            EVENT_TERMINAL_OUTPUT => {
                let _ = handle.emit("daemon:terminal_output", params);
            }
            EVENT_TERMINAL_EXITED => {
                let _ = handle.emit("daemon:terminal_exited", params);
            }
            _ => {}
        }
    }

    /// Send a JSON-RPC request and wait for response
    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &'static str,
        params: Option<P>,
    ) -> Result<R, String> {
        if !*self.connected.read().await {
            return Err("daemon_disconnected".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = Request {
            id,
            method,
            params: params.map(|p| serde_json::to_value(p).unwrap()),
        };

        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Send request
        {
            let mut writer = self.writer.lock().await;
            let json = serde_json::to_string(&request).map_err(|e| format!("Serialize error: {e}"))?;
            writer
                .write_all(json.as_bytes())
                .await
                .map_err(|e| format!("Write error: {e}"))?;
            writer
                .write_all(b"\n")
                .await
                .map_err(|e| format!("Write error: {e}"))?;
            writer
                .flush()
                .await
                .map_err(|e| format!("Flush error: {e}"))?;
        }

        // Wait for response with timeout
        let result = timeout(REQUEST_TIMEOUT, rx)
            .await
            .map_err(|_| "Request timeout")?
            .map_err(|_| "Request cancelled")?;

        let value = result?;
        serde_json::from_value(value).map_err(|e| format!("Deserialize error: {e}"))
    }
}

/// Spawn a background task to handle auto-reconnection
pub fn spawn_reconnect_task(state: Arc<DaemonState>) {
    tokio::spawn(async move {
        let mut attempt = 0usize;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Check if connected
            if state.is_connected().await {
                attempt = 0;
                continue;
            }

            // Check if configured
            if state.get_config().await.is_none() {
                continue;
            }

            // Attempt reconnect
            let delay = RECONNECT_DELAYS
                .get(attempt)
                .copied()
                .unwrap_or(RECONNECT_DELAYS[RECONNECT_DELAYS.len() - 1]);
            state
                .emit_debug(
                    "reconnect:scheduled",
                    Some(json!({
                        "attempt": attempt + 1,
                        "delay_ms": delay.as_millis()
                    })),
                )
                .await;
            tokio::time::sleep(delay).await;

            if let Err(error) = state.connect().await {
                state
                    .emit_debug("reconnect:failed", Some(json!({ "error": error })))
                    .await;
                attempt = (attempt + 1).min(RECONNECT_DELAYS.len() - 1);
            } else {
                state.emit_debug("reconnect:success", None).await;
                attempt = 0;
            }
        }
    });
}

fn emit_debug(app_handle: Option<&AppHandle>, message: &str, data: Option<Value>) {
    let Some(handle) = app_handle else {
        return;
    };
    let payload = match data {
        Some(data) => json!({ "message": message, "data": data }),
        None => json!({ "message": message }),
    };
    let _ = handle.emit("daemon:debug", payload);
}

#[cfg(test)]
mod tests {
    use super::DaemonClient;
    use crate::daemon::config::DaemonConfig;
    use crate::daemon::protocol::{
        GitDiffResult, GitLogResult, GitStatusResult, SessionIdParams, SessionInfo,
        METHOD_GIT_DIFF, METHOD_GIT_LOG, METHOD_GIT_STATUS, METHOD_LIST_SESSIONS,
    };
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{timeout, Duration};

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    async fn read_line(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> String {
        let mut line = String::new();
        let bytes = timeout(TEST_TIMEOUT, reader.read_line(&mut line))
            .await
            .expect("read timeout")
            .expect("read line");
        assert!(bytes > 0, "expected request");
        line
    }

    #[test]
    fn client_connects_and_lists_sessions() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind listener");
            let addr = listener.local_addr().expect("local addr");

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    handle_mock_server(stream).await;
                }
            });

            let config = DaemonConfig {
                host: "127.0.0.1".to_string(),
                port: addr.port(),
                token: "secret".to_string(),
            };

            let client = DaemonClient::connect_without_app(&config)
                .await
                .expect("connect");
            let sessions: Vec<SessionInfo> = client
                .call(METHOD_LIST_SESSIONS, Option::<()>::None)
                .await
                .expect("list sessions");

            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].path, "/tmp/project");
            assert_eq!(sessions[0].name, "project");
        });
    }

    #[test]
    fn client_requests_git_status() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind listener");
            let addr = listener.local_addr().expect("local addr");

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    handle_mock_git_status_server(stream).await;
                }
            });

            let config = DaemonConfig {
                host: "127.0.0.1".to_string(),
                port: addr.port(),
                token: "secret".to_string(),
            };

            let client = DaemonClient::connect_without_app(&config)
                .await
                .expect("connect");
            let status: GitStatusResult = client
                .call(
                    METHOD_GIT_STATUS,
                    Some(SessionIdParams {
                        session_id: "/tmp/project".to_string(),
                    }),
                )
                .await
                .expect("git status");

            assert_eq!(status.branch_name, "main");
            assert_eq!(status.staged_files.len(), 1);
            assert_eq!(status.unstaged_files.len(), 1);
            assert_eq!(status.total_additions, 3);
            assert_eq!(status.total_deletions, 1);
        });
    }

    #[test]
    fn client_requests_git_diff() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind listener");
            let addr = listener.local_addr().expect("local addr");

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    handle_mock_git_diff_server(stream).await;
                }
            });

            let config = DaemonConfig {
                host: "127.0.0.1".to_string(),
                port: addr.port(),
                token: "secret".to_string(),
            };

            let client = DaemonClient::connect_without_app(&config)
                .await
                .expect("connect");
            let diff: GitDiffResult = client
                .call(
                    METHOD_GIT_DIFF,
                    Some(SessionIdParams {
                        session_id: "/tmp/project".to_string(),
                    }),
                )
                .await
                .expect("git diff");

            assert_eq!(diff.files.len(), 1);
            assert_eq!(diff.files[0].path, "src/lib.rs");
            assert!(diff.files[0].diff.contains("+fn test()"));
            assert!(!diff.truncated);
            assert!(diff.truncated_files.is_empty());
        });
    }

    #[test]
    fn client_requests_git_log() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind listener");
            let addr = listener.local_addr().expect("local addr");

            tokio::spawn(async move {
                if let Ok((stream, _)) = listener.accept().await {
                    handle_mock_git_log_server(stream).await;
                }
            });

            let config = DaemonConfig {
                host: "127.0.0.1".to_string(),
                port: addr.port(),
                token: "secret".to_string(),
            };

            let client = DaemonClient::connect_without_app(&config)
                .await
                .expect("connect");
            let log: GitLogResult = client
                .call(
                    METHOD_GIT_LOG,
                    Some(crate::daemon::protocol::GitLogParams {
                        session_id: "/tmp/project".to_string(),
                        limit: Some(1),
                    }),
                )
                .await
                .expect("git log");

            assert_eq!(log.entries.len(), 1);
            assert_eq!(log.entries[0].sha, "abc123");
            assert_eq!(log.entries[0].summary, "Init repo");
            assert_eq!(log.ahead, 0);
            assert_eq!(log.behind, 0);
        });
    }

    async fn handle_mock_server(stream: TcpStream) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let auth_line = read_line(&mut reader).await;
        let auth_value: serde_json::Value =
            serde_json::from_str(auth_line.trim()).expect("auth json");
        assert_eq!(auth_value.get("method"), Some(&json!("auth")));

        let auth_response = json!({"id": 1, "result": {"ok": true}}).to_string();
        writer
            .write_all(auth_response.as_bytes())
            .await
            .expect("write auth response");
        writer.write_all(b"\n").await.expect("newline");

        let list_line = read_line(&mut reader).await;
        let list_value: serde_json::Value =
            serde_json::from_str(list_line.trim()).expect("list json");
        assert_eq!(list_value.get("method"), Some(&json!("list_sessions")));

        let list_response = json!({
            "id": 2,
            "result": [{"path": "/tmp/project", "name": "project"}]
        })
        .to_string();
        writer
            .write_all(list_response.as_bytes())
            .await
            .expect("write list response");
        writer.write_all(b"\n").await.expect("newline");
    }

    async fn handle_mock_git_status_server(stream: TcpStream) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let auth_line = read_line(&mut reader).await;
        let auth_value: serde_json::Value =
            serde_json::from_str(auth_line.trim()).expect("auth json");
        assert_eq!(auth_value.get("method"), Some(&json!("auth")));

        let auth_response = json!({"id": 1, "result": {"ok": true}}).to_string();
        writer
            .write_all(auth_response.as_bytes())
            .await
            .expect("write auth response");
        writer.write_all(b"\n").await.expect("newline");

        let status_line = read_line(&mut reader).await;
        let status_value: serde_json::Value =
            serde_json::from_str(status_line.trim()).expect("status json");
        assert_eq!(status_value.get("method"), Some(&json!("git_status")));
        let params = status_value.get("params").expect("params");
        assert_eq!(params.get("session_id"), Some(&json!("/tmp/project")));

        let status_response = json!({
            "id": 2,
            "result": {
                "branchName": "main",
                "stagedFiles": [
                    {"path": "src/lib.rs", "status": "modified", "additions": 2, "deletions": 1}
                ],
                "unstagedFiles": [
                    {"path": "README.md", "status": "modified", "additions": 1, "deletions": 0}
                ],
                "totalAdditions": 3,
                "totalDeletions": 1
            }
        })
        .to_string();
        writer
            .write_all(status_response.as_bytes())
            .await
            .expect("write status response");
        writer.write_all(b"\n").await.expect("newline");
    }

    async fn handle_mock_git_diff_server(stream: TcpStream) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let auth_line = read_line(&mut reader).await;
        let auth_value: serde_json::Value =
            serde_json::from_str(auth_line.trim()).expect("auth json");
        assert_eq!(auth_value.get("method"), Some(&json!("auth")));

        let auth_response = json!({"id": 1, "result": {"ok": true}}).to_string();
        writer
            .write_all(auth_response.as_bytes())
            .await
            .expect("write auth response");
        writer.write_all(b"\n").await.expect("newline");

        let diff_line = read_line(&mut reader).await;
        let diff_value: serde_json::Value =
            serde_json::from_str(diff_line.trim()).expect("diff json");
        assert_eq!(diff_value.get("method"), Some(&json!("git_diff")));
        let params = diff_value.get("params").expect("params");
        assert_eq!(params.get("session_id"), Some(&json!("/tmp/project")));

        let diff_response = json!({
            "id": 2,
            "result": {
                "files": [
                    {"path": "src/lib.rs", "diff": "@@ -1 +1\n+fn test()"}
                ],
                "truncated": false,
                "truncated_files": []
            }
        })
        .to_string();
        writer
            .write_all(diff_response.as_bytes())
            .await
            .expect("write diff response");
        writer.write_all(b"\n").await.expect("newline");
    }

    async fn handle_mock_git_log_server(stream: TcpStream) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let auth_line = read_line(&mut reader).await;
        let auth_value: serde_json::Value =
            serde_json::from_str(auth_line.trim()).expect("auth json");
        assert_eq!(auth_value.get("method"), Some(&json!("auth")));

        let auth_response = json!({"id": 1, "result": {"ok": true}}).to_string();
        writer
            .write_all(auth_response.as_bytes())
            .await
            .expect("write auth response");
        writer.write_all(b"\n").await.expect("newline");

        let log_line = read_line(&mut reader).await;
        let log_value: serde_json::Value =
            serde_json::from_str(log_line.trim()).expect("log json");
        assert_eq!(log_value.get("method"), Some(&json!("git_log")));
        let params = log_value.get("params").expect("params");
        assert_eq!(params.get("session_id"), Some(&json!("/tmp/project")));

        let log_response = json!({
            "id": 2,
            "result": {
                "entries": [
                    {"sha": "abc123", "summary": "Init repo", "author": "Jane", "timestamp": 1700000000}
                ],
                "ahead": 0,
                "behind": 0,
                "upstream": null
            }
        })
        .to_string();
        writer
            .write_all(log_response.as_bytes())
            .await
            .expect("write log response");
        writer.write_all(b"\n").await.expect("newline");
    }
}
