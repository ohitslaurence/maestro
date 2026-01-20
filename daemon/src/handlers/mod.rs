pub mod auth;
pub mod git;
pub mod opencode;
pub mod sessions;
pub mod terminal;

use std::sync::Arc;

use crate::protocol::*;
use crate::state::{ClientId, DaemonState};

/// Dispatch a request to the appropriate handler
pub async fn dispatch(
    request: &Request,
    state: Arc<DaemonState>,
    client_id: ClientId,
) -> String {
    match request.method.as_str() {
        METHOD_AUTH => auth::handle(request, &state).await,
        METHOD_LIST_SESSIONS => sessions::handle_list(request, &state).await,
        METHOD_SESSION_INFO => sessions::handle_info(request, &state).await,
        METHOD_TERMINAL_OPEN => terminal::handle_open(request, state, client_id).await,
        METHOD_TERMINAL_WRITE => terminal::handle_write(request, &state).await,
        METHOD_TERMINAL_RESIZE => terminal::handle_resize(request, &state).await,
        METHOD_TERMINAL_CLOSE => terminal::handle_close(request, &state).await,
        METHOD_GIT_STATUS => git::handle_status(request, &state).await,
        METHOD_GIT_DIFF => git::handle_diff(request, &state).await,
        METHOD_GIT_LOG => git::handle_log(request, &state).await,
        METHOD_OPENCODE_CONNECT_WORKSPACE => opencode::handle_connect(request, state).await,
        METHOD_OPENCODE_DISCONNECT_WORKSPACE => opencode::handle_disconnect(request, &state).await,
        METHOD_OPENCODE_STATUS => opencode::handle_status(request, &state).await,
        METHOD_OPENCODE_SESSION_LIST => opencode::handle_session_list(request, &state).await,
        METHOD_OPENCODE_SESSION_CREATE => opencode::handle_session_create(request, &state).await,
        METHOD_OPENCODE_SESSION_PROMPT => opencode::handle_session_prompt(request, &state).await,
        METHOD_OPENCODE_SESSION_ABORT => opencode::handle_session_abort(request, &state).await,
        _ => {
            let resp = ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Unknown method: {}", request.method),
            );
            serde_json::to_string(&resp).unwrap()
        }
    }
}
