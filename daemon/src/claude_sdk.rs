//! Claude Agent SDK server management
//!
//! Spawns and manages Claude SDK servers per workspace, bridging SSE events to clients.

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
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

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
    child: Option<Child>,
    sse_handle: Option<JoinHandle<()>>,
}

impl ClaudeSdkServer {
    /// Spawn a new Claude SDK server for the given workspace
    pub fn spawn(workspace_id: String, workspace_path: String) -> Result<Self, String> {
        let server_dir = resolve_server_dir()?;
        info!(
            "Spawning Claude SDK server for workspace {} at {}",
            workspace_id, workspace_path
        );

        let mut child = Command::new("bun")
            .args(["run", "serve"])
            .current_dir(server_dir)
            .env("MAESTRO_WORKSPACE_DIR", &workspace_path)
            .env("MAESTRO_HOST", "127.0.0.1")
            .env("MAESTRO_PORT", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn Claude SDK server: {e}"))?;

        let pid = child.id();

        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture stdout")?;
        let reader = BufReader::new(stdout);

        let mut base_url = None;
        for line in reader.lines().map_while(Result::ok) {
            debug!("Claude SDK stdout: {}", line);
            if let Some(url) = parse_listening_url(&line) {
                base_url = Some(url);
                break;
            }
        }

        let base_url = base_url.ok_or("Failed to parse server URL from stdout")?;
        info!(
            "Claude SDK server started at {} (pid: {})",
            base_url, pid
        );

        Ok(Self {
            workspace_id,
            workspace_path,
            base_url,
            pid,
            child: Some(child),
            sse_handle: None,
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

    /// Stop the SSE bridge
    pub fn stop_sse_bridge(&mut self) {
        if let Some(handle) = self.sse_handle.take() {
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

        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ClaudeSdkServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn resolve_server_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = env::var("MAESTRO_CLAUDE_SDK_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let cwd = env::current_dir().map_err(|e| format!("Failed to read cwd: {e}"))?;
    let candidates = [cwd.join("daemon/claude-sdk"), cwd.join("claude-sdk")];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err("Claude SDK server directory not found. Set MAESTRO_CLAUDE_SDK_DIR".to_string())
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
        info!("Starting Claude SDK SSE stream for workspace {}", workspace_id);

        match run_sse_stream(&base_url, &workspace_id, &workspace_path, &state).await {
            Ok(()) => {
                info!("Claude SDK SSE stream ended cleanly for {}", workspace_id);
                break;
            }
            Err(e) => {
                warn!(
                    "Claude SDK SSE disconnected for {}: {}, reconnecting...",
                    workspace_id, e
                );

                let delay = BACKOFF_DELAYS
                    .get(attempt)
                    .copied()
                    .unwrap_or(BACKOFF_DELAYS[BACKOFF_DELAYS.len() - 1]);

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

    let client = eventsource_client::ClientBuilder::for_url(&url)
        .map_err(|e| format!("Failed to create SSE client: {e}"))?
        .header("x-opencode-directory", workspace_path)
        .map_err(|e| format!("Failed to set header: {e}"))?
        .build();

    let mut stream = client.stream();

    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => {
                if let eventsource_client::SSE::Event(sse_event) = event {
                    let event_data: Value = match serde_json::from_str(&sse_event.data) {
                        Ok(v) => v,
                        Err(e) => {
                            debug!(
                                "Failed to parse Claude SDK SSE data as JSON: {} - {}",
                                e, sse_event.data
                            );
                            continue;
                        }
                    };

                    let daemon_event = OpenCodeDaemonEvent {
                        workspace_id: workspace_id.to_string(),
                        event_type: sse_event.event_type,
                        event: event_data,
                    };

                    let event = Event::new(EVENT_OPENCODE_EVENT, daemon_event);
                    let msg =
                        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());

                    state.broadcast_to_all_clients(msg).await;
                }
            }
            Err(e) => {
                return Err(format!("Claude SDK SSE stream error: {e}"));
            }
        }
    }

    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeSdkSession {
    pub id: String,
    #[serde(flatten)]
    pub data: Value,
}
