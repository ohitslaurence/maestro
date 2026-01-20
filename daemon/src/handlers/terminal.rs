use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use crate::protocol::*;
use crate::state::{ClientId, DaemonState};
use crate::terminal::TerminalHandle;

pub async fn handle_open(
    request: &Request,
    state: Arc<DaemonState>,
    client_id: ClientId,
) -> String {
    let params: TerminalOpenParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    // Validate session exists
    if state.get_session(&params.session_id).await.is_none() {
        let resp = ErrorResponse::new(
            request.id,
            SESSION_NOT_FOUND,
            format!("Session not found: {}", params.session_id),
        );
        return serde_json::to_string(&resp).unwrap();
    }

    let key = DaemonState::terminal_key(&params.session_id, &params.terminal_id);

    // Check if terminal already exists
    if state.terminal_exists(&key).await {
        let resp = ErrorResponse::new(
            request.id,
            TERMINAL_EXISTS,
            format!("Terminal already exists: {}", params.terminal_id),
        );
        return serde_json::to_string(&resp).unwrap();
    }

    // Open PTY
    let cwd = PathBuf::from(&params.session_id);
    let (handle, reader) = match TerminalHandle::open(
        params.terminal_id.clone(),
        &cwd,
        params.cols,
        params.rows,
    ) {
        Ok(h) => h,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INTERNAL_ERROR, e);
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let handle = Arc::new(handle);

    // Double-check for race condition before storing
    if state.terminal_exists(&key).await {
        handle.kill().await;
        let resp = ErrorResponse::new(
            request.id,
            TERMINAL_EXISTS,
            format!("Terminal already exists: {}", params.terminal_id),
        );
        return serde_json::to_string(&resp).unwrap();
    }

    state.store_terminal(key.clone(), handle, client_id).await;

    // Spawn reader thread to stream output to owning client
    spawn_terminal_reader(
        reader,
        state,
        client_id,
        params.session_id,
        params.terminal_id.clone(),
        key,
    );

    let resp = SuccessResponse::new(request.id, TerminalOpenResult {
        terminal_id: params.terminal_id,
    });
    serde_json::to_string(&resp).unwrap()
}

fn spawn_terminal_reader(
    mut reader: Box<dyn std::io::Read + Send>,
    state: Arc<DaemonState>,
    owner_client_id: ClientId,
    session_id: String,
    terminal_id: String,
    key: String,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let data = String::from_utf8_lossy(&buffer[..count]).to_string();

                    // Create terminal_output event
                    let event = Event::new(
                        EVENT_TERMINAL_OUTPUT,
                        TerminalOutputParams {
                            session_id: session_id.clone(),
                            terminal_id: terminal_id.clone(),
                            data,
                        },
                    );
                    let json = serde_json::to_string(&event).unwrap();

                    // Send to owning client
                    rt.block_on(state.send_to_client(owner_client_id, json));
                }
                Err(_) => break,
            }
        }

        // Terminal exited
        let exit_code = rt.block_on(async {
            if let Some(handle) = state.get_terminal(&key).await {
                handle.try_wait().await.flatten()
            } else {
                None
            }
        });

        // Send terminal_exited event
        let event = Event::new(
            EVENT_TERMINAL_EXITED,
            TerminalExitedParams {
                session_id: session_id.clone(),
                terminal_id: terminal_id.clone(),
                exit_code,
            },
        );
        let json = serde_json::to_string(&event).unwrap();
        rt.block_on(state.send_to_client(owner_client_id, json));

        // Clean up terminal
        rt.block_on(state.close_terminal(&key));
    });
}

pub async fn handle_write(request: &Request, state: &DaemonState) -> String {
    let params: TerminalWriteParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let key = DaemonState::terminal_key(&params.session_id, &params.terminal_id);

    match state.get_terminal(&key).await {
        Some(handle) => {
            if let Err(e) = handle.write(params.data.as_bytes()).await {
                let resp = ErrorResponse::new(request.id, INTERNAL_ERROR, e);
                return serde_json::to_string(&resp).unwrap();
            }
            let resp = SuccessResponse::new(request.id, serde_json::json!({}));
            serde_json::to_string(&resp).unwrap()
        }
        None => {
            let resp = ErrorResponse::new(
                request.id,
                TERMINAL_NOT_FOUND,
                format!("Terminal not found: {}", params.terminal_id),
            );
            serde_json::to_string(&resp).unwrap()
        }
    }
}

pub async fn handle_resize(request: &Request, state: &DaemonState) -> String {
    let params: TerminalResizeParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let key = DaemonState::terminal_key(&params.session_id, &params.terminal_id);

    match state.get_terminal(&key).await {
        Some(handle) => {
            if let Err(e) = handle.resize(params.cols, params.rows).await {
                let resp = ErrorResponse::new(request.id, INTERNAL_ERROR, e);
                return serde_json::to_string(&resp).unwrap();
            }
            let resp = SuccessResponse::new(request.id, serde_json::json!({}));
            serde_json::to_string(&resp).unwrap()
        }
        None => {
            let resp = ErrorResponse::new(
                request.id,
                TERMINAL_NOT_FOUND,
                format!("Terminal not found: {}", params.terminal_id),
            );
            serde_json::to_string(&resp).unwrap()
        }
    }
}

pub async fn handle_close(request: &Request, state: &DaemonState) -> String {
    let params: TerminalCloseParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let key = DaemonState::terminal_key(&params.session_id, &params.terminal_id);

    if !state.terminal_exists(&key).await {
        let resp = ErrorResponse::new(
            request.id,
            TERMINAL_NOT_FOUND,
            format!("Terminal not found: {}", params.terminal_id),
        );
        return serde_json::to_string(&resp).unwrap();
    }

    state.close_terminal(&key).await;

    let resp = SuccessResponse::new(request.id, serde_json::json!({}));
    serde_json::to_string(&resp).unwrap()
}
