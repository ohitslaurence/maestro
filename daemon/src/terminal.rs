use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use tokio::sync::Mutex;

/// Handle to an active terminal PTY
pub struct TerminalHandle {
    #[allow(dead_code)]
    terminal_id: String,
    master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send>>,
}

impl TerminalHandle {
    /// Open a new PTY in the given working directory
    pub fn open(
        terminal_id: String,
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<(Self, Box<dyn Read + Send>), String> {
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

        let shell = shell_path();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(cwd);
        cmd.arg("-i");
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

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

        let handle = Self {
            terminal_id,
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
        };

        Ok((handle, reader))
    }

    /// Write data to the terminal
    pub async fn write(&self, data: &[u8]) -> Result<(), String> {
        let mut writer = self.writer.lock().await;
        writer
            .write_all(data)
            .map_err(|e| format!("Failed to write to pty: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("Failed to flush pty: {e}"))?;
        Ok(())
    }

    /// Resize the terminal
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        let size = PtySize {
            rows: rows.max(2),
            cols: cols.max(2),
            pixel_width: 0,
            pixel_height: 0,
        };
        let master = self.master.lock().await;
        master
            .resize(size)
            .map_err(|e| format!("Failed to resize pty: {e}"))?;
        Ok(())
    }

    /// Kill the terminal process
    pub async fn kill(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill();
    }

    /// Try to get exit status (non-blocking)
    pub async fn try_wait(&self) -> Option<Option<i32>> {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(Some(status)) => Some(Some(status.exit_code() as i32)),
            Ok(None) => None,
            Err(_) => Some(None),
        }
    }
}

fn shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string())
}
