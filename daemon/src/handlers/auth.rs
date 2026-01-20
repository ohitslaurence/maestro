use crate::protocol::*;
use crate::state::DaemonState;

pub async fn handle(request: &Request, state: &DaemonState) -> String {
    let params: AuthParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            let resp = ErrorResponse::new(request.id, INVALID_PARAMS, format!("Invalid params: {e}"));
            return serde_json::to_string(&resp).unwrap();
        }
    };

    // Check token
    match &state.token {
        Some(expected) if params.token == *expected => {
            let resp = SuccessResponse::new(request.id, AuthResult { ok: true });
            serde_json::to_string(&resp).unwrap()
        }
        Some(_) => {
            let resp = ErrorResponse::new(request.id, AUTH_FAILED, "Invalid token");
            serde_json::to_string(&resp).unwrap()
        }
        None => {
            // Auth not required, always succeed
            let resp = SuccessResponse::new(request.id, AuthResult { ok: true });
            serde_json::to_string(&resp).unwrap()
        }
    }
}
