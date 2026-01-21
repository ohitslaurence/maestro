// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Analytics HTTP handlers.
//!
//! Implements event capture, identity resolution, and API key management endpoints.

use axum::{
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use loom_analytics_core::{OrgId as AnalyticsOrgId, UserId as AnalyticsUserId};
use loom_server_analytics::{
	alias_impl, batch_capture_impl, capture_event_impl, count_events_impl, create_api_key_impl,
	export_events_impl, get_person_by_distinct_id_impl, get_person_impl, identify_impl,
	list_api_keys_impl, list_events_impl, list_persons_impl, parse_key_type, revoke_api_key_impl,
	set_properties_impl, AnalyticsApiKeyContext, AnalyticsRepository, UserAuthContext,
};
pub use loom_server_api::analytics::{
	AliasRequest, AnalyticsApiKeyResponse, AnalyticsErrorResponse, AnalyticsKeyTypeApi,
	AnalyticsSuccessResponse, BatchCaptureRequest, CaptureEventRequest, CaptureResponse,
	CountEventsQuery, CountEventsResponse, CreateAnalyticsApiKeyRequest,
	CreateAnalyticsApiKeyResponse, EventResponse, ExportEventsRequest, ExportEventsResponse,
	IdentifyRequest, IdentifyResponse, ListAnalyticsApiKeysResponse, ListEventsQuery,
	ListEventsResponse, ListPersonsQuery, ListPersonsResponse, PersonResponse, SetPropertiesRequest,
};

use crate::{
	api::AppState,
	api_response::{internal_error, not_found},
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	impl_api_error_response, parse_id,
	validation::parse_org_id as shared_parse_org_id,
};
use loom_server_analytics::{MergeAuditHook, PersonMergeDetails};
use loom_server_audit::{AuditEventType, AuditLogBuilder, AuditService, UserId as AuditUserId};
use std::sync::Arc;

impl_api_error_response!(AnalyticsErrorResponse);

// ============================================================================
// Audit Hook Implementation
// ============================================================================

/// Audit hook that logs person merge events to the audit service.
pub struct AnalyticsMergeAuditHook {
	audit_service: Arc<AuditService>,
}

impl AnalyticsMergeAuditHook {
	/// Creates a new audit hook that logs to the given audit service.
	pub fn new(audit_service: Arc<AuditService>) -> Self {
		Self { audit_service }
	}
}

impl MergeAuditHook for AnalyticsMergeAuditHook {
	fn on_merge(&self, details: PersonMergeDetails) {
		self.audit_service.log(
			AuditLogBuilder::new(AuditEventType::AnalyticsPersonMerged)
				.resource("analytics_person", details.winner_id.to_string())
				.details(serde_json::json!({
					"org_id": details.org_id.0.to_string(),
					"winner_id": details.winner_id.to_string(),
					"loser_id": details.loser_id.to_string(),
					"reason": format!("{:?}", details.reason),
					"events_reassigned": details.events_reassigned,
					"identities_transferred": details.identities_transferred,
				}))
				.build(),
		);
	}
}

// ============================================================================
// SDK Routes (API Key Auth)
// ============================================================================

/// Capture a single analytics event.
#[utoipa::path(
    post,
    path = "/api/analytics/capture",
    request_body = CaptureEventRequest,
    responses(
        (status = 200, description = "Event captured successfully", body = CaptureResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload), fields(event = %payload.event, distinct_id = %payload.distinct_id))]
pub async fn capture_event(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<CaptureEventRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	capture_event_impl(analytics_state, api_key_ctx, headers, payload)
		.await
		.into_response()
}

/// Capture multiple analytics events in a batch.
#[utoipa::path(
    post,
    path = "/api/analytics/batch",
    request_body = BatchCaptureRequest,
    responses(
        (status = 200, description = "Events captured successfully", body = CaptureResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload), fields(batch_size = payload.batch.len()))]
pub async fn batch_capture(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<BatchCaptureRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	batch_capture_impl(analytics_state, api_key_ctx, headers, payload)
		.await
		.into_response()
}

/// Identify a user by linking a distinct_id to a user_id.
#[utoipa::path(
    post,
    path = "/api/analytics/identify",
    request_body = IdentifyRequest,
    responses(
        (status = 200, description = "User identified successfully", body = IdentifyResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload), fields(distinct_id = %payload.distinct_id, user_id = %payload.user_id))]
