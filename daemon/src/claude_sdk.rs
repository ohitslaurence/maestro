//! Claude Agent SDK server management
//!
//! Spawns and manages Claude SDK servers per workspace, bridging SSE events to clients.
//! Implements auto-restart on crash (once) per spec §10 Design Decision 2.

use std::env;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eventsource_client::Client as _;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::opencode::{OpenCodeDaemonEvent, EVENT_OPENCODE_EVENT};
use crate::protocol::Event;
use crate::state::{DaemonState, ServerStatus};

/// Find an available port by binding to port 0 and returning the assigned port.
/// Per spec §5 step 1: Daemon allocates an available port.
fn find_available_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Failed to bind to ephemeral port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local addr: {e}"))?
        .port();
    // Listener is dropped here, releasing the port for the server to use
    Ok(port)
}

/// Health-check configuration per spec §5 step 3
const HEALTH_CHECK_INTERVAL_MS: u64 = 100;
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 30;

/// Claude SDK server instance for a workspace
pub struct ClaudeSdkServer {
    pub workspace_id: String,
    pub workspace_path: String,
    pub base_url: String,
    pub port: u16,
    #[allow(dead_code)]
    pub pid: u32,
    child: Arc<Mutex<Option<Child>>>,
    sse_handle: Option<JoinHandle<()>>,
    /// Handle for the process monitor task (auto-restart on crash per spec §10)
    monitor_handle: Option<JoinHandle<()>>,
    /// Handle for the health-check task (spec §5 step 3)
    health_check_handle: Option<JoinHandle<()>>,
}

impl ClaudeSdkServer {
    /// Spawn a new Claude SDK server for the given workspace.
    /// Per spec §5 step 1: Daemon allocates an available port and spawns with MAESTRO_PORT.
    pub fn spawn(workspace_id: String, workspace_path: String) -> Result<Self, String> {
        let port = find_available_port()?;
        info!("[claude_sdk] Allocated port {} for workspace {}", port, workspace_id);
        let (child, pid, base_url) = spawn_server_process(&workspace_path, port)?;

        info!(
            "Claude SDK server started at {} (pid: {}, port: {})",
            base_url, pid, port
        );

        Ok(Self {
            workspace_id,
            workspace_path,
            base_url,
            port,
            pid,
            child: Arc::new(Mutex::new(Some(child))),
            sse_handle: None,
            monitor_handle: None,
            health_check_handle: None,
        })
    }

    /// Start the SSE bridge to forward events to all clients
    pub fn start_sse_bridge(&mut self, state: Arc<DaemonState>) {
        let base_url = self.base_url.clone();
        let workspace_id = self.workspace_id.clone();
        let workspace_path = self.workspace_path.clone();

        let handle = tokio::spawn(async move {
            run_sse_bridge(base_url, workspace_id, workspace_path, state).await;
        });

        self.sse_handle = Some(handle);
    }

    /// Start health-check polling to transition Starting → Ready (spec §5 step 3).
    /// On success (200 OK), transitions status to Ready and starts SSE bridge.
    pub fn start_health_check(&mut self, state: Arc<DaemonState>) {
        let base_url = self.base_url.clone();
        let workspace_id = self.workspace_id.clone();
        let workspace_path = self.workspace_path.clone();

        let handle = tokio::spawn(async move {
            run_health_check(base_url, workspace_id, workspace_path, state).await;
        });

        self.health_check_handle = Some(handle);
    }

    /// Stop the health-check task
    fn stop_health_check(&mut self) {
        if let Some(handle) = self.health_check_handle.take() {
            handle.abort();
        }
    }

    /// Start process monitoring for auto-restart on crash (spec §10)
    pub fn start_process_monitor(&mut self, state: Arc<DaemonState>) {
        let workspace_id = self.workspace_id.clone();
        let workspace_path = self.workspace_path.clone();
        let port = self.port;
        let child_handle = Arc::clone(&self.child);

        let handle = tokio::spawn(async move {
            monitor_process(workspace_id, workspace_path, port, child_handle, state).await;
        });

        self.monitor_handle = Some(handle);
    }

    /// Stop the SSE bridge
    pub fn stop_sse_bridge(&mut self) {
        if let Some(handle) = self.sse_handle.take() {
            handle.abort();
        }
    }

    /// Stop the process monitor
    fn stop_process_monitor(&mut self) {
        if let Some(handle) = self.monitor_handle.take() {
            handle.abort();
        }
    }

