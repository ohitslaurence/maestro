// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Secrets management HTTP handlers.
//!
//! Implements user-facing secret management endpoints:
//! - Organization-scoped secrets (CRUD)
//! - Repository-scoped secrets (CRUD)
//!
//! These endpoints return metadata only, never secret values.

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_common_secret::SecretString;
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::RepoStore;
use loom_server_secrets::store::SecretFilter;
use loom_server_secrets::{
	CreateSecretInput, SecretScope, SecretsService, SoftwareKeyBackend, SqliteSecretStore,
};
use std::sync::Arc;
use uuid::Uuid;

pub use loom_server_api::secrets::*;

use crate::{
	api::AppState,
	api_response::id_parse_error,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	impl_api_error_response,
	validation::{parse_org_id as shared_parse_org_id, parse_uuid},
};

impl_api_error_response!(SecretErrorResponse);

async fn get_secrets_service(
	state: &AppState,
	locale: &str,
) -> Result<
	Arc<SecretsService<SoftwareKeyBackend, SqliteSecretStore>>,
	(StatusCode, Json<SecretErrorResponse>),
> {
	match state.secrets_service.as_ref() {
		Some(svc) => Ok(svc.clone()),
		None => Err((
			StatusCode::SERVICE_UNAVAILABLE,
			Json(SecretErrorResponse {
				error: "not_configured".to_string(),
				message: t(locale, "server.api.secrets.not_configured").to_string(),
			}),
		)),
	}
}

async fn verify_org_membership(
	state: &AppState,
	org_id: &OrgId,
	user_id: &loom_server_auth::types::UserId,
	locale: &str,
) -> Result<(), (StatusCode, Json<SecretErrorResponse>)> {
	match state.org_repo.get_membership(org_id, user_id).await {
		Ok(Some(_)) => Ok(()),
		Ok(None) => Err((
			StatusCode::FORBIDDEN,
			Json(SecretErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.not_a_member").to_string(),
			}),
		)),
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			Err((
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			))
		}
	}
}

async fn verify_org_admin(
	state: &AppState,
	org_id: &OrgId,
	user_id: &loom_server_auth::types::UserId,
	locale: &str,
) -> Result<(), (StatusCode, Json<SecretErrorResponse>)> {
	match state.org_repo.get_membership(org_id, user_id).await {
		Ok(Some(m)) if m.role == OrgRole::Owner || m.role == OrgRole::Admin => Ok(()),
		Ok(Some(_)) => Err((
			StatusCode::FORBIDDEN,
			Json(SecretErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.admin_required").to_string(),
			}),
		)),
		Ok(None) => Err((
			StatusCode::FORBIDDEN,
			Json(SecretErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.not_a_member").to_string(),
			}),
		)),
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			Err((
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			))
		}
	}
}

async fn verify_repo_access(
	state: &AppState,
	repo_id: Uuid,
	user_id: &loom_server_auth::types::UserId,
	locale: &str,
) -> Result<OrgId, (StatusCode, Json<SecretErrorResponse>)> {
	let scm_store = match state.scm_repo_store.as_ref() {
		Some(store) => store,
		None => {
			return Err((
				StatusCode::SERVICE_UNAVAILABLE,
				Json(SecretErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.scm.not_configured").to_string(),
				}),
			));
		}
	};

	let repo = match scm_store.get_by_id(repo_id).await {
		Ok(Some(r)) => r,
		Ok(None) => {
			return Err((
				StatusCode::NOT_FOUND,
				Json(SecretErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.repo.not_found").to_string(),
				}),
			));
		}
		Err(e) => {
			tracing::error!(error = %e, %repo_id, "Failed to get repo");
			return Err((
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			));
		}
	};

	let org_id = OrgId::new(repo.owner_id);

	match state.org_repo.get_membership(&org_id, user_id).await {
		Ok(Some(m)) if m.role == OrgRole::Owner || m.role == OrgRole::Admin => Ok(org_id),
		Ok(Some(_)) => Err((
			StatusCode::FORBIDDEN,
			Json(SecretErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.admin_required").to_string(),
			}),
		)),
		Ok(None) => Err((
			StatusCode::FORBIDDEN,
			Json(SecretErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.not_a_member").to_string(),
			}),
		)),
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check org membership");
			Err((
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			))
		}
	}
}

