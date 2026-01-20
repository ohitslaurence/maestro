//! Claude SDK RPC handlers

use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};

use crate::claude_sdk::ClaudeSdkServer;
use crate::opencode::OpenCodeRegistry;
use crate::protocol::*;
use crate::state::DaemonState;

/// Handle claude_sdk_connect_workspace request
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

    if state.has_claude_sdk_server(&params.workspace_id).await {
        if let Some(base_url) = state.get_claude_sdk_server(&params.workspace_id).await {
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

    let mut server = match ClaudeSdkServer::spawn(
        params.workspace_id.clone(),
        params.workspace_path.clone(),
    ) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to spawn Claude SDK server: {e}");
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                CLAUDE_SDK_ERROR,
                format!("Failed to spawn server: {e}"),
            ))
            .unwrap();
        }
    };

    let base_url = server.base_url.clone();

    server.start_sse_bridge(state.clone());

    state
        .store_claude_sdk_server(params.workspace_id.clone(), server)
        .await;

    info!(
        "Claude SDK workspace {} connected at {}",
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

/// Handle claude_sdk_disconnect_workspace request
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

    let removed = state.remove_claude_sdk_server(&params.workspace_id).await;

    if removed {
        info!("Claude SDK workspace {} disconnected", params.workspace_id);
    }

    serde_json::to_string(&SuccessResponse::new(request.id, json!({"ok": removed}))).unwrap()
}

/// Handle claude_sdk_status request
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

    let base_url = state.get_claude_sdk_server(&params.workspace_id).await;
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

/// Handle claude_sdk_session_list request
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

    let base_url = match state.get_claude_sdk_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                CLAUDE_SDK_NOT_CONNECTED,
                "Claude SDK not connected for this workspace",
            ))
            .unwrap();
        }
    };

    match OpenCodeRegistry::proxy_get(&base_url, "/session", None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_session_create request
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

    let base_url = match state.get_claude_sdk_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                CLAUDE_SDK_NOT_CONNECTED,
                "Claude SDK not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let body = params.title.map(|t| json!({"title": t}));

    match OpenCodeRegistry::proxy_post(&base_url, "/session", body, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_session_prompt request
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

    let base_url = match state.get_claude_sdk_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                CLAUDE_SDK_NOT_CONNECTED,
                "Claude SDK not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let path = format!("/session/{}/message", params.session_id);
    let body = json!({
        "parts": [{
            "type": "text",
            "text": params.message
        }]
    });

    match OpenCodeRegistry::proxy_post(&base_url, &path, Some(body), None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_session_abort request
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

    let base_url = match state.get_claude_sdk_server(&params.workspace_id).await {
        Some(url) => url,
        None => {
            return serde_json::to_string(&ErrorResponse::new(
                request.id,
                CLAUDE_SDK_NOT_CONNECTED,
                "Claude SDK not connected for this workspace",
            ))
            .unwrap();
        }
    };

    let path = format!("/session/{}/abort", params.session_id);

    match OpenCodeRegistry::proxy_post(&base_url, &path, None, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}