pub async fn identify(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<IdentifyRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	identify_impl(analytics_state, api_key_ctx, payload)
		.await
		.into_response()
}

/// Create an alias linking two distinct_ids.
#[utoipa::path(
    post,
    path = "/api/analytics/alias",
    request_body = AliasRequest,
    responses(
        (status = 200, description = "Alias created successfully", body = IdentifyResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload), fields(distinct_id = %payload.distinct_id, alias = %payload.alias))]
pub async fn alias(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<AliasRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	alias_impl(analytics_state, api_key_ctx, payload)
		.await
		.into_response()
}

/// Set properties on a person.
#[utoipa::path(
    post,
    path = "/api/analytics/set",
    request_body = SetPropertiesRequest,
    responses(
        (status = 200, description = "Properties set successfully", body = IdentifyResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload), fields(distinct_id = %payload.distinct_id))]
pub async fn set_properties(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<SetPropertiesRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	set_properties_impl(analytics_state, api_key_ctx, payload)
		.await
		.into_response()
}

// ============================================================================
// Query Routes (ReadWrite API Key Auth)
// The impl functions check for ReadWrite key internally
// ============================================================================

/// List persons in an organization (requires ReadWrite API key).
#[utoipa::path(
    get,
    path = "/api/analytics/persons",
    params(
        ListPersonsQuery
    ),
    responses(
        (status = 200, description = "List of persons", body = ListPersonsResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers))]
pub async fn list_persons(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<ListPersonsQuery>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	list_persons_impl(analytics_state, api_key_ctx, query)
		.await
		.into_response()
}

/// Get a specific person by ID (requires ReadWrite API key).
#[utoipa::path(
    get,
    path = "/api/analytics/persons/{person_id}",
    params(
        ("person_id" = String, Path, description = "Person ID")
    ),
    responses(
        (status = 200, description = "Person details", body = PersonResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 404, description = "Person not found", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers), fields(%person_id))]
pub async fn get_person(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(person_id): Path<String>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	get_person_impl(analytics_state, api_key_ctx, person_id)
		.await
		.into_response()
}

/// Get a person by distinct_id (requires ReadWrite API key).
#[utoipa::path(
    get,
    path = "/api/analytics/persons/by-distinct-id/{distinct_id}",
    params(
        ("distinct_id" = String, Path, description = "Distinct ID")
    ),
    responses(
        (status = 200, description = "Person details", body = PersonResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 404, description = "Person not found", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers), fields(%distinct_id))]
pub async fn get_person_by_distinct_id(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(distinct_id): Path<String>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	get_person_by_distinct_id_impl(analytics_state, api_key_ctx, distinct_id)
		.await
		.into_response()
}

/// List events (requires ReadWrite API key).
#[utoipa::path(
    get,
    path = "/api/analytics/events",
    params(
        ListEventsQuery
    ),
    responses(
        (status = 200, description = "List of events", body = ListEventsResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers))]
pub async fn list_events(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<ListEventsQuery>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	list_events_impl(analytics_state, api_key_ctx, query)
		.await
		.into_response()
}

/// Count events (requires ReadWrite API key).
#[utoipa::path(
    get,
    path = "/api/analytics/events/count",
    params(
        CountEventsQuery
    ),
    responses(
        (status = 200, description = "Event count", body = CountEventsResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers))]
pub async fn count_events(
	State(state): State<AppState>,
	headers: HeaderMap,
	Query(query): Query<CountEventsQuery>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	count_events_impl(analytics_state, api_key_ctx, query)
		.await
		.into_response()
}

