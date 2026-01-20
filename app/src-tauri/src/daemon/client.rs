use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
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

        // Disconnect existing client if any
        self.disconnect().await;

        let client = DaemonClient::connect(&config, app_handle.clone()).await?;
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
        app_handle: AppHandle,
    ) {
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
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
                                Self::handle_event(&app_handle, &parsed);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            // Mark as disconnected
            *connected.write().await = false;
            let _ = app_handle.emit("daemon:disconnected", serde_json::json!({"reason": "connection_lost"}));

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

    fn handle_event(app_handle: &AppHandle, parsed: &Value) {
        let method = match parsed.get("method").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => return,
        };

        let params = parsed.get("params").cloned().unwrap_or(Value::Null);

        match method {
            EVENT_TERMINAL_OUTPUT => {
                let _ = app_handle.emit("daemon:terminal_output", params);
            }
            EVENT_TERMINAL_EXITED => {
                let _ = app_handle.emit("daemon:terminal_exited", params);
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
            let delay = RECONNECT_DELAYS.get(attempt).copied().unwrap_or(RECONNECT_DELAYS[RECONNECT_DELAYS.len() - 1]);
            tokio::time::sleep(delay).await;

            if let Err(_e) = state.connect().await {
                attempt = (attempt + 1).min(RECONNECT_DELAYS.len() - 1);
            } else {
                attempt = 0;
            }
        }
    });
}
