// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_llm_anthropic::{
	AccountDetails as PoolAccountDetails, AccountHealthStatus as PoolAccountHealthStatus,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

/// A user in admin API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AdminUserResponse {
	pub id: String,
	pub display_name: String,
	pub primary_email: Option<String>,
	pub avatar_url: Option<String>,
	pub is_system_admin: bool,
	pub is_support: bool,
	pub is_auditor: bool,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub deleted_at: Option<DateTime<Utc>>,
}

/// Paginated list of users.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListUsersResponse {
	pub users: Vec<AdminUserResponse>,
	pub total: i64,
	pub limit: i32,
	pub offset: i32,
}

/// Query parameters for listing users.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct ListUsersParams {
	#[serde(default = "default_limit")]
	pub limit: i32,
	#[serde(default)]
	pub offset: i32,
	pub search: Option<String>,
}

fn default_limit() -> i32 {
	50
}

/// Request to update a user's global roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateRolesRequest {
	pub is_system_admin: Option<bool>,
	pub is_support: Option<bool>,
	pub is_auditor: Option<bool>,
}

/// Request to start impersonation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ImpersonateRequest {
	pub reason: String,
}

/// Response for impersonation start.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ImpersonateResponse {
	pub session_id: String,
	pub message: String,
}

/// User info for impersonation state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ImpersonationUserInfo {
	pub id: String,
	pub display_name: String,
}

/// Current impersonation state for the authenticated admin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ImpersonationState {
	pub is_impersonating: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub original_user: Option<ImpersonationUserInfo>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub impersonated_user: Option<ImpersonationUserInfo>,
}

/// An audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AuditLogEntryResponse {
	pub id: String,
	pub timestamp: DateTime<Utc>,
	pub event_type: String,
	pub actor_user_id: Option<String>,
	pub impersonating_user_id: Option<String>,
	pub resource_type: Option<String>,
	pub resource_id: Option<String>,
	pub action: String,
	pub ip_address: Option<String>,
	pub user_agent: Option<String>,
	pub details: Option<serde_json::Value>,
}

/// Paginated list of audit logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListAuditLogsResponse {
	pub logs: Vec<AuditLogEntryResponse>,
	pub total: i64,
	pub limit: i32,
	pub offset: i32,
}

/// Query parameters for listing audit logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct ListAuditLogsParams {
	pub event_type: Option<String>,
	pub actor_id: Option<String>,
	pub resource_type: Option<String>,
	pub resource_id: Option<String>,
	pub from: Option<DateTime<Utc>>,
	pub to: Option<DateTime<Utc>>,
	#[serde(default = "default_limit")]
	pub limit: i32,
	#[serde(default)]
	pub offset: i32,
}

/// Success response for admin operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AdminSuccessResponse {
	pub message: String,
}

/// Error response for admin operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AdminErrorResponse {
	pub error: String,
	pub message: String,
}

/// Response for deleting a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct DeleteUserResponse {
	pub message: String,
	pub user_id: String,
}

/// Account status for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
	Available,
	CoolingDown,
	Disabled,
}

/// Details of an Anthropic OAuth account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AccountDetailsResponse {
	pub id: String,
	pub status: AccountStatus,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cooldown_remaining_secs: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub last_error: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub expires_at: Option<DateTime<Utc>>,
}

/// Summary of account pool status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AccountsSummary {
	pub total: usize,
	pub available: usize,
	pub cooling_down: usize,
	pub disabled: usize,
}

/// Response for listing Anthropic accounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AnthropicAccountsResponse {
	pub accounts: Vec<AccountDetailsResponse>,
	pub summary: AccountsSummary,
}

/// Request to initiate OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct InitiateOAuthRequest {
	pub redirect_after: Option<String>,
}

/// Response for initiating OAuth flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct InitiateOAuthResponse {
	/// URL to open in browser for OAuth authorization.
	pub redirect_url: String,
	/// State token to use when submitting the code.
	pub state: String,
}

/// Request to submit OAuth authorization code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SubmitOAuthCodeRequest {
	/// The authorization code from Anthropic's callback page.
	pub code: String,
	/// The state token from the initiate response.
	pub state: String,
}

/// Response for successfully adding an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct AddAccountResponse {
	/// The ID of the newly added account.
	pub account_id: String,
}

/// Response for removing an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RemoveAccountResponse {
	pub removed: String,
}

/// Query parameters for OAuth callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicOAuthCallbackQuery {
	pub code: String,
	pub state: String,
}

impl From<PoolAccountHealthStatus> for AccountStatus {
	fn from(status: PoolAccountHealthStatus) -> Self {
		match status {
			PoolAccountHealthStatus::Available => AccountStatus::Available,
			PoolAccountHealthStatus::CoolingDown => AccountStatus::CoolingDown,
			PoolAccountHealthStatus::Disabled => AccountStatus::Disabled,
		}
	}
}

impl From<PoolAccountDetails> for AccountDetailsResponse {
	fn from(details: PoolAccountDetails) -> Self {
		Self {
			id: details.id,
			status: details.status.into(),
			cooldown_remaining_secs: details.cooldown_remaining_secs,
			last_error: details.last_error,
			expires_at: details.expires_at,
		}
	}
}
