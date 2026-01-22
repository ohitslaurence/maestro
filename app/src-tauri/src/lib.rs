use std::sync::Arc;

use tauri::Emitter;
#[cfg(target_os = "macos")]
use tauri::Manager;

mod agent_state;
mod daemon;
mod hooks;
mod sessions;
mod storage;
mod terminal;
mod tools;

use daemon::DaemonConfig;

// Re-export agent state event emission functions (§4)
pub use agent_state::{
    emit_hook_lifecycle, emit_session_error, emit_state_changed, emit_tool_lifecycle,
    AgentStateEvent, AgentStateEventEnvelope, AgentStateKind, HookRunRecord, StateChangeReason,
    ToolRunRecord, AGENT_STATE_EVENT_CHANNEL,
};

// Re-export hook runner types for orchestration (§5)
pub use hooks::{load_hooks_config, run_post_tool_hooks, HookPipelineResult};

// Re-export streaming event types and emission (§4 streaming-event-schema spec)
pub use sessions::{
    StreamEvent, StreamEventPayload, StreamEventType, STREAM_EVENT_CHANNEL,
    STREAM_SCHEMA_VERSION,
    // Payload types
    TextDeltaPayload, ToolCallDeltaPayload, ToolCallCompletedPayload, CompletedPayload,
    ErrorPayload, StatusPayload, ThinkingDeltaPayload, ArtifactDeltaPayload, MetadataPayload,
    // Enum types
    CompletionReason, StreamErrorCode, ToolCallStatus, AgentProcessingState, TokenUsage,
};

// Re-export storage types and commands (§4, §5 session-persistence spec)
pub use storage::{
    ThreadRecord, ThreadSummary, ThreadPrivacy, ThreadMetadata, ThreadIndex,
    SessionRecord, SessionStatus, SessionAgentConfig, SessionToolRun, SessionToolRunStatus,
    MessageRecord, MessageRole, MESSAGE_SCHEMA_VERSION, INDEX_SCHEMA_VERSION,
    ResumeResult, SessionResumedPayload, SESSION_RESUMED_EVENT,
    list_threads, load_thread, save_thread, delete_thread, create_session, mark_session_ended,
    append_message, list_messages, rebuild_index, resume_thread,
};

/// Emit a streaming event to the frontend via Tauri's event system.
///
/// Per spec §4 (streaming-event-schema.md):
/// - Channel: `agent:stream_event`
/// - Emits the `StreamEvent` envelope defined in §3.
///
/// This is the primary emission point for all streaming events from harness adapters.
pub fn emit_stream_event<R: tauri::Runtime>(app: &tauri::AppHandle<R>, event: &StreamEvent) {
    let _ = app.emit(STREAM_EVENT_CHANNEL, event);
}

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
            // Claude SDK commands (proxied to daemon, spec §4)
            daemon::claude_sdk_connect_workspace,
            daemon::claude_sdk_disconnect_workspace,
            daemon::claude_sdk_status,
            daemon::claude_sdk_session_list,
            daemon::claude_sdk_session_create,
            daemon::claude_sdk_session_prompt,
            daemon::claude_sdk_session_abort,
            daemon::claude_sdk_models,
            // Claude SDK permission commands (dynamic-tool-approvals spec §4)
            daemon::claude_sdk_permission_reply,
            daemon::claude_sdk_permission_pending,
            // Local-only commands (agent harness - future)
            sessions::spawn_session,
            sessions::stop_session,
            // Storage commands (session-persistence §4, §5)
            storage::list_threads,
            storage::load_thread,
            storage::save_thread,
            storage::delete_thread,
            storage::create_session,
            storage::mark_session_ended,
            storage::append_message,
            storage::list_messages,
            storage::rebuild_index,
            storage::resume_thread,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
