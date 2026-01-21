// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Health and metrics HTTP handlers.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};

use crate::{
	api::AppState,
	error::ServerError,
	health::{self, HealthComponents, HealthResponse, HealthStatus},
};

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "System is healthy", body = HealthResponse),
        (status = 503, description = "System is unhealthy", body = HealthResponse)
    ),
    tag = "health"
)]
/// GET /health - Comprehensive health check endpoint.
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
	use tokio::time::Instant;

	let overall_start = Instant::now();

	// Run checks in parallel
	let (
		database,
		bin_dir,
		google_cse,
		serper,
		github_app,
		kubernetes,
		smtp,
		geoip,
		jobs,
		secrets,
		scim,
	) = tokio::join!(
		health::check_database(&state.repo),
		async { health::check_bin_dir() },
		health::check_google_cse(),
		health::check_serper(),
		health::check_github_app(state.github_client.clone()),
		health::check_kubernetes(state.provisioner.as_ref()),
		health::check_smtp(state.smtp_client.as_ref()),
		async { health::check_geoip(state.geoip_service.as_ref()) },
		health::check_jobs(state.job_scheduler.as_ref()),
		health::check_secrets(state.secrets_service.as_ref(), state.svid_issuer.as_ref()),
		health::check_scim(&state.scim_config, &state.org_repo)
	);

	let llm_providers = health::check_llm_providers(state.llm_service.as_deref()).await;

	// Check auth providers (sync check, just validates configuration)
	let auth_providers = health::check_auth_providers(
		state.github_oauth.as_deref(),
		state.google_oauth.as_deref(),
		state.okta_oauth.as_deref(),
		state.smtp_client.is_some(),
	);

	let components = HealthComponents {
		auth_providers,
		bin_dir,
		database,
		geoip,
		github_app,
		google_cse,
		jobs,
		kubernetes,
		llm_providers,
		scim,
		secrets,
		serper,
		smtp,
	};

	let status = health::aggregate_status(&components);
	let duration_ms = overall_start.elapsed().as_millis() as u64;

	let response = HealthResponse {
		status,
		timestamp: chrono::Utc::now().to_rfc3339(),
		duration_ms,
		version: loom_common_version::HealthVersionInfo::current(),
		components,
	};

	let http_status = match status {
		HealthStatus::Healthy | HealthStatus::Degraded => StatusCode::OK,
		HealthStatus::Unhealthy | HealthStatus::Unknown => StatusCode::SERVICE_UNAVAILABLE,
	};

	(http_status, Json(response))
}

#[utoipa::path(
    get,
    path = "/metrics",
    responses(
        (status = 200, description = "Prometheus metrics", content_type = "text/plain")
    ),
    tag = "health"
)]
/// GET /metrics - Prometheus metrics export endpoint.
///
/// Returns all query metrics in Prometheus text format. Includes:
/// - Total queries sent/succeeded/failed
/// - Query latency histogram
/// - Pending queries gauge
/// - Metrics by query type and session
/// - Timeout counters by query type
pub async fn prometheus_metrics(
	State(state): State<AppState>,
) -> Result<impl IntoResponse, ServerError> {
	match state.query_metrics.gather_metrics() {
		Ok(metrics) => {
			tracing::debug!("prometheus_metrics: gathering metrics");
			Ok((
				StatusCode::OK,
				[(
					axum::http::header::CONTENT_TYPE,
					"text/plain; version=0.0.4; charset=utf-8",
				)],
				metrics,
			))
		}
		Err(e) => {
			tracing::error!(error = %e, "prometheus_metrics: failed to gather metrics");
			Err(ServerError::Internal(format!(
				"Failed to gather metrics: {e}"
			)))
		}
	}
}
