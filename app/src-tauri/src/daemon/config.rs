use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;
use tokio::fs;

/// Daemon connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
}

impl DaemonConfig {
    /// Get the config file path in the Tauri app data directory
    pub fn config_path(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
        let app_data = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data directory: {e}"))?;
        Ok(app_data.join("daemon.json"))
    }

    /// Load config from disk
    pub async fn load(app_handle: &tauri::AppHandle) -> Result<Option<Self>, String> {
        let path = Self::config_path(app_handle)?;
        if !path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read config: {e}"))?;
        let config: Self =
            serde_json::from_str(&contents).map_err(|e| format!("Invalid config: {e}"))?;
        Ok(Some(config))
    }

    /// Save config to disk
    pub async fn save(&self, app_handle: &tauri::AppHandle) -> Result<(), String> {
        let path = Self::config_path(app_handle)?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }

        let contents =
            serde_json::to_string_pretty(self).map_err(|e| format!("Failed to serialize: {e}"))?;
        fs::write(&path, contents)
            .await
            .map_err(|e| format!("Failed to write config: {e}"))?;
        Ok(())
    }
}
