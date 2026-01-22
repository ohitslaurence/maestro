use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::claude_sdk::ClaudeSdkServer;
use crate::opencode::OpenCodeServer;
use crate::protocol::SessionInfo;
use crate::terminal::TerminalHandle;

/// Server status for restart resilience (spec §3)
#[derive(Debug, Clone)]
pub enum ServerStatus {
    /// Process spawned, awaiting health check
    Starting,
    /// Health check passed (GET /health returns 200)
    Ready,
    /// Restart threshold exceeded
    Error(String),
}

/// Runtime tracking for Claude SDK server restart resilience (spec §3)
#[derive(Debug, Clone)]
pub struct ClaudeServerRuntime {
    pub workspace_id: String,
    pub port: u16,
    pub base_url: String,
    /// Consecutive failures; resets on successful Ready
    pub restart_count: u32,
    pub status: ServerStatus,
}

/// Unique client identifier
pub type ClientId = u64;

/// Daemon-wide shared state
pub struct DaemonState {
    /// Token for authentication (None if auth disabled)
    pub token: Option<String>,

    /// Configured sessions (path → SessionInfo)
    pub sessions: RwLock<HashMap<String, SessionInfo>>,

    /// Active terminals (sessionPath:terminalId → TerminalHandle)
    pub terminals: RwLock<HashMap<String, Arc<TerminalHandle>>>,

    /// Terminal ownership (terminalKey → ClientId)
    pub terminal_owners: RwLock<HashMap<String, ClientId>>,

    /// Client event senders (ClientId → sender)
    pub clients: RwLock<HashMap<ClientId, ClientSender>>,

    /// Next client ID counter
    next_client_id: Mutex<ClientId>,

    /// Active OpenCode servers (workspaceId → OpenCodeServer)
    pub opencode_servers: RwLock<HashMap<String, OpenCodeServer>>,

    /// Active Claude SDK servers (workspaceId → ClaudeSdkServer)
    pub claude_sdk_servers: RwLock<HashMap<String, ClaudeSdkServer>>,

    /// Claude SDK server runtime state for restart resilience (spec §3)
    pub claude_server_runtimes: RwLock<HashMap<String, ClaudeServerRuntime>>,
}

/// Channel for sending events to a client
pub type ClientSender = mpsc::UnboundedSender<String>;

