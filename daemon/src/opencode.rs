//! OpenCode server management
//!
//! Spawns and manages OpenCode servers per workspace, bridging SSE events to clients.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use eventsource_client::Client as _;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::protocol::Event;
use crate::state::DaemonState;

/// OpenCode server instance for a workspace
pub struct OpenCodeServer {
    pub workspace_id: String,
    pub workspace_path: String,
    pub base_url: String,
    #[allow(dead_code)]
    pub pid: u32,
    child: Option<Child>,
    sse_handle: Option<JoinHandle<()>>,
}

impl OpenCodeServer {
    /// Spawn a new OpenCode server for the given workspace
    pub fn spawn(workspace_id: String, workspace_path: String) -> Result<Self, String> {
        info!(
            "Spawning OpenCode server for workspace {} at {}",
            workspace_id, workspace_path
        );

        // Spawn opencode serve with port 0 to get a random available port
        let mut child = Command::new("bun")
            .args(["run", "opencode", "serve", "--hostname", "127.0.0.1", "--port", "0"])
            .current_dir(&workspace_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn OpenCode server: {e}"))?;

        let pid = child.id();

        // Read stdout to find the port
        let stdout = child
            .stdout
            .take()
            .ok_or("Failed to capture stdout")?;
        let reader = BufReader::new(stdout);

        let mut base_url = None;
        for line in reader.lines().map_while(Result::ok) {
            debug!("OpenCode stdout: {}", line);
            // Look for "Listening on http://127.0.0.1:PORT" or similar
            if let Some(url) = parse_listening_url(&line) {
                base_url = Some(url);
                break;
            }
        }

        let base_url = base_url.ok_or("Failed to parse server URL from stdout")?;
        info!("OpenCode server started at {} (pid: {})", base_url, pid);

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

    /// Shutdown the OpenCode server
    pub fn shutdown(&mut self) {
        info!(
            "Shutting down OpenCode server for workspace {}",
            self.workspace_id
        );

        // Stop SSE bridge first
        self.stop_sse_bridge();

        // Kill the process
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Check if the server is healthy via GET /path
    #[allow(dead_code)]
    pub async fn health_check(&self) -> bool {
        let client = reqwest::Client::new();
        match client
            .get(format!("{}/path", self.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                warn!("Health check failed for {}: {}", self.workspace_id, e);
                false
            }
        }
    }
}

impl Drop for OpenCodeServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Parse the listening URL from OpenCode server stdout
fn parse_listening_url(line: &str) -> Option<String> {
    let line_lower = line.to_lowercase();
    if line_lower.contains("listening") || line_lower.contains("started") {
        // Look for http://... URL in the line
        if let Some(start) = line.find("http://") {
            let rest = &line[start..];
            // Find the end of the URL (space, newline, or end of string)
            let end = rest
                .find(|c: char| c.is_whitespace())
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Backoff durations for SSE reconnection
const BACKOFF_DELAYS: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// Run the SSE bridge, auto-reconnecting on disconnect
async fn run_sse_bridge(
    base_url: String,
    workspace_id: String,
    workspace_path: String,
    state: Arc<DaemonState>,
) {
    let mut attempt = 0usize;

    loop {
        info!("Starting SSE stream for workspace {}", workspace_id);

        match run_sse_stream(&base_url, &workspace_id, &workspace_path, &state).await {
            Ok(()) => {
                // Clean exit, stop the loop
                info!("SSE stream ended cleanly for workspace {}", workspace_id);
                break;
            }
            Err(e) => {
                warn!(
                    "SSE disconnected for {}: {}, reconnecting...",
                    workspace_id, e
                );

                // Exponential backoff
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

/// Run a single SSE stream connection
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
                    // Parse the SSE event data as JSON
                    let event_data: Value = match serde_json::from_str(&sse_event.data) {
                        Ok(v) => v,
                        Err(e) => {
                            debug!(
                                "Failed to parse SSE event data as JSON: {} - {}",
                                e, sse_event.data
                            );
                            continue;
                        }
                    };

                    // Wrap in daemon event envelope
                    let daemon_event = OpenCodeDaemonEvent {
                        workspace_id: workspace_id.to_string(),
                        event_type: sse_event.event_type,
                        event: event_data,
                    };

                    // Broadcast to all connected clients
                    let event = Event::new(EVENT_OPENCODE_EVENT, daemon_event);
                    let msg =
                        serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());

                    state.broadcast_to_all_clients(msg).await;
                }
            }
            Err(e) => {
                return Err(format!("SSE stream error: {e}"));
            }
        }
    }

    Ok(())
}

/// Event name for OpenCode events forwarded to clients
pub const EVENT_OPENCODE_EVENT: &str = "opencode:event";

/// Wrapper for OpenCode events sent to clients
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeDaemonEvent {
    pub workspace_id: String,
    pub event_type: String,
    pub event: Value,
}

/// OpenCode session info (from OpenCode API)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCodeSession {
    pub id: String,
    #[serde(flatten)]
    pub data: Value,
}

/// OpenCode server registry operations
pub struct OpenCodeRegistry;

impl OpenCodeRegistry {
    /// Create an HTTP client for proxying requests to OpenCode
    pub fn http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default()
    }

    /// Proxy GET request to OpenCode server
    pub async fn proxy_get(base_url: &str, path: &str, headers: Option<Vec<(&str, &str)>>) -> Result<Value, String> {
        let client = Self::http_client();
        let url = format!("{}{}", base_url, path);

        let mut req = client.get(&url);
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k, v);
            }
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {} from OpenCode", resp.status()));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))
    }

    /// Proxy POST request to OpenCode server
    pub async fn proxy_post(
        base_url: &str,
        path: &str,
        body: Option<Value>,
        headers: Option<Vec<(&str, &str)>>,
    ) -> Result<Value, String> {
        let client = Self::http_client();
        let url = format!("{}{}", base_url, path);

        let mut req = client.post(&url);
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req = req.header(k, v);
            }
        }
        if let Some(body) = body {
            req = req.json(&body);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status} from OpenCode: {body}"));
        }

        resp.json::<Value>()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_listening_url_finds_url() {
        let line = "Listening on http://127.0.0.1:3000";
        assert_eq!(
            parse_listening_url(line),
            Some("http://127.0.0.1:3000".to_string())
        );

        let line2 = "Server started at http://localhost:8080/api";
        assert_eq!(
            parse_listening_url(line2),
            Some("http://localhost:8080/api".to_string())
        );

        let line3 = "Some other message";
        assert_eq!(parse_listening_url(line3), None);
    }
}
