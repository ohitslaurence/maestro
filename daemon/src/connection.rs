use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::handlers;
use crate::protocol::{ErrorResponse, Request, AUTH_REQUIRED};
use crate::state::{ClientId, DaemonState};

const AUTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Handle a single client connection
pub async fn handle_client(stream: TcpStream, state: Arc<DaemonState>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    info!("Client connected: {peer}");

    let (client_id, event_rx) = state.register_client().await;
    debug!("Assigned client_id={client_id} to {peer}");

    let result = handle_client_inner(stream, state.clone(), client_id, event_rx).await;

    if let Err(e) = result {
        debug!("Client {peer} error: {e}");
    }

    info!("Client disconnected: {peer}");
    state.unregister_client(client_id).await;
}

async fn handle_client_inner(
    stream: TcpStream,
    state: Arc<DaemonState>,
    client_id: ClientId,
    mut event_rx: mpsc::UnboundedReceiver<String>,
) -> Result<(), String> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Auth phase
    let authenticated = if state.token.is_some() {
        match timeout(AUTH_TIMEOUT, wait_for_auth(&mut reader, &mut writer, &state)).await {
            Ok(Ok(true)) => true,
            Ok(Ok(false)) => {
                // Auth failed but timeout not exceeded, keep trying
                // Actually, wait_for_auth returns true only on success
                false
            }
            Ok(Err(e)) => {
                debug!("Auth error: {e}");
                return Err(e);
            }
            Err(_) => {
                warn!("Auth timeout for client {client_id}");
                return Err("Auth timeout".to_string());
            }
        }
    } else {
        // No auth required
        true
    };

    if !authenticated {
        return Err("Authentication failed".to_string());
    }

    // Main loop: read requests and forward events
    loop {
        tokio::select! {
            // Read request from client
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Some(response) = process_request(trimmed, state.clone(), client_id).await {
                                if let Err(e) = writer.write_all(response.as_bytes()).await {
                                    error!("Failed to write response: {e}");
                                    break;
                                }
                                if let Err(e) = writer.write_all(b"\n").await {
                                    error!("Failed to write newline: {e}");
                                    break;
                                }
                            }
                        }
                        line.clear();
                    }
                    Err(e) => {
                        debug!("Read error: {e}");
                        break;
                    }
                }
            }

            // Forward events to client
            Some(event) = event_rx.recv() => {
                if let Err(e) = writer.write_all(event.as_bytes()).await {
                    error!("Failed to write event: {e}");
                    break;
                }
                if let Err(e) = writer.write_all(b"\n").await {
                    error!("Failed to write newline: {e}");
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Wait for auth request within timeout
async fn wait_for_auth(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    state: &DaemonState,
) -> Result<bool, String> {
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => return Err("Connection closed".to_string()),
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Parse request
                let request: Request = match serde_json::from_str(trimmed) {
                    Ok(r) => r,
                    Err(e) => {
                        let resp = ErrorResponse::new(0, "invalid_params", format!("Invalid JSON: {e}"));
                        let json = serde_json::to_string(&resp).unwrap();
                        let _ = writer.write_all(json.as_bytes()).await;
                        let _ = writer.write_all(b"\n").await;
                        continue;
                    }
                };

                if request.method == "auth" {
                    let response = handlers::auth::handle(&request, state).await;
                    let _ = writer.write_all(response.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;

                    // Check if auth succeeded
                    if response.contains("\"ok\":true") || response.contains("\"ok\": true") {
                        return Ok(true);
                    }
                    // Auth failed, but allow retry (loop continues)
                } else {
                    // Non-auth request before authentication
                    let resp = ErrorResponse::new(
                        request.id,
                        AUTH_REQUIRED,
                        "Authentication required. Send auth request first.",
                    );
                    let json = serde_json::to_string(&resp).unwrap();
                    let _ = writer.write_all(json.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                }
            }
            Err(e) => return Err(format!("Read error: {e}")),
        }
    }
}

/// Process a single request and return JSON response
async fn process_request(
    line: &str,
    state: Arc<DaemonState>,
    client_id: ClientId,
) -> Option<String> {
    let request: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let resp = ErrorResponse::new(0, "invalid_params", format!("Invalid JSON: {e}"));
            return Some(serde_json::to_string(&resp).unwrap());
        }
    };

    Some(handlers::dispatch(&request, state, client_id).await)
}

#[cfg(test)]
mod tests {
    use super::handle_client;
    use crate::protocol::SessionInfo;
    use crate::state::DaemonState;
    use serde_json::Value;
    use std::sync::Arc;
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
        assert!(bytes > 0, "expected response line");
        line
    }

    #[tokio::test]
    async fn rpc_auth_and_list_sessions_round_trip() {
        let sessions = vec![SessionInfo {
            path: "/tmp/project".to_string(),
            name: "project".to_string(),
        }];
        let state = Arc::new(DaemonState::new(Some("secret".to_string()), sessions));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        let state_clone = state.clone();

        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                handle_client(stream, state_clone).await;
            }
        });

        let stream = timeout(TEST_TIMEOUT, TcpStream::connect(addr))
            .await
            .expect("connect timeout")
            .expect("connect");
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        writer
            .write_all(b"{\"id\":1,\"method\":\"auth\",\"params\":{\"token\":\"secret\"}}\n")
            .await
            .expect("write auth");

        let auth_line = read_line(&mut reader).await;
        let auth_value: Value = serde_json::from_str(auth_line.trim()).expect("auth json");
        assert!(auth_value.get("result").is_some());

        writer
            .write_all(b"{\"id\":2,\"method\":\"list_sessions\",\"params\":{}}\n")
            .await
            .expect("write list_sessions");

        let list_line = read_line(&mut reader).await;
        let list_value: Value = serde_json::from_str(list_line.trim()).expect("list json");
        let result = list_value
            .get("result")
            .and_then(|value| value.as_array())
            .expect("session list");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("path"), Some(&Value::String("/tmp/project".to_string())));
    }
}