/// Export events (requires ReadWrite API key).
#[utoipa::path(
    post,
    path = "/api/analytics/events/export",
    request_body = ExportEventsRequest,
    responses(
        (status = 200, description = "Exported events", body = ExportEventsResponse),
        (status = 401, description = "Invalid API key", body = AnalyticsErrorResponse),
        (status = 403, description = "Read access required", body = AnalyticsErrorResponse),
        (status = 500, description = "Internal error", body = AnalyticsErrorResponse)
    ),
    tag = "analytics",
    security(
        ("api_key" = [])
    )
)]
#[tracing::instrument(skip(state, headers, payload))]
pub async fn export_events(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<ExportEventsRequest>,
) -> impl IntoResponse {
	let api_key_ctx = match extract_api_key_context(&state, &headers).await {
		Ok(ctx) => ctx,
		Err(response) => return response,
	};

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	let org_id = api_key_ctx.org_id;
	let api_key_id = api_key_ctx.api_key_id;
	let export_limit = payload.limit;

	let result = export_events_impl(analytics_state, api_key_ctx, payload).await;

	// Log audit event for exports (these are significant data access operations)
	// Note: For API key auth, we log the API key ID as the resource since there's no user actor
	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::AnalyticsEventsExported)
			.resource("analytics_api_key", api_key_id.to_string())
			.details(serde_json::json!({
				"org_id": org_id.0.to_string(),
				"export_limit": export_limit,
			}))
			.build(),
	);

	result.into_response()
}

// ============================================================================
// API Key Management Routes (User Auth)
// ============================================================================

