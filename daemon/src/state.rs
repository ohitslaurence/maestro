use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

use crate::opencode::OpenCodeServer;
use crate::protocol::SessionInfo;
use crate::terminal::TerminalHandle;

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

    /// Broadcast a message to all connected clients
    pub async fn broadcast_to_all_clients(&self, msg: String) {
        let clients = self.clients.read().await;
        for tx in clients.values() {
            let _ = tx.send(msg.clone());
        }
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
}
