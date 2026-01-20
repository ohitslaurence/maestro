use tauri::Manager;

mod sessions;
mod terminal;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                let _ = fix_path_env::fix();
            }

            // Get the main window and set up
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    use tauri::TitleBarStyle;
                    let _ = window.set_title_bar_style(TitleBarStyle::Overlay);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            sessions::list_sessions,
            sessions::spawn_session,
            sessions::stop_session,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
            sessions::get_git_status,
            sessions::get_git_diffs,
            sessions::get_git_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
