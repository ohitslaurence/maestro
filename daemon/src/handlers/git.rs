use std::path::Path;

use crate::git;
use crate::protocol::*;
use crate::state::DaemonState;

pub async fn handle_status(request: &Request, state: &DaemonState) -> String {
    let params: SessionIdParams = match serde_json::from_value(request.params.clone()) {
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

    let path = Path::new(&params.session_id);
    match git::get_status(path) {
        Ok(result) => {
            let resp = SuccessResponse::new(request.id, result);
            serde_json::to_string(&resp).unwrap()
        }
        Err(e) => {
            let resp = ErrorResponse::new(request.id, GIT_ERROR, e);
            serde_json::to_string(&resp).unwrap()
        }
    }
}

pub async fn handle_diff(request: &Request, state: &DaemonState) -> String {
    let params: SessionIdParams = match serde_json::from_value(request.params.clone()) {
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

    let path = Path::new(&params.session_id);
    match git::get_diff(path) {
        Ok(result) => {
            let resp = SuccessResponse::new(request.id, result);
            serde_json::to_string(&resp).unwrap()
        }
        Err(e) => {
            let resp = ErrorResponse::new(request.id, GIT_ERROR, e);
            serde_json::to_string(&resp).unwrap()
        }
    }
}

pub async fn handle_log(request: &Request, state: &DaemonState) -> String {
    let params: GitLogParams = match serde_json::from_value(request.params.clone()) {
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

    let path = Path::new(&params.session_id);
    let limit = params.limit.unwrap_or(40);

    match git::get_log(path, limit) {
        Ok(result) => {
            let resp = SuccessResponse::new(request.id, result);
            serde_json::to_string(&resp).unwrap()
        }
        Err(e) => {
            let resp = ErrorResponse::new(request.id, GIT_ERROR, e);
            serde_json::to_string(&resp).unwrap()
        }
    }
}
