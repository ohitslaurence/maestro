//! Claude SDK RPC handlers

use std::sync::Arc;

use serde_json::json;
use tracing::{error, info};

use crate::claude_sdk::ClaudeSdkServer;
use crate::opencode::OpenCodeRegistry;
use crate::protocol::*;
use crate::state::{ClaudeServerRuntime, DaemonState, ServerStatus};

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
    let port = server.port;

    // Create runtime state with Starting status (spec §5 step 2)
    let runtime = ClaudeServerRuntime {
        workspace_id: params.workspace_id.clone(),
        port,
        base_url: base_url.clone(),
        restart_count: 0,
        status: ServerStatus::Starting,
    };
    state.store_claude_server_runtime(runtime).await;

    // Start health-check polling instead of SSE bridge directly (spec §5 step 3)
    // SSE bridge will be started by health-check once server is Ready
    server.start_health_check(state.clone());
    server.start_process_monitor(state.clone());

    state
        .store_claude_sdk_server(params.workspace_id.clone(), server)
        .await;

    info!(
        "Claude SDK workspace {} spawned at {} (awaiting health check)",
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
    let params: ClaudeSdkSessionPromptParams =
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
    // Build request body per composer-options spec §4: SendMessageRequest
    let mut body = json!({
        "parts": [{
            "type": "text",
            "text": params.message
        }]
    });
    // Add maxThinkingTokens if provided (per-message override)
    if let Some(tokens) = params.max_thinking_tokens {
        body["maxThinkingTokens"] = json!(tokens);
    }

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

/// Handle claude_sdk_models request (composer-options spec §4)
pub async fn handle_models(request: &Request, state: &DaemonState) -> String {
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

    match OpenCodeRegistry::proxy_get(&base_url, "/models", None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_permission_reply request (dynamic-tool-approvals spec §4)
pub async fn handle_permission_reply(request: &Request, state: &DaemonState) -> String {
    let params: ClaudeSdkPermissionReplyParams =
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

    let path = format!("/permission/{}/reply", params.request_id);
    let mut body = json!({ "reply": params.reply });
    if let Some(msg) = params.message {
        body["message"] = json!(msg);
    }

    match OpenCodeRegistry::proxy_post(&base_url, &path, Some(body), None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_permission_pending request (dynamic-tool-approvals spec §4)
pub async fn handle_permission_pending(request: &Request, state: &DaemonState) -> String {
    let params: ClaudeSdkPermissionPendingParams =
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

    // Build path with optional session_id query parameter
    let path = match params.session_id {
        Some(sid) => format!("/permission/pending?sessionId={}", sid),
        None => "/permission/pending".to_string(),
    };

    match OpenCodeRegistry::proxy_get(&base_url, &path, None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}

/// Handle claude_sdk_session_settings_update request (session-settings spec §4)
pub async fn handle_session_settings_update(request: &Request, state: &DaemonState) -> String {
    let params: ClaudeSdkSessionSettingsUpdateParams =
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

    // PATCH /session/:id/settings per session-settings spec §4.1
    let path = format!("/session/{}/settings", params.session_id);
    let body = json!({ "settings": params.settings });

    match OpenCodeRegistry::proxy_patch(&base_url, &path, Some(body), None).await {
        Ok(result) => serde_json::to_string(&SuccessResponse::new(request.id, result)).unwrap(),
        Err(e) => {
            serde_json::to_string(&ErrorResponse::new(request.id, CLAUDE_SDK_ERROR, e)).unwrap()
        }
    }
}
