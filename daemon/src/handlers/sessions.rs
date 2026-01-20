use std::path::Path;

use crate::git;
use crate::protocol::*;
use crate::state::DaemonState;

pub async fn handle_list(request: &Request, state: &DaemonState) -> String {
    let sessions = state.list_sessions().await;
    let resp = SuccessResponse::new(request.id, sessions);
    serde_json::to_string(&resp).unwrap()
}

pub async fn handle_info(request: &Request, state: &DaemonState) -> String {
    let params: SessionIdParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    match state.get_session(&params.session_id).await {
        Some(session) => {
            let path = Path::new(&session.path);
            let has_git = git::is_git_repo(path);

            let result = SessionInfoResult {
                path: session.path,
                name: session.name,
                has_git,
            };
            let resp = SuccessResponse::new(request.id, result);
            serde_json::to_string(&resp).unwrap()
        }
        None => {
            let resp = ErrorResponse::new(
                request.id,
                SESSION_NOT_FOUND,
                format!("Session not found: {}", params.session_id),
            );
            serde_json::to_string(&resp).unwrap()
        }
    }
}