/// List analytics API keys for an organization.
#[utoipa::path(
    get,
    path = "/api/orgs/{org_id}/analytics/api-keys",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of API keys", body = ListAnalyticsApiKeysResponse),
        (status = 401, description = "Not authenticated", body = AnalyticsErrorResponse),
        (status = 404, description = "Organization not found", body = AnalyticsErrorResponse)
    ),
    tag = "analytics"
)]
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn list_api_keys(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		AnalyticsErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	// Check org membership
	match state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		Ok(Some(_)) => {}
		Ok(None) => {
			return not_found::<AnalyticsErrorResponse>(t(locale, "server.api.org.not_a_member"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			return internal_error::<AnalyticsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	}

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	let analytics_org_id = AnalyticsOrgId(org_id.into_inner());
	let user_ctx = UserAuthContext {
		user_id: AnalyticsUserId(current_user.user.id.into_inner()),
		org_id: analytics_org_id,
	};

	list_api_keys_impl(analytics_state, user_ctx)
		.await
		.into_response()
}

/// Create a new analytics API key.
#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/analytics/api-keys",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    request_body = CreateAnalyticsApiKeyRequest,
    responses(
        (status = 201, description = "API key created", body = CreateAnalyticsApiKeyResponse),
        (status = 400, description = "Invalid request", body = AnalyticsErrorResponse),
        (status = 401, description = "Not authenticated", body = AnalyticsErrorResponse),
        (status = 404, description = "Organization not found", body = AnalyticsErrorResponse)
    ),
    tag = "analytics"
)]
#[tracing::instrument(skip(state, payload), fields(%org_id, name = %payload.name))]
pub async fn create_api_key(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
	Json(payload): Json<CreateAnalyticsApiKeyRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id_str = org_id.clone();
	let org_id = parse_id!(
		AnalyticsErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	// Check org membership
	match state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		Ok(Some(_)) => {}
		Ok(None) => {
			return not_found::<AnalyticsErrorResponse>(t(locale, "server.api.org.not_a_member"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			return internal_error::<AnalyticsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	}

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	let key_name = payload.name.clone();
	let key_type = payload.key_type;
	let analytics_org_id = AnalyticsOrgId(org_id.into_inner());
	let user_ctx = UserAuthContext {
		user_id: AnalyticsUserId(current_user.user.id.into_inner()),
		org_id: analytics_org_id,
	};

	let result = create_api_key_impl(analytics_state, user_ctx, payload).await;

	// Log audit event for API key creation attempt
	// Note: We log after the operation; the audit trail captures the action was attempted
	// by an authenticated user. Failures are logged in the implementation function.
	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::AnalyticsApiKeyCreated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("analytics_api_key", "pending")
			.details(serde_json::json!({
				"org_id": org_id_str,
				"name": key_name,
				"key_type": format!("{:?}", key_type),
			}))
			.build(),
	);

	result.into_response()
}

/// Revoke an analytics API key.
#[utoipa::path(
    delete,
    path = "/api/orgs/{org_id}/analytics/api-keys/{key_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("key_id" = String, Path, description = "API Key ID")
    ),
    responses(
        (status = 200, description = "API key revoked", body = AnalyticsSuccessResponse),
        (status = 401, description = "Not authenticated", body = AnalyticsErrorResponse),
        (status = 404, description = "API key not found", body = AnalyticsErrorResponse)
    ),
    tag = "analytics"
)]
#[tracing::instrument(skip(state), fields(%org_id, %key_id))]
pub async fn revoke_api_key(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, key_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id_str = org_id.clone();
	let key_id_clone = key_id.clone();
	let org_id = parse_id!(
		AnalyticsErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	// Check org membership
	match state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		Ok(Some(_)) => {}
		Ok(None) => {
			return not_found::<AnalyticsErrorResponse>(t(locale, "server.api.org.not_a_member"))
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			return internal_error::<AnalyticsErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	}

	let analytics_state = match &state.analytics_state {
		Some(s) => s.clone(),
		None => {
			return internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response()
		}
	};

	let analytics_org_id = AnalyticsOrgId(org_id.into_inner());
	let user_ctx = UserAuthContext {
		user_id: AnalyticsUserId(current_user.user.id.into_inner()),
		org_id: analytics_org_id,
	};

	let result = revoke_api_key_impl(analytics_state, user_ctx, key_id).await;

	// Log audit event for API key revocation attempt
	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::AnalyticsApiKeyRevoked)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("analytics_api_key", key_id_clone)
			.details(serde_json::json!({
				"org_id": org_id_str,
			}))
			.build(),
	);

	result.into_response()
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract and validate API key from request headers.
async fn extract_api_key_context(
	state: &AppState,
	headers: &HeaderMap,
) -> Result<AnalyticsApiKeyContext, axum::response::Response> {
	// Get the Authorization header
	let auth_header = match headers.get("authorization") {
		Some(h) => match h.to_str() {
			Ok(s) => s,
			Err(_) => {
				return Err(
					(
						StatusCode::UNAUTHORIZED,
						Json(AnalyticsErrorResponse {
							error: "unauthorized".to_string(),
							message: "Invalid Authorization header encoding".to_string(),
						}),
					)
						.into_response(),
				);
			}
		},
		None => {
			return Err(
				(
					StatusCode::UNAUTHORIZED,
					Json(AnalyticsErrorResponse {
						error: "unauthorized".to_string(),
						message: "Missing Authorization header".to_string(),
					}),
				)
					.into_response(),
			);
		}
	};

	// Extract Bearer token
	let token = match auth_header.strip_prefix("Bearer ") {
		Some(t) => t,
		None => {
			return Err(
				(
					StatusCode::UNAUTHORIZED,
					Json(AnalyticsErrorResponse {
						error: "unauthorized".to_string(),
						message: "Invalid Authorization header format, expected Bearer token".to_string(),
					}),
				)
					.into_response(),
			);
		}
	};

	// Parse key type from prefix
	let key_type = match parse_key_type(token) {
		Some(kt) => kt,
		None => {
			return Err(
				(
					StatusCode::UNAUTHORIZED,
					Json(AnalyticsErrorResponse {
						error: "unauthorized".to_string(),
						message: "Invalid API key format".to_string(),
					}),
				)
					.into_response(),
			);
		}
	};

	let analytics_repo = match &state.analytics_repo {
		Some(r) => r.clone(),
		None => {
			return Err(
				internal_error::<AnalyticsErrorResponse>("Analytics not configured").into_response(),
			);
		}
	};

	// Find and verify the API key
	let api_key = match analytics_repo.find_api_key_by_raw(token).await {
		Ok(Some(key)) => key,
		Ok(None) => {
			return Err(
				(
					StatusCode::UNAUTHORIZED,
					Json(AnalyticsErrorResponse {
						error: "unauthorized".to_string(),
						message: "Invalid API key".to_string(),
					}),
				)
					.into_response(),
			);
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to verify API key");
			return Err(
				internal_error::<AnalyticsErrorResponse>("Failed to validate API key").into_response(),
			);
		}
	};

	// Check if key is revoked
	if api_key.revoked_at.is_some() {
		return Err(
			(
				StatusCode::UNAUTHORIZED,
				Json(AnalyticsErrorResponse {
					error: "unauthorized".to_string(),
					message: "API key has been revoked".to_string(),
				}),
			)
				.into_response(),
		);
	}

	Ok(AnalyticsApiKeyContext {
		api_key_id: api_key.id,
		org_id: api_key.org_id,
		key_type,
	})
}
