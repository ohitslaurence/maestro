use std::sync::Arc;

#[cfg(target_os = "macos")]
use tauri::Manager;

mod daemon;
mod sessions;
mod terminal;

use daemon::DaemonConfig;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let daemon_state = Arc::new(daemon::client::DaemonState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .manage(daemon_state.clone())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            {
                let _ = fix_path_env::fix();
            }

            // Get the main window and set up
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                use tauri::TitleBarStyle;
                let _ = window.set_title_bar_style(TitleBarStyle::Overlay);
            }

            // Initialize daemon state
            let handle = app.handle().clone();
            let state = daemon_state.clone();
            tauri::async_runtime::spawn(async move {
                state.set_app_handle(handle.clone()).await;

                // Load config and auto-connect
                if let Ok(Some(config)) = DaemonConfig::load(&handle).await {
                    state.set_config(Some(config)).await;
                    let _ = state.connect().await;
                }

                // Start reconnect task
                daemon::client::spawn_reconnect_task(state);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Daemon connection commands
            daemon::daemon_connect,
            daemon::daemon_disconnect,
            daemon::daemon_status,
            daemon::daemon_configure,
            // Session commands (proxied to daemon)
            daemon::list_sessions,
            daemon::session_info,
            // Terminal commands (proxied to daemon)
            daemon::terminal_open,
            daemon::terminal_write,
            daemon::terminal_resize,
            daemon::terminal_close,
            // Git commands (proxied to daemon)
            daemon::git_status,
            daemon::git_diff,
            daemon::git_log,
            // OpenCode commands (proxied to daemon)
            daemon::opencode_connect_workspace,
            daemon::opencode_disconnect_workspace,
            daemon::opencode_status,
            daemon::opencode_session_list,
            daemon::opencode_session_create,
            daemon::opencode_session_prompt,
            daemon::opencode_session_abort,
            daemon::opencode_session_messages,
            // Local-only commands (agent harness - future)
            sessions::spawn_session,
            sessions::stop_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
