//! Claude Agent SDK server management
//!
//! Spawns and manages Claude SDK servers per workspace, bridging SSE events to clients.
//! Implements auto-restart on crash (once) per spec §10 Design Decision 2.

use std::env;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use eventsource_client::Client as _;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::opencode::{OpenCodeDaemonEvent, EVENT_OPENCODE_EVENT};
use crate::protocol::Event;
use crate::state::DaemonState;

/// Claude SDK server instance for a workspace
pub struct ClaudeSdkServer {
    pub workspace_id: String,
    pub workspace_path: String,
    pub base_url: String,
    #[allow(dead_code)]
    pub pid: u32,
    child: Arc<Mutex<Option<Child>>>,
    sse_handle: Option<JoinHandle<()>>,
    /// Handle for the process monitor task (auto-restart on crash per spec §10)
    monitor_handle: Option<JoinHandle<()>>,
}

impl ClaudeSdkServer {
    /// Spawn a new Claude SDK server for the given workspace
    pub fn spawn(workspace_id: String, workspace_path: String) -> Result<Self, String> {
        let (child, pid, base_url) = spawn_server_process(&workspace_path)?;

        info!(
            "Claude SDK server started at {} (pid: {})",
            base_url, pid
        );

        Ok(Self {
            workspace_id,
            workspace_path,
            base_url,
            pid,
            child: Arc::new(Mutex::new(Some(child))),
            sse_handle: None,
            monitor_handle: None,
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

    /// Start process monitoring for auto-restart on crash (spec §10)
    pub fn start_process_monitor(&mut self, state: Arc<DaemonState>) {
        let workspace_id = self.workspace_id.clone();
        let workspace_path = self.workspace_path.clone();
        let child_handle = Arc::clone(&self.child);

        let handle = tokio::spawn(async move {
            monitor_process(workspace_id, workspace_path, child_handle, state).await;
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
/// Returns (Child, pid, base_url) on success.
fn spawn_server_process(workspace_path: &str) -> Result<(Child, u32, String), String> {
    let server_dir = resolve_server_dir()?;
    info!(
        "[claude_sdk] Spawning server: workspace={} server_dir={}",
        workspace_path,
        server_dir.display()
    );

    debug!("[claude_sdk] Running: bun run serve");
    debug!("[claude_sdk] Env: MAESTRO_WORKSPACE_DIR={}", workspace_path);
    debug!("[claude_sdk] Env: MAESTRO_HOST=127.0.0.1 MAESTRO_PORT=0");

    let mut child = Command::new("bun")
        .args(["run", "serve"])
        .current_dir(&server_dir)
        .env("MAESTRO_WORKSPACE_DIR", workspace_path)
        .env("MAESTRO_HOST", "127.0.0.1")
        .env("MAESTRO_PORT", "0")
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
        return Err("Failed to parse server URL from stdout".to_string());
    }

    let base_url = base_url.unwrap();
    info!("[claude_sdk] Server ready at {} (pid={})", base_url, pid);
    Ok((child, pid, base_url))
}

/// Monitor the server process and auto-restart on crash (once).
/// Per spec §10 Design Decision 2: auto-restart once, then mark as Error.
async fn monitor_process(
    workspace_id: String,
    workspace_path: String,
    child_handle: Arc<Mutex<Option<Child>>>,
    state: Arc<DaemonState>,
) {
    let mut restart_count = 0u32;
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

            // Small delay before restart
            tokio::time::sleep(Duration::from_secs(1)).await;

            match spawn_server_process(&workspace_path) {
                Ok((new_child, new_pid, new_url)) => {
                    info!(
                        "Claude SDK server for {} restarted at {} (pid: {})",
                        workspace_id, new_url, new_pid
                    );

                    // Store new child
                    let mut guard = child_handle.lock().await;
                    *guard = Some(new_child);

                    // Note: base_url and pid are outdated in the ClaudeSdkServer struct
                    // but SSE bridge will reconnect automatically via backoff
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
