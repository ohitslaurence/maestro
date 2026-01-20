use clap::Parser;
use serde::Deserialize;
use std::path::PathBuf;

use crate::protocol::SessionInfo;

/// Maestro daemon - remote terminal and git operations
#[derive(Parser, Debug)]
#[command(name = "maestro-daemon")]
pub struct Args {
    /// Bind address
    #[arg(long, default_value = "127.0.0.1:4733")]
    pub listen: String,

    /// Auth token (or set MAESTRO_DAEMON_TOKEN env var)
    #[arg(long, env = "MAESTRO_DAEMON_TOKEN")]
    pub token: Option<String>,

    /// Data directory
    #[arg(long, env = "MAESTRO_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    /// Disable auth (dev only)
    #[arg(long)]
    pub insecure_no_auth: bool,
}

impl Args {
    pub fn data_dir(&self) -> PathBuf {
        self.data_dir
            .clone()
            .unwrap_or_else(|| dirs_data_dir().join("maestro"))
    }

    pub fn require_auth(&self) -> bool {
        !self.insecure_no_auth
    }
}

fn dirs_data_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".local/share"))
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

/// sessions.json format
#[derive(Debug, Deserialize)]
pub struct SessionsConfig {
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct SessionEntry {
    pub path: String,
    pub name: Option<String>,
}

impl SessionsConfig {
    pub fn load(data_dir: &PathBuf) -> Result<Self, String> {
        let path = data_dir.join("sessions.json");
        if !path.exists() {
            return Ok(SessionsConfig { sessions: vec![] });
        }

        let content =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read sessions.json: {e}"))?;

        serde_json::from_str(&content).map_err(|e| format!("Failed to parse sessions.json: {e}"))
    }

    pub fn to_session_infos(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|e| SessionInfo {
                path: e.path.clone(),
                name: e.name.clone().unwrap_or_else(|| {
                    PathBuf::from(&e.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| e.path.clone())
                }),
            })
            .collect()
    }
}
