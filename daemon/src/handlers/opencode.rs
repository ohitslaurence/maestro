//! OpenCode RPC handlers

use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};

use crate::opencode::{OpenCodeRegistry, OpenCodeServer};
use crate::protocol::*;
use crate::state::DaemonState;

/// Handle opencode_connect_workspace request
pub async fn handle_connect(request: &Request, state: Arc<DaemonState>) -> String {
    let params: OpenCodeConnectParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    // Check if already connected
    if state.has_opencode_server(&params.workspace_id).await {
        if let Some(base_url) = state.get_opencode_server(&params.workspace_id).await {
            return serde_json::to_string(&SuccessResponse::new(
                request.id,
                OpenCodeConnectResult {
                    workspace_id: params.workspace_id,
                    base_url,
                },
            ))
            .unwrap();
        }
    }

    // Spawn OpenCode server
    let mut server = match OpenCodeServer::spawn(
        params.workspace_id.clone(),
        params.workspace_path.clone(),
    ) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to spawn OpenCode server: {e}");
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_ERROR,
                format!("Failed to spawn server: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = server.base_url.clone();

    // Start SSE bridge
    server.start_sse_bridge(state.clone());

    // Store server in state
    state
        .store_opencode_server(params.workspace_id.clone(), server)
        .await;

    info!(
        "OpenCode workspace {} connected at {}",
        params.workspace_id, base_url
    );

    serde_json::to_string(&SuccessResponse::new(
        request.id,
        OpenCodeConnectResult {
            workspace_id: params.workspace_id,
            base_url,
        },
    ))
    .unwrap()
}

/// Handle opencode_disconnect_workspace request
pub async fn handle_disconnect(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeWorkspaceParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let removed = state.remove_opencode_server(&params.workspace_id).await;

    if removed {
        info!("OpenCode workspace {} disconnected", params.workspace_id);
    }

    serde_json::to_string(&SuccessResponse::new(request.id, json!({"ok": removed}))).unwrap()
}

/// Handle opencode_status request
pub async fn handle_status(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeWorkspaceParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = state.get_opencode_server(&params.workspace_id).await;
    let connected = base_url.is_some();

    serde_json::to_string(&SuccessResponse::new(
        request.id,
        OpenCodeStatusResult {
            connected,
            base_url,
        },
    ))
    .unwrap()
}

/// Handle opencode_session_list request
pub async fn handle_session_list(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeWorkspaceParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = match state.get_opencode_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_NOT_CONNECTED,
                "OpenCode not connected for this workspace",
            ))
            .unwrap();
        }
    };

    match OpenCodeRegistry::proxy_get(&base_url, "/session", None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, OPENCODE_ERROR, e)).unwrap()
        }
    }
}

/// Handle opencode_session_create request
pub async fn handle_session_create(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeSessionCreateParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = match state.get_opencode_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_NOT_CONNECTED,
                "OpenCode not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let body = params.title.map(|t| json!({"title": t}));

    match OpenCodeRegistry::proxy_post(&base_url, "/session", body, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, OPENCODE_ERROR, e)).unwrap()
        }
    }
}

/// Handle opencode_session_prompt request
pub async fn handle_session_prompt(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeSessionPromptParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = match state.get_opencode_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_NOT_CONNECTED,
                "OpenCode not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let path = format!("/session/{}/message", params.session_id);
    // OpenCode expects parts array format, not simple content string
    let body = json!({
        "parts": [{
            "type": "text",
            "text": params.message
        }]
    });

    match OpenCodeRegistry::proxy_post(&base_url, &path, Some(body), None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, OPENCODE_ERROR, e)).unwrap()
        }
    }
}

/// Handle opencode_session_abort request
pub async fn handle_session_abort(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeSessionAbortParams = match serde_json::from_value(request.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                INVALID_PARAMS,
                format!("Invalid params: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = match state.get_opencode_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_NOT_CONNECTED,
                "OpenCode not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let path = format!("/session/{}/abort", params.session_id);

    match OpenCodeRegistry::proxy_post(&base_url, &path, None, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, OPENCODE_ERROR, e)).unwrap()
        }
    }
}

/// Handle opencode_session_messages request - fetch session history
pub async fn handle_session_messages(request: &Request, state: &DaemonState) -> String {
    let params: OpenCodeSessionMessagesParams =
        match serde_json::from_value(request.params.clone()) {
            Ok(p) => p,
            Err(e) => {
                return serde_json::to_string(&ErrorResponse::new(
                    request.id,
                    INVALID_PARAMS,
                    format!("Invalid params: {e}"),
                ))
                .unwrap();
            }
        };

    let base_url = match state.get_opencode_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                OPENCODE_NOT_CONNECTED,
                "OpenCode not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let path = format!("/session/{}/message", params.session_id);

    match OpenCodeRegistry::proxy_get(&base_url, &path, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, OPENCODE_ERROR, e)).unwrap()
        }
    }
}