impl DaemonState {
    pub fn new(token: Option<String>, sessions: Vec<SessionInfo>) -> Self {
        let sessions_map: HashMap<String, SessionInfo> = sessions
            .into_iter()
            .map(|s| (s.path.clone(), s))
            .collect();

        Self {
            token,
            sessions: RwLock::new(sessions_map),
            terminals: RwLock::new(HashMap::new()),
            terminal_owners: RwLock::new(HashMap::new()),
            clients: RwLock::new(HashMap::new()),
            next_client_id: Mutex::new(1),
            opencode_servers: RwLock::new(HashMap::new()),
            claude_sdk_servers: RwLock::new(HashMap::new()),
            claude_server_runtimes: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new client, returning its ID and event receiver
    pub async fn register_client(&self) -> (ClientId, mpsc::UnboundedReceiver<String>) {
        let mut id = self.next_client_id.lock().await;
        let client_id = *id;
        *id += 1;

        let (tx, rx) = mpsc::unbounded_channel();
        self.clients.write().await.insert(client_id, tx);

        (client_id, rx)
    }

    /// Unregister a client and clean up its terminals
    pub async fn unregister_client(&self, client_id: ClientId) {
        self.clients.write().await.remove(&client_id);

        // Find all terminals owned by this client
        let owned_terminals: Vec<String> = {
            let owners = self.terminal_owners.read().await;
            owners
                .iter()
                .filter(|(_, &owner)| owner == client_id)
                .map(|(key, _)| key.clone())
                .collect()
        };

        // Close each owned terminal
        for key in owned_terminals {
            self.close_terminal(&key).await;
        }
    }

    /// Get session info by path
    pub async fn get_session(&self, path: &str) -> Option<SessionInfo> {
        self.sessions.read().await.get(path).cloned()
    }

    /// List all sessions
    pub async fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// Terminal key format
    pub fn terminal_key(session_id: &str, terminal_id: &str) -> String {
        format!("{session_id}:{terminal_id}")
    }

    /// Store a terminal handle with ownership
    pub async fn store_terminal(
        &self,
        key: String,
        handle: Arc<TerminalHandle>,
        owner: ClientId,
    ) {
        self.terminals.write().await.insert(key.clone(), handle);
        self.terminal_owners.write().await.insert(key, owner);
    }

    /// Get a terminal handle
    pub async fn get_terminal(&self, key: &str) -> Option<Arc<TerminalHandle>> {
        self.terminals.read().await.get(key).cloned()
    }

    /// Check if terminal exists
    pub async fn terminal_exists(&self, key: &str) -> bool {
        self.terminals.read().await.contains_key(key)
    }

    /// Close a terminal and remove from state
    pub async fn close_terminal(&self, key: &str) {
        if let Some(handle) = self.terminals.write().await.remove(key) {
            handle.kill().await;
        }
        self.terminal_owners.write().await.remove(key);
    }

    /// Send an event to a specific client
    pub async fn send_to_client(&self, client_id: ClientId, msg: String) {
        if let Some(tx) = self.clients.read().await.get(&client_id) {
            let _ = tx.send(msg);
        }
    }

    /// Get terminal owner
    #[allow(dead_code)]
    pub async fn get_terminal_owner(&self, key: &str) -> Option<ClientId> {
        self.terminal_owners.read().await.get(key).copied()
    }

    /// Broadcast a message to all connected clients. Returns number of clients.
    pub async fn broadcast_to_all_clients(&self, msg: String) -> usize {
        let clients = self.clients.read().await;
        let count = clients.len();
        for tx in clients.values() {
            let _ = tx.send(msg.clone());
        }
        count
    }

    /// Store an OpenCode server
    pub async fn store_opencode_server(&self, workspace_id: String, server: OpenCodeServer) {
        self.opencode_servers.write().await.insert(workspace_id, server);
    }

    /// Get an OpenCode server by workspace ID
    pub async fn get_opencode_server(&self, workspace_id: &str) -> Option<String> {
        self.opencode_servers
            .read()
            .await
            .get(workspace_id)
            .map(|s| s.base_url.clone())
    }

    /// Check if an OpenCode server exists
    pub async fn has_opencode_server(&self, workspace_id: &str) -> bool {
        self.opencode_servers.read().await.contains_key(workspace_id)
    }

    /// Remove an OpenCode server (shuts it down)
    pub async fn remove_opencode_server(&self, workspace_id: &str) -> bool {
        if let Some(mut server) = self.opencode_servers.write().await.remove(workspace_id) {
            server.shutdown();
            true
        } else {
            false
        }
    }

    /// Store a Claude SDK server
    pub async fn store_claude_sdk_server(&self, workspace_id: String, server: ClaudeSdkServer) {
        self.claude_sdk_servers
            .write()
            .await
            .insert(workspace_id, server);
    }

    /// Get a Claude SDK server by workspace ID
    pub async fn get_claude_sdk_server(&self, workspace_id: &str) -> Option<String> {
        self.claude_sdk_servers
            .read()
            .await
            .get(workspace_id)
            .map(|s| s.base_url.clone())
    }

    /// Check if a Claude SDK server exists
    pub async fn has_claude_sdk_server(&self, workspace_id: &str) -> bool {
        self.claude_sdk_servers
            .read()
            .await
            .contains_key(workspace_id)
    }

    /// Remove a Claude SDK server (shuts it down)
    pub async fn remove_claude_sdk_server(&self, workspace_id: &str) -> bool {
        if let Some(mut server) = self
            .claude_sdk_servers
            .write()
            .await
            .remove(workspace_id)
        {
            server.shutdown();
            true
        } else {
            false
        }
    }

    /// Store or update Claude server runtime state (spec §3)
    pub async fn store_claude_server_runtime(&self, runtime: ClaudeServerRuntime) {
        self.claude_server_runtimes
            .write()
            .await
            .insert(runtime.workspace_id.clone(), runtime);
    }

    /// Get Claude server runtime by workspace ID (spec §4)
    pub async fn get_claude_server_runtime(&self, workspace_id: &str) -> Option<ClaudeServerRuntime> {
        self.claude_server_runtimes
            .read()
            .await
            .get(workspace_id)
            .cloned()
    }

    /// Update Claude server runtime status (spec §5)
    pub async fn update_claude_server_status(&self, workspace_id: &str, status: ServerStatus) {
        if let Some(runtime) = self.claude_server_runtimes.write().await.get_mut(workspace_id) {
            runtime.status = status;
        }
    }

    /// Update Claude server runtime base_url and port (spec §5 Edge Cases)
    pub async fn update_claude_server_url(&self, workspace_id: &str, port: u16, base_url: String) {
        if let Some(runtime) = self.claude_server_runtimes.write().await.get_mut(workspace_id) {
            runtime.port = port;
            runtime.base_url = base_url;
        }
    }

    /// Increment Claude server restart count (spec §5)
    pub async fn increment_claude_server_restart_count(&self, workspace_id: &str) -> u32 {
        if let Some(runtime) = self.claude_server_runtimes.write().await.get_mut(workspace_id) {
            runtime.restart_count += 1;
            runtime.restart_count
        } else {
            0
        }
    }

    /// Reset Claude server restart count on successful Ready (spec §3)
    pub async fn reset_claude_server_restart_count(&self, workspace_id: &str) {
        if let Some(runtime) = self.claude_server_runtimes.write().await.get_mut(workspace_id) {
            runtime.restart_count = 0;
        }
    }

    /// Remove Claude server runtime state
    pub async fn remove_claude_server_runtime(&self, workspace_id: &str) {
        self.claude_server_runtimes.write().await.remove(workspace_id);
    }
}
