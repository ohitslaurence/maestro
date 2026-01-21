// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! OpenAPI documentation for loom-server.
//!
//! This module provides the OpenAPI 3.0 specification for the Loom Server API,
//! generated from Rust types using utoipa.

use utoipa::OpenApi;

/// Main OpenAPI documentation struct.
///
/// This generates the complete OpenAPI specification for the Loom Server API.
/// Access the interactive documentation at `/api` and the raw JSON spec at
/// `/api/openapi.json`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Loom Server API",
        version = "1.0.0",
        description = "AI-powered coding assistant server API. Loom provides thread persistence, LLM proxy endpoints, GitHub integration, and web search capabilities.",
        license(name = "Proprietary"),
        contact(
            name = "Geoffrey Huntley",
            email = "ghuntley@ghuntley.com",
            url = "https://ghuntley.com"
        )
    ),
    servers(
        (url = "/", description = "Local server")
    ),
    tags(
        (name = "threads", description = "Thread CRUD and search operations for conversation persistence"),
        (name = "share", description = "Thread sharing and support access management"),
        (name = "health", description = "Health checks, metrics, and system status"),
        (name = "llm-proxy", description = "LLM provider proxy endpoints (Anthropic, OpenAI, Vertex)"),
        (name = "github", description = "GitHub App integration and code search"),
        (name = "google-cse", description = "Google Custom Search Engine proxy"),
        (name = "server-query", description = "Server query orchestration for client-server communication"),
        (name = "debug", description = "Debug and tracing endpoints for development"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "sessions", description = "Session management and revocation"),
        (name = "orgs", description = "Organization management and membership"),
        (name = "teams", description = "Team management within organizations"),
        (name = "users", description = "User profile management"),
        (name = "weavers", description = "Weaver provisioning and management"),
        (name = "api-keys", description = "API key management for organizations"),
        (name = "admin", description = "System administration endpoints (system_admin only)"),
        (name = "repos", description = "Repository management"),
        (name = "mirrors", description = "Push mirror management for repositories")
    ),
    paths(
        // Thread endpoints
        crate::routes::threads::search_threads,
        crate::routes::threads::upsert_thread,
        crate::routes::threads::get_thread,
        crate::routes::threads::list_threads,
        crate::routes::threads::delete_thread,
        crate::routes::threads::update_thread_visibility,
        // Share endpoints
        crate::routes::share::create_share_link,
        crate::routes::share::revoke_share_link,
        crate::routes::share::get_shared_thread,
        crate::routes::share::request_support_access,
        crate::routes::share::approve_support_access,
        crate::routes::share::revoke_support_access,
        // Health endpoints
        crate::routes::health::health_check,
        crate::routes::health::prometheus_metrics,
        // Auth endpoints
        crate::routes::auth::get_providers,
        crate::routes::auth::get_current_user,
        crate::routes::auth::logout,
        crate::routes::auth::request_magic_link,
        crate::routes::auth::device_start,
        crate::routes::auth::device_poll,
        crate::routes::auth::device_complete,
        // Session endpoints
        crate::routes::sessions::list_sessions,
        crate::routes::sessions::revoke_session,
        // Organization endpoints
        crate::routes::orgs::list_orgs,
        crate::routes::orgs::create_org,
        crate::routes::orgs::get_org,
        crate::routes::orgs::update_org,
        crate::routes::orgs::delete_org,
        crate::routes::orgs::list_org_members,
        crate::routes::orgs::add_org_member,
        crate::routes::orgs::remove_org_member,
        // Team endpoints
        crate::routes::teams::list_teams,
        crate::routes::teams::create_team,
        crate::routes::teams::get_team,
        crate::routes::teams::update_team,
        crate::routes::teams::delete_team,
        crate::routes::teams::list_team_members,
        crate::routes::teams::add_team_member,
        crate::routes::teams::remove_team_member,
        // User endpoints
        crate::routes::users::get_user_profile,
        crate::routes::users::update_current_user,
        crate::routes::users::request_account_deletion,
        crate::routes::users::restore_account,
        // Admin endpoints
        crate::routes::admin::list_users,
        crate::routes::admin::update_user_roles,
        crate::routes::admin::start_impersonation,
        crate::routes::admin::stop_impersonation,
        crate::routes::admin::list_audit_logs,
        // API key endpoints
        crate::routes::api_keys::list_api_keys,
        crate::routes::api_keys::create_api_key,
        crate::routes::api_keys::revoke_api_key,
        crate::routes::api_keys::get_api_key_usage,
        // Google CSE endpoints
        crate::routes::cse::proxy_cse,
        // GitHub endpoints
        crate::routes::github::get_github_app_info,
        crate::routes::github::get_github_installation_by_repo,
        crate::routes::github::proxy_github_search_code,
        crate::routes::github::proxy_github_repo_info,
        crate::routes::github::proxy_github_file_contents,
        // Debug endpoints
        crate::routes::debug::get_query_trace,
        crate::routes::debug::list_query_traces,
        crate::routes::debug::get_trace_stats,
        // Weaver endpoints
        crate::routes::weaver::create_weaver,
        crate::routes::weaver::list_weavers,
        crate::routes::weaver::get_weaver,
        crate::routes::weaver::delete_weaver,
        crate::routes::weaver::stream_logs,
        crate::routes::weaver::trigger_cleanup,
        // Mirror endpoints
        crate::routes::mirrors::list_mirrors,
        crate::routes::mirrors::create_mirror,
        crate::routes::mirrors::delete_mirror,
        crate::routes::mirrors::trigger_sync,
    ),
    components(
        schemas(
            // API request/response types
            crate::routes::threads::SearchResponse,
            crate::routes::threads::SearchResponseHit,
            crate::routes::threads::UpdateVisibilityRequest,
            crate::routes::threads::ListResponse,
            // Share types
            crate::routes::share::CreateShareLinkRequest,
            crate::routes::share::CreateShareLinkResponse,
            crate::routes::share::ShareLinkSuccessResponse,
            crate::routes::share::ShareLinkErrorResponse,
            crate::routes::share::SharedThreadResponse,
            crate::routes::share::SupportAccessRequestResponse,
            crate::routes::share::SupportAccessApprovalResponse,
            crate::routes::share::SupportAccessSuccessResponse,
            crate::routes::share::SupportAccessErrorResponse,
            crate::routes::auth::AuthProvidersResponse,
            crate::routes::auth::CurrentUserResponse,
            crate::routes::auth::AuthSuccessResponse,
            crate::routes::auth::AuthErrorResponse,
            crate::routes::auth::MagicLinkRequest,
            crate::routes::auth::DeviceCodeStartResponse,
            crate::routes::auth::DeviceCodePollRequest,
            crate::routes::auth::DeviceCodePollResponse,
            // Session types
            crate::routes::sessions::ListSessionsResponse,
            crate::routes::sessions::SessionResponse,
            crate::routes::sessions::SessionSuccessResponse,
            crate::routes::sessions::SessionErrorResponse,
            // Organization types
            crate::routes::orgs::ListOrgsResponse,
            crate::routes::orgs::OrgResponse,
            crate::routes::orgs::CreateOrgRequest,
            crate::routes::orgs::UpdateOrgRequest,
            crate::routes::orgs::OrgSuccessResponse,
            crate::routes::orgs::OrgErrorResponse,
            crate::routes::orgs::OrgVisibilityApi,
            crate::routes::orgs::ListOrgMembersResponse,
            crate::routes::orgs::OrgMemberResponse,
            crate::routes::orgs::AddOrgMemberRequest,
            // Team types
            crate::routes::teams::ListTeamsResponse,
            crate::routes::teams::TeamResponse,
            crate::routes::teams::CreateTeamRequest,
            crate::routes::teams::UpdateTeamRequest,
            crate::routes::teams::TeamSuccessResponse,
            crate::routes::teams::TeamErrorResponse,
            crate::routes::teams::TeamRoleApi,
            crate::routes::teams::ListTeamMembersResponse,
            crate::routes::teams::TeamMemberResponse,
            crate::routes::teams::AddTeamMemberRequest,
            // User types
            crate::routes::users::UserProfileResponse,
            crate::routes::users::CurrentUserProfileResponse,
            crate::routes::users::UpdateUserProfileRequest,
            crate::routes::users::UserSuccessResponse,
            crate::routes::users::UserErrorResponse,
            crate::routes::users::AccountDeletionResponse,
            // API key types
            crate::routes::api_keys::ListApiKeysResponse,
            crate::routes::api_keys::ApiKeyResponse,
            crate::routes::api_keys::CreateApiKeyRequest,
            crate::routes::api_keys::CreateApiKeyResponse,
            crate::routes::api_keys::ApiKeyScopeApi,
            crate::routes::api_keys::ApiKeySuccessResponse,
            crate::routes::api_keys::ApiKeyErrorResponse,
            crate::routes::api_keys::ApiKeyUsageListResponse,
            crate::routes::api_keys::ApiKeyUsageResponse,
            // Admin types
            crate::routes::admin::AdminUserResponse,
            crate::routes::admin::ListUsersResponse,
            crate::routes::admin::UpdateRolesRequest,
            crate::routes::admin::ImpersonateRequest,
            crate::routes::admin::ImpersonateResponse,
            crate::routes::admin::ImpersonationState,
            crate::routes::admin::ImpersonationUserInfo,
            crate::routes::admin::AuditLogEntryResponse,
            crate::routes::admin::ListAuditLogsResponse,
            crate::routes::admin::AdminSuccessResponse,
            crate::routes::admin::AdminErrorResponse,
            crate::routes::cse::CseProxyRequest,
            crate::routes::cse::CseProxyResponse,
            crate::routes::cse::CseProxyResultItem,
            crate::routes::github::GithubSearchCodeRequest,
            crate::routes::github::GithubRepoInfoRequest,
            crate::routes::github::GithubFileContentsRequest,
            crate::routes::github::GithubRepoInfoResponse,
            crate::routes::github::GithubFileContentsResponse,
            // Health types
            crate::health::HealthResponse,
            crate::health::HealthStatus,
            crate::health::HealthComponents,
            crate::health::DatabaseHealth,
            crate::health::BinDirHealth,
            crate::health::LlmProvidersHealth,
            crate::health::LlmProviderHealth,
            crate::health::AnthropicAccountHealth,
            crate::health::AnthropicAccountStatus,
            crate::health::AnthropicPoolHealth,
            crate::health::GoogleCseHealth,
            crate::health::GithubAppHealth,
            // Error types
            crate::error::ErrorResponse,
            // Thread types (from loom-thread crate)
            loom_common_thread::Thread,
            loom_common_thread::ThreadId,
            loom_common_thread::ThreadSummary,
            loom_common_thread::ThreadVisibility,
            loom_common_thread::MessageSnapshot,
            loom_common_thread::MessageRole,
            loom_common_thread::ConversationSnapshot,
            loom_common_thread::AgentStateSnapshot,
            loom_common_thread::AgentStateKind,
            loom_common_thread::ThreadMetadata,
            // GitHub types (from loom-server-github-app crate)
            loom_server_github_app::AppInfoResponse,
            loom_server_github_app::InstallationStatusResponse,
            loom_server_github_app::CodeSearchRequest,
            loom_server_github_app::CodeSearchResponse,
            loom_server_github_app::CodeSearchItem,
            // Google CSE types (from loom-server-search-google-cse crate)
            loom_server_search_google_cse::CseRequest,
            loom_server_search_google_cse::CseResponse,
            loom_server_search_google_cse::CseResultItem,
            // Weaver types
            crate::routes::weaver::CreateWeaverApiRequest,
            crate::routes::weaver::WeaverApiResponse,
            crate::routes::weaver::WeaverStatusApi,
            crate::routes::weaver::ListWeaversApiResponse,
            crate::routes::weaver::CleanupApiResponse,
            crate::routes::weaver::ResourceSpecApi,
            // Mirror types
            crate::routes::mirrors::CreateMirrorRequest,
            crate::routes::mirrors::MirrorResponse,
            crate::routes::mirrors::ListMirrorsResponse,
            crate::routes::mirrors::SyncResponse,
        )
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
	use super::*;

	/// Verify the OpenAPI spec generates valid JSON.
	#[test]
	fn test_openapi_spec_generates_valid_json() {
		let spec = ApiDoc::openapi();
		let json = serde_json::to_string_pretty(&spec).expect("should serialize to JSON");

		assert!(!json.is_empty());
		assert!(json.contains("\"openapi\""));
		assert!(json.contains("\"3.1"));
		assert!(json.contains("Loom Server API"));
	}

	/// Verify all expected tags are present.
	#[test]
	fn test_openapi_spec_has_all_tags() {
		let spec = ApiDoc::openapi();
		let json = serde_json::to_string(&spec).expect("should serialize");

		let expected_tags = [
			"threads",
			"health",
			"llm-proxy",
			"github",
			"google-cse",
			"server-query",
			"debug",
			"auth",
			"weavers",
		];
		for tag in expected_tags {
			assert!(json.contains(tag), "Missing tag: {tag}");
		}
	}

	/// Verify all documented endpoints are present in paths.
	#[test]
	fn test_openapi_spec_has_documented_paths() {
		let spec = ApiDoc::openapi();
		let json = serde_json::to_string(&spec).expect("should serialize");

		let expected_paths = [
			"/api/threads",
			"/api/threads/{id}",
			"/api/threads/search",
			"/health",
			"/metrics",
			"/proxy/cse",
			"/api/github/app",
		];
		for path in expected_paths {
			assert!(json.contains(path), "Missing path: {path}");
		}
	}
}