    /// Shutdown the Claude SDK server
    pub fn shutdown(&mut self) {
        info!(
            "Shutting down Claude SDK server for workspace {}",
            self.workspace_id
        );

        self.stop_sse_bridge();
        self.stop_process_monitor();
        self.stop_health_check();

        // Use try_lock to avoid blocking; if locked, the monitor is handling shutdown
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl Drop for ClaudeSdkServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn resolve_server_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = env::var("MAESTRO_CLAUDE_SERVER_DIR") {
        debug!("[claude_sdk] Using MAESTRO_CLAUDE_SERVER_DIR: {}", dir);
        return Ok(PathBuf::from(dir));
    }

    let cwd = env::current_dir().map_err(|e| format!("Failed to read cwd: {e}"))?;
    debug!("[claude_sdk] Resolving server dir from cwd: {}", cwd.display());

    let candidates = [
        cwd.join("daemon/claude-server"),
        cwd.join("claude-server"),
    ];

    for candidate in &candidates {
        debug!("[claude_sdk] Checking candidate: {} (exists={})", candidate.display(), candidate.exists());
        if candidate.exists() {
            info!("[claude_sdk] Found server directory: {}", candidate.display());
            return Ok(candidate.clone());
        }
    }

    error!("[claude_sdk] Server directory not found. Checked: {:?}", candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>());
    Err("Claude server directory not found. Set MAESTRO_CLAUDE_SERVER_DIR".to_string())
}

/// Spawn the server process and wait for the listening URL.
/// Per spec §5 step 1: pass the allocated port via MAESTRO_PORT.
/// Returns (Child, pid, base_url) on success.
fn spawn_server_process(workspace_path: &str, port: u16) -> Result<(Child, u32, String), String> {
    let server_dir = resolve_server_dir()?;
    info!(
        "[claude_sdk] Spawning server: workspace={} server_dir={} port={}",
        workspace_path,
        server_dir.display(),
        port
    );

    debug!("[claude_sdk] Running: bun run serve");
    debug!("[claude_sdk] Env: MAESTRO_WORKSPACE_DIR={}", workspace_path);
    debug!("[claude_sdk] Env: MAESTRO_HOST=127.0.0.1 MAESTRO_PORT={}", port);

    // Use env var if set, otherwise default to acceptEdits (auto-approve file operations)
    let permission_mode = env::var("MAESTRO_CLAUDE_PERMISSION_MODE")
        .unwrap_or_else(|_| "acceptEdits".to_string());
    debug!("[claude_sdk] Env: MAESTRO_CLAUDE_PERMISSION_MODE={}", permission_mode);

    let mut child = Command::new("bun")
        .args(["run", "serve"])
        .current_dir(&server_dir)
        .env("MAESTRO_WORKSPACE_DIR", workspace_path)
        .env("MAESTRO_HOST", "127.0.0.1")
        .env("MAESTRO_PORT", port.to_string())
        .env("MAESTRO_CLAUDE_PERMISSION_MODE", permission_mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            error!("[claude_sdk] Failed to spawn: {}", e);
            format!("Failed to spawn Claude SDK server: {e}")
        })?;

    let pid = child.id();
    info!("[claude_sdk] Server process started with pid={}", pid);

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take();
    let reader = BufReader::new(stdout);

    debug!("[claude_sdk] Reading stdout to find listening URL...");
    let mut base_url = None;
    let mut line_count = 0;
    for line in reader.lines().map_while(Result::ok) {
        line_count += 1;
        info!("[claude_sdk] stdout[{}]: {}", line_count, line);
        if let Some(url) = parse_listening_url(&line) {
            info!("[claude_sdk] Found listening URL: {}", url);
            base_url = Some(url);
            break;
        }
    }

    if base_url.is_none() {
        error!("[claude_sdk] Stdout closed after {} lines without finding listening URL", line_count);

        // Capture stderr for error diagnosis
        if let Some(stderr) = stderr {
            let stderr_reader = BufReader::new(stderr);
            for (i, line) in stderr_reader.lines().map_while(Result::ok).enumerate() {
                error!("[claude_sdk] stderr[{}]: {}", i + 1, line);
            }
        }

        // Check if process exited
        match child.try_wait() {
            Ok(Some(status)) => error!("[claude_sdk] Process exited with: {}", status),
            Ok(None) => error!("[claude_sdk] Process still running but no output"),
            Err(e) => error!("[claude_sdk] Failed to check process status: {}", e),
        }

        return Err("Failed to parse server URL from stdout".to_string());
    }

    let base_url = base_url.unwrap();
    info!("[claude_sdk] Server ready at {} (pid={})", base_url, pid);
    Ok((child, pid, base_url))
}

