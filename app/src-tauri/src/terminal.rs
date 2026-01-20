use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, LazyLock};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

static TERMINAL_SESSIONS: LazyLock<Mutex<HashMap<String, Arc<TerminalSession>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct TerminalSession {
    pub(crate) id: String,
    pub(crate) master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    pub(crate) writer: Mutex<Box<dyn Write + Send>>,
    pub(crate) child: Mutex<Box<dyn portable_pty::Child + Send>>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TerminalSessionInfo {
    id: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TerminalOutputPayload {
    session_id: String,
    terminal_id: String,
    data: String,
}

fn terminal_key(session_id: &str, terminal_id: &str) -> String {
    format!("{session_id}:{terminal_id}")
}

fn shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}

fn spawn_terminal_reader(
    app: AppHandle,
    session_id: String,
    terminal_id: String,
    mut reader: Box<dyn Read + Send>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let data = String::from_utf8_lossy(&buffer[..count]).to_string();
                    let payload = TerminalOutputPayload {
                        session_id: session_id.clone(),
                        terminal_id: terminal_id.clone(),
                        data,
                    };
                    let _ = app.emit("terminal-output", payload);
                }
                Err(_) => break,
            }
        }
    });
}

#[tauri::command]
pub(crate) async fn terminal_open(
    session_id: String,
    terminal_id: String,
    cols: u16,
    rows: u16,
    app: AppHandle,
) -> Result<TerminalSessionInfo, String> {
    if terminal_id.is_empty() {
        return Err("Terminal id is required".to_string());
    }
    let key = terminal_key(&session_id, &terminal_id);

    // Check if session already exists
    {
        let sessions = TERMINAL_SESSIONS.lock().await;
        if let Some(existing) = sessions.get(&key) {
            return Ok(TerminalSessionInfo {
                id: existing.id.clone(),
            });
        }
    }

    // Get CWD - use session_id as path for now (caller provides project path)
    let cwd = std::path::PathBuf::from(&session_id);

    let pty_system = native_pty_system();
    let size = PtySize {
        rows: rows.max(2),
        cols: cols.max(2),
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = pty_system
        .openpty(size)
        .map_err(|e| format!("Failed to open pty: {e}"))?;

    let mut cmd = CommandBuilder::new(shell_path());
    cmd.cwd(cwd);
    cmd.arg("-i");
    cmd.env("TERM", "xterm-256color");

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell: {e}"))?;
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to open pty reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to open pty writer: {e}"))?;

    let session = Arc::new(TerminalSession {
        id: terminal_id.clone(),
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
    });
    let session_info_id = session.id.clone();

    // Insert session, handling race condition
    {
        let mut sessions = TERMINAL_SESSIONS.lock().await;
        if let Some(existing) = sessions.get(&key) {
            // Another task created it first, kill our new child
            let mut child = session.child.lock().await;
            let _ = child.kill();
            return Ok(TerminalSessionInfo {
                id: existing.id.clone(),
            });
        }
        sessions.insert(key, session);
    }

    spawn_terminal_reader(app, session_id, terminal_id, reader);

    Ok(TerminalSessionInfo {
        id: session_info_id,
    })
}

#[tauri::command]
pub(crate) async fn terminal_write(
    session_id: String,
    terminal_id: String,
    data: String,
) -> Result<(), String> {
    let key = terminal_key(&session_id, &terminal_id);
    let sessions = TERMINAL_SESSIONS.lock().await;
    let session = sessions
        .get(&key)
        .ok_or_else(|| "Terminal session not found".to_string())?;
    let mut writer = session.writer.lock().await;
    writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed to write to pty: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush pty: {e}"))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn terminal_resize(
    session_id: String,
    terminal_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let key = terminal_key(&session_id, &terminal_id);
    let sessions = TERMINAL_SESSIONS.lock().await;
    let session = sessions
        .get(&key)
        .ok_or_else(|| "Terminal session not found".to_string())?;
    let size = PtySize {
        rows: rows.max(2),
        cols: cols.max(2),
        pixel_width: 0,
        pixel_height: 0,
    };
    let master = session.master.lock().await;
    master
        .resize(size)
        .map_err(|e| format!("Failed to resize pty: {e}"))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn terminal_close(
    session_id: String,
    terminal_id: String,
) -> Result<(), String> {
    let key = terminal_key(&session_id, &terminal_id);
    let mut sessions = TERMINAL_SESSIONS.lock().await;
    let session = sessions
        .remove(&key)
        .ok_or_else(|| "Terminal session not found".to_string())?;
    let mut child = session.child.lock().await;
    let _ = child.kill();
    Ok(())
}