// =============================================================================
// Organization Secret Routes
// =============================================================================

#[utoipa::path(
    get,
    path = "/api/orgs/{org_id}/secrets",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of secrets", body = ListSecretsResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not a member", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn list_org_secrets(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = match shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	if let Err(resp) = verify_org_membership(&state, &org_id, &current_user.user.id, locale).await {
		return resp.into_response();
	}

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let filter = SecretFilter {
		org_id: Some(org_id),
		scope: Some(SecretScope::Org { org_id }),
		..Default::default()
	};

	match service.list_secrets(&filter).await {
		Ok(secrets) => {
			let responses: Vec<SecretMetadataResponse> = secrets
				.into_iter()
				.map(|s| SecretMetadataResponse {
					name: s.name,
					scope: "org".to_string(),
					description: s.description,
					current_version: s.current_version,
					created_at: chrono::DateTime::default(),
					updated_at: chrono::DateTime::default(),
				})
				.collect();
			(
				StatusCode::OK,
				Json(ListSecretsResponse { secrets: responses }),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list org secrets");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/secrets",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    request_body = CreateSecretRequest,
    responses(
        (status = 201, description = "Secret created", body = SecretMetadataResponse),
        (status = 400, description = "Invalid request", body = SecretErrorResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 409, description = "Secret already exists", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state, payload), fields(%org_id))]
pub async fn create_org_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
	Json(payload): Json<CreateSecretRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = match shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	if let Err(resp) = verify_org_admin(&state, &org_id, &current_user.user.id, locale).await {
		return resp.into_response();
	}

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let input = CreateSecretInput {
		org_id,
		scope: SecretScope::Org { org_id },
		repo_id: None,
		weaver_id: None,
		name: payload.name,
		value: SecretString::new(payload.value),
		description: payload.description,
		created_by: current_user.user.id,
	};

	match service.create_secret(input).await {
		Ok(meta) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "org".to_string(),
				description: meta.description,
				current_version: meta.current_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::CREATED, Json(response)).into_response()
		}
		Err(loom_server_secrets::SecretsError::SecretAlreadyExists(_)) => (
			StatusCode::CONFLICT,
			Json(SecretErrorResponse {
				error: "already_exists".to_string(),
				message: t(locale, "server.api.secrets.already_exists").to_string(),
			}),
		)
			.into_response(),
		Err(loom_server_secrets::SecretsError::InvalidSecretName(msg)) => (
			StatusCode::BAD_REQUEST,
			Json(SecretErrorResponse {
				error: "invalid_name".to_string(),
				message: msg,
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to create org secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    get,
    path = "/api/orgs/{org_id}/secrets/{name}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    responses(
        (status = 200, description = "Secret metadata", body = SecretMetadataResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not a member", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%org_id, %name))]
pub async fn get_org_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = match shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	if let Err(resp) = verify_org_membership(&state, &org_id, &current_user.user.id, locale).await {
		return resp.into_response();
	}

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	match service
		.get_secret_by_name(org_id, SecretScope::Org { org_id }, None, None, &name)
		.await
	{
		Ok(Some(meta)) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "org".to_string(),
				description: meta.description,
				current_version: meta.current_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Ok(None) => (
			StatusCode::NOT_FOUND,
			Json(SecretErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.secrets.not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to get org secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    put,
    path = "/api/orgs/{org_id}/secrets/{name}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    request_body = UpdateSecretRequest,
    responses(
        (status = 200, description = "Secret updated", body = SecretMetadataResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state, payload), fields(%org_id, %name))]
pub async fn update_org_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, name)): Path<(String, String)>,
	Json(payload): Json<UpdateSecretRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = match shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	if let Err(resp) = verify_org_admin(&state, &org_id, &current_user.user.id, locale).await {
		return resp.into_response();
	}

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let meta = match service
		.get_secret_by_name(org_id, SecretScope::Org { org_id }, None, None, &name)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(SecretErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.secrets.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get secret for update");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	match service
		.rotate_secret(
			meta.id,
			SecretString::new(payload.value),
			current_user.user.id,
		)
		.await
	{
		Ok(new_version) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "org".to_string(),
				description: meta.description,
				current_version: new_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to update org secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    delete,
    path = "/api/orgs/{org_id}/secrets/{name}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    responses(
        (status = 200, description = "Secret deleted", body = SecretSuccessResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%org_id, %name))]
pub async fn delete_org_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = match shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	if let Err(resp) = verify_org_admin(&state, &org_id, &current_user.user.id, locale).await {
		return resp.into_response();
	}

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let meta = match service
		.get_secret_by_name(org_id, SecretScope::Org { org_id }, None, None, &name)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(SecretErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.secrets.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get secret for delete");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	match service.delete_secret(meta.id, current_user.user.id).await {
		Ok(()) => (
			StatusCode::OK,
			Json(SecretSuccessResponse {
				message: t(locale, "server.api.secrets.deleted").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete org secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

// =============================================================================
// Repository Secret Routes
// =============================================================================

#[utoipa::path(
    get,
    path = "/api/repos/{repo_id}/secrets",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    responses(
        (status = 200, description = "List of secrets", body = ListSecretsResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Repository not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%repo_id))]
pub async fn list_repo_secrets(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(repo_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let repo_id = match parse_uuid(&repo_id, &t(locale, "server.api.repo.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	let org_id = match verify_repo_access(&state, repo_id, &current_user.user.id, locale).await {
		Ok(org_id) => org_id,
		Err(resp) => return resp.into_response(),
	};

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let filter = SecretFilter {
		org_id: Some(org_id),
		repo_id: Some(repo_id),
		..Default::default()
	};

	match service.list_secrets(&filter).await {
		Ok(secrets) => {
			let responses: Vec<SecretMetadataResponse> = secrets
				.into_iter()
				.map(|s| SecretMetadataResponse {
					name: s.name,
					scope: "repo".to_string(),
					description: s.description,
					current_version: s.current_version,
					created_at: chrono::DateTime::default(),
					updated_at: chrono::DateTime::default(),
				})
				.collect();
			(
				StatusCode::OK,
				Json(ListSecretsResponse { secrets: responses }),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list repo secrets");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    post,
    path = "/api/repos/{repo_id}/secrets",
    params(
        ("repo_id" = String, Path, description = "Repository ID")
    ),
    request_body = CreateSecretRequest,
    responses(
        (status = 201, description = "Secret created", body = SecretMetadataResponse),
        (status = 400, description = "Invalid request", body = SecretErrorResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 409, description = "Secret already exists", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state, payload), fields(%repo_id))]
pub async fn create_repo_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(repo_id): Path<String>,
	Json(payload): Json<CreateSecretRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let repo_id = match parse_uuid(&repo_id, &t(locale, "server.api.repo.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	let org_id = match verify_repo_access(&state, repo_id, &current_user.user.id, locale).await {
		Ok(org_id) => org_id,
		Err(resp) => return resp.into_response(),
	};

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let input = CreateSecretInput {
		org_id,
		scope: SecretScope::Repo {
			org_id,
			repo_id: repo_id.to_string(),
		},
		repo_id: Some(repo_id),
		weaver_id: None,
		name: payload.name,
		value: SecretString::new(payload.value),
		description: payload.description,
		created_by: current_user.user.id,
	};

	match service.create_secret(input).await {
		Ok(meta) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "repo".to_string(),
				description: meta.description,
				current_version: meta.current_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::CREATED, Json(response)).into_response()
		}
		Err(loom_server_secrets::SecretsError::SecretAlreadyExists(_)) => (
			StatusCode::CONFLICT,
			Json(SecretErrorResponse {
				error: "already_exists".to_string(),
				message: t(locale, "server.api.secrets.already_exists").to_string(),
			}),
		)
			.into_response(),
		Err(loom_server_secrets::SecretsError::InvalidSecretName(msg)) => (
			StatusCode::BAD_REQUEST,
			Json(SecretErrorResponse {
				error: "invalid_name".to_string(),
				message: msg,
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to create repo secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    get,
    path = "/api/repos/{repo_id}/secrets/{name}",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    responses(
        (status = 200, description = "Secret metadata", body = SecretMetadataResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%repo_id, %name))]
pub async fn get_repo_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((repo_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let repo_id = match parse_uuid(&repo_id, &t(locale, "server.api.repo.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	let org_id = match verify_repo_access(&state, repo_id, &current_user.user.id, locale).await {
		Ok(org_id) => org_id,
		Err(resp) => return resp.into_response(),
	};

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let scope = SecretScope::Repo {
		org_id,
		repo_id: repo_id.to_string(),
	};

	match service
		.get_secret_by_name(org_id, scope, Some(repo_id), None, &name)
		.await
	{
		Ok(Some(meta)) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "repo".to_string(),
				description: meta.description,
				current_version: meta.current_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Ok(None) => (
			StatusCode::NOT_FOUND,
			Json(SecretErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.secrets.not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to get repo secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    put,
    path = "/api/repos/{repo_id}/secrets/{name}",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    request_body = UpdateSecretRequest,
    responses(
        (status = 200, description = "Secret updated", body = SecretMetadataResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state, payload), fields(%repo_id, %name))]
pub async fn update_repo_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((repo_id, name)): Path<(String, String)>,
	Json(payload): Json<UpdateSecretRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let repo_id = match parse_uuid(&repo_id, &t(locale, "server.api.repo.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	let org_id = match verify_repo_access(&state, repo_id, &current_user.user.id, locale).await {
		Ok(org_id) => org_id,
		Err(resp) => return resp.into_response(),
	};

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let scope = SecretScope::Repo {
		org_id,
		repo_id: repo_id.to_string(),
	};

	let meta = match service
		.get_secret_by_name(org_id, scope, Some(repo_id), None, &name)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(SecretErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.secrets.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get secret for update");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	match service
		.rotate_secret(
			meta.id,
			SecretString::new(payload.value),
			current_user.user.id,
		)
		.await
	{
		Ok(new_version) => {
			let response = SecretMetadataResponse {
				name: meta.name,
				scope: "repo".to_string(),
				description: meta.description,
				current_version: new_version,
				created_at: chrono::DateTime::default(),
				updated_at: chrono::DateTime::default(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to update repo secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    delete,
    path = "/api/repos/{repo_id}/secrets/{name}",
    params(
        ("repo_id" = String, Path, description = "Repository ID"),
        ("name" = String, Path, description = "Secret name")
    ),
    responses(
        (status = 200, description = "Secret deleted", body = SecretSuccessResponse),
        (status = 401, description = "Not authenticated", body = SecretErrorResponse),
        (status = 403, description = "Not authorized", body = SecretErrorResponse),
        (status = 404, description = "Secret not found", body = SecretErrorResponse)
    ),
    tag = "secrets"
)]
#[tracing::instrument(skip(state), fields(%repo_id, %name))]
pub async fn delete_repo_secret(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((repo_id, name)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let repo_id = match parse_uuid(&repo_id, &t(locale, "server.api.repo.invalid_id")) {
		Ok(id) => id,
		Err(e) => return id_parse_error::<SecretErrorResponse>(e).into_response(),
	};

	let org_id = match verify_repo_access(&state, repo_id, &current_user.user.id, locale).await {
		Ok(org_id) => org_id,
		Err(resp) => return resp.into_response(),
	};

	let service = match get_secrets_service(&state, locale).await {
		Ok(svc) => svc,
		Err(resp) => return resp.into_response(),
	};

	let scope = SecretScope::Repo {
		org_id,
		repo_id: repo_id.to_string(),
	};

	let meta = match service
		.get_secret_by_name(org_id, scope, Some(repo_id), None, &name)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(SecretErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.secrets.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get secret for delete");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	match service.delete_secret(meta.id, current_user.user.id).await {
		Ok(()) => (
			StatusCode::OK,
			Json(SecretSuccessResponse {
				message: t(locale, "server.api.secrets.deleted").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete repo secret");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SecretErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}