/// Monitor the server process and auto-restart on crash (once).
/// Per spec §10 Design Decision 2: auto-restart once, then mark as Error.
/// Per spec §5 step 4: On restart, try same port first; if EADDRINUSE, allocate new port.
async fn monitor_process(
    workspace_id: String,
    workspace_path: String,
    initial_port: u16,
    child_handle: Arc<Mutex<Option<Child>>>,
    state: Arc<DaemonState>,
) {
    let mut restart_count = 0u32;
    let mut current_port = initial_port;
    const MAX_RESTARTS: u32 = 1; // Auto-restart once on crash

    loop {
        // Wait for process exit
        let exit_status = {
            let mut guard = child_handle.lock().await;
            if let Some(ref mut child) = *guard {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        // Process exited
                        guard.take(); // Clear the child
                        Some(status)
                    }
                    Ok(None) => {
                        // Process still running
                        None
                    }
                    Err(e) => {
                        warn!("Error checking process status for {}: {}", workspace_id, e);
                        None
                    }
                }
            } else {
                // No child process, monitor should exit
                debug!("No child process for {}, stopping monitor", workspace_id);
                return;
            }
        };

        if let Some(status) = exit_status {
            if status.success() {
                info!(
                    "Claude SDK server for {} exited cleanly",
                    workspace_id
                );
                return;
            }

            // Process crashed
            warn!(
                "Claude SDK server for {} crashed with status {:?}",
                workspace_id, status
            );

            if restart_count >= MAX_RESTARTS {
                error!(
                    "Claude SDK server for {} crashed {} times, marking as Error",
                    workspace_id,
                    restart_count + 1
                );

                // Update server status to Error in state
                // (Note: We can't directly update status field due to ownership,
                // but the server will be removed and require explicit re-spawn)
                state.remove_claude_sdk_server(&workspace_id).await;
                return;
            }

            // Attempt restart
            restart_count += 1;
            info!(
                "Attempting to restart Claude SDK server for {} (attempt {})",
                workspace_id, restart_count
            );

            // Per spec §5 step 4: Set status to Starting before respawn
            state
                .update_claude_server_status(&workspace_id, ServerStatus::Starting)
                .await;

            // Per spec §5 step 4: Wait 1s before respawn
            tokio::time::sleep(Duration::from_secs(1)).await;

            // Try same port first (per spec §5 step 4)
            let spawn_result = spawn_server_process(&workspace_path, current_port);
            let spawn_result = match spawn_result {
                Ok(result) => Ok(result),
                Err(e) if e.contains("EADDRINUSE") || e.contains("address already in use") => {
                    // Port in use, allocate new port (per spec §5 Edge Cases)
                    info!(
                        "[claude_sdk] Port {} in use, allocating new port for {}",
                        current_port, workspace_id
                    );
                    match find_available_port() {
                        Ok(new_port) => {
                            current_port = new_port;
                            spawn_server_process(&workspace_path, new_port)
                        }
                        Err(port_err) => Err(port_err),
                    }
                }
                Err(e) => Err(e),
            };

            match spawn_result {
                Ok((new_child, new_pid, new_url)) => {
                    info!(
                        "Claude SDK server for {} restarted at {} (pid: {}, port: {})",
                        workspace_id, new_url, new_pid, current_port
                    );

                    // Store new child
                    let mut guard = child_handle.lock().await;
                    *guard = Some(new_child);

                    // Update runtime state with new port/url if changed (per spec §5 Edge Cases)
                    state.update_claude_server_url(&workspace_id, current_port, new_url).await;
                }
                Err(e) => {
                    error!(
                        "Failed to restart Claude SDK server for {}: {}",
                        workspace_id, e
                    );
                    state.remove_claude_sdk_server(&workspace_id).await;
                    return;
                }
            }
        }

        // Check every 2 seconds
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Parse the listening URL from Claude SDK server stdout
fn parse_listening_url(line: &str) -> Option<String> {
    let line_lower = line.to_lowercase();
    if line_lower.contains("listening") || line_lower.contains("started") {
        if let Some(start) = line.find("http://") {
            let rest = &line[start..];
            let end = rest
                .find(|c: char| c.is_whitespace())
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Health-check polling to transition Starting → Ready (spec §5 step 3).
/// Polls GET {base_url}/health at 100ms intervals, times out after 30s.
/// On success, transitions status to Ready, resets restart_count, and starts SSE bridge.
async fn run_health_check(
    base_url: String,
    workspace_id: String,
    workspace_path: String,
    state: Arc<DaemonState>,
) {
    let health_url = format!("{}/health", base_url);
    let start = Instant::now();
    let timeout = Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS);
    let interval = Duration::from_millis(HEALTH_CHECK_INTERVAL_MS);

    info!(
        "[claude_sdk] Starting health-check polling for {} at {}",
        workspace_id, health_url
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    loop {
        if start.elapsed() > timeout {
            error!(
                "[claude_sdk] Health-check timeout for {} after {:?}",
                workspace_id, timeout
            );
            state
                .update_claude_server_status(
                    &workspace_id,
                    ServerStatus::Error("Health check timeout".to_string()),
                )
                .await;
            return;
        }

        match client.get(&health_url).send().await {
            Ok(response) if response.status().is_success() => {
                info!(
                    "[claude_sdk] Health-check passed for {} (status={})",
                    workspace_id,
                    response.status()
                );

                // Transition to Ready and reset restart count (spec §3, §5 step 3)
                state
                    .update_claude_server_status(&workspace_id, ServerStatus::Ready)
                    .await;
                state.reset_claude_server_restart_count(&workspace_id).await;

                // Start SSE bridge now that server is ready
                // We need to get a mutable reference to the server to start the bridge
                let mut servers = state.claude_sdk_servers.write().await;
                if let Some(server) = servers.get_mut(&workspace_id) {
                    let bridge_state = Arc::clone(&state);
                    let bridge_base_url = base_url.clone();
                    let bridge_workspace_id = workspace_id.clone();
                    let bridge_workspace_path = workspace_path.clone();

                    let handle = tokio::spawn(async move {
                        run_sse_bridge(
                            bridge_base_url,
                            bridge_workspace_id,
                            bridge_workspace_path,
                            bridge_state,
                        )
                        .await;
                    });
                    server.sse_handle = Some(handle);
                }

                return;
            }
            Ok(response) => {
                debug!(
                    "[claude_sdk] Health-check not ready for {} (status={})",
                    workspace_id,
                    response.status()
                );
            }
            Err(e) => {
                debug!(
                    "[claude_sdk] Health-check error for {}: {}",
                    workspace_id, e
                );
            }
        }

        tokio::time::sleep(interval).await;
    }
}

const BACKOFF_DELAYS: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

async fn run_sse_bridge(
    base_url: String,
    workspace_id: String,
    workspace_path: String,
    state: Arc<DaemonState>,
) {
    let mut attempt = 0usize;

    loop {
        info!("[claude_sdk] SSE bridge starting: workspace={} url={}/event attempt={}",
              workspace_id, base_url, attempt);

        match run_sse_stream(&base_url, &workspace_id, &workspace_path, &state).await {
            Ok(()) => {
                info!("[claude_sdk] SSE stream ended cleanly: workspace={}", workspace_id);
                break;
            }
            Err(e) => {
                let delay = BACKOFF_DELAYS
                    .get(attempt)
                    .copied()
                    .unwrap_or(BACKOFF_DELAYS[BACKOFF_DELAYS.len() - 1]);

                warn!(
                    "[claude_sdk] SSE disconnected: workspace={} error={} retry_delay={:?}",
                    workspace_id, e, delay
                );

                tokio::time::sleep(delay).await;
                attempt = (attempt + 1).min(BACKOFF_DELAYS.len() - 1);
            }
        }
    }
}

async fn run_sse_stream(
    base_url: &str,
    workspace_id: &str,
    workspace_path: &str,
    state: &Arc<DaemonState>,
) -> Result<(), String> {
    let url = format!("{}/event", base_url);
    debug!("[claude_sdk] Connecting to SSE: {}", url);

    let client = eventsource_client::ClientBuilder::for_url(&url)
        .map_err(|e| format!("Failed to create SSE client: {e}"))?
        .header("x-opencode-directory", workspace_path)
        .map_err(|e| format!("Failed to set header: {e}"))?
        .build();

    let mut stream = client.stream();
    let mut event_count = 0u64;
    info!("[claude_sdk] SSE stream connected: workspace={}", workspace_id);

    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => {
                if let eventsource_client::SSE::Event(sse_event) = event {
                    event_count += 1;
                    let event_type = &sse_event.event_type;

                    let event_data: Value = match serde_json::from_str(&sse_event.data) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(
                                "[claude_sdk] SSE parse error: type={} error={} data={}",
                                event_type, e, sse_event.data
                            );
                            continue;
                        }
                    };

                    // Extract session_id if present for logging
                    let session_id = event_data.get("sessionId")
                        .or_else(|| event_data.get("session_id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    debug!(
                        "[claude_sdk] SSE event #{}: type={} session={} workspace={}",
                        event_count, event_type, session_id, workspace_id
                    );

                    let daemon_event = OpenCodeDaemonEvent {
                        workspace_id: workspace_id.to_string(),
                        event_type: event_type.clone(),
                        event: event_data,
                    };

                    let event = Event::new(EVENT_OPENCODE_EVENT, daemon_event);
                    let msg =
                        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());

                    let client_count = state.broadcast_to_all_clients(msg).await;
                    debug!("[claude_sdk] Broadcast to {} clients", client_count);
                }
            }
            Err(e) => {
                error!("[claude_sdk] SSE stream error: {} (after {} events)", e, event_count);
                return Err(format!("Claude SDK SSE stream error: {e}"));
            }
        }
    }

    info!("[claude_sdk] SSE stream closed after {} events", event_count);
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeSdkSession {
    pub id: String,
    #[serde(flatten)]
    pub data: Value,
}
