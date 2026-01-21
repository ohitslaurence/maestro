// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

// ============================================================================
// Environment Types
// ============================================================================

/// An environment in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EnvironmentResponse {
	/// Unique identifier for the environment.
	pub id: String,
	/// Organization ID this environment belongs to.
	pub org_id: String,
	/// Environment name (e.g., "dev", "prod").
	pub name: String,
	/// Optional hex color code (e.g., "#10b981").
	pub color: Option<String>,
	/// When the environment was created.
	pub created_at: DateTime<Utc>,
}

/// Request to create a new environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateEnvironmentRequest {
	/// Environment name (lowercase alphanumeric with underscores, 2-50 chars).
	pub name: String,
	/// Optional hex color code (e.g., "#10b981").
	pub color: Option<String>,
}

/// Request to update an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateEnvironmentRequest {
	/// New environment name (optional).
	pub name: Option<String>,
	/// New hex color code (optional).
	pub color: Option<String>,
}

/// Response for listing environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListEnvironmentsResponse {
	pub environments: Vec<EnvironmentResponse>,
}

// ============================================================================
// SDK Key Types
// ============================================================================

/// SDK key type for API requests/responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SdkKeyTypeApi {
	/// Safe for browser, single user context.
	ClientSide,
	/// Secret, backend only, any user context.
	ServerSide,
}

/// An SDK key in API responses (without the secret key).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SdkKeyResponse {
	/// Unique identifier for the SDK key.
	pub id: String,
	/// Environment ID this key belongs to.
	pub environment_id: String,
	/// Environment name for display.
	pub environment_name: String,
	/// Type of SDK key (client_side or server_side).
	pub key_type: SdkKeyTypeApi,
	/// Human-readable name for the key.
	pub name: String,
	/// User ID who created the key.
	pub created_by: String,
	/// When the key was created.
	pub created_at: DateTime<Utc>,
	/// When the key was last used.
	pub last_used_at: Option<DateTime<Utc>>,
	/// When the key was revoked (None if active).
	pub revoked_at: Option<DateTime<Utc>>,
}

/// Response for creating a new SDK key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateSdkKeyResponse {
	/// Unique identifier for the SDK key.
	pub id: String,
	/// The actual SDK key value (only shown once!).
	pub key: String,
	/// Environment ID this key belongs to.
	pub environment_id: String,
	/// Type of SDK key.
	pub key_type: SdkKeyTypeApi,
	/// Human-readable name for the key.
	pub name: String,
	/// When the key was created.
	pub created_at: DateTime<Utc>,
}

/// Request to create an SDK key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateSdkKeyRequest {
	/// Environment ID to create the key for.
	pub environment_id: String,
	/// Type of SDK key (client_side or server_side).
	pub key_type: SdkKeyTypeApi,
	/// Human-readable name for the key.
	pub name: String,
}

/// Response for listing SDK keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListSdkKeysResponse {
	pub sdk_keys: Vec<SdkKeyResponse>,
}

// ============================================================================
// Flag Types
// ============================================================================

/// Variant value type in API requests/responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "type", content = "value")]
pub enum VariantValueApi {
	/// Boolean value (true/false).
	Boolean(bool),
	/// String value.
	String(String),
	/// JSON value.
	Json(serde_json::Value),
}

/// A variant of a feature flag in API requests/responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct VariantApi {
	/// Variant name (e.g., "control", "treatment_a").
	pub name: String,
	/// The value of this variant.
	pub value: VariantValueApi,
	/// Weight for percentage-based distribution.
	pub weight: u32,
}

/// A prerequisite for a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagPrerequisiteApi {
	/// Key of the prerequisite flag.
	pub flag_key: String,
	/// Required variant of the prerequisite flag.
	pub required_variant: String,
}

/// A feature flag in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagResponse {
	/// Unique identifier for the flag.
	pub id: String,
	/// Organization ID (None for platform flags).
	pub org_id: Option<String>,
	/// Structured key (e.g., "checkout.new_flow").
	pub key: String,
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Tags for categorization.
	pub tags: Vec<String>,
	/// User ID of the maintainer.
	pub maintainer_user_id: Option<String>,
	/// Available variants.
	pub variants: Vec<VariantApi>,
	/// Default variant name.
	pub default_variant: String,
	/// Prerequisites for this flag.
	pub prerequisites: Vec<FlagPrerequisiteApi>,
	/// Whether the flag is archived.
	pub is_archived: bool,
	/// When the flag was created.
	pub created_at: DateTime<Utc>,
	/// When the flag was last updated.
	pub updated_at: DateTime<Utc>,
	/// When the flag was archived (if archived).
	pub archived_at: Option<DateTime<Utc>>,
}

/// Request to create a new flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateFlagRequest {
	/// Structured key (e.g., "checkout.new_flow"). Must be lowercase alphanumeric
	/// with dots and underscores, 3-100 characters.
	pub key: String,
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Tags for categorization.
	#[serde(default)]
	pub tags: Vec<String>,
	/// User ID of the maintainer.
	pub maintainer_user_id: Option<String>,
	/// Available variants.
	pub variants: Vec<VariantApi>,
	/// Default variant name (must exist in variants).
	pub default_variant: String,
	/// Prerequisites for this flag.
	#[serde(default)]
	pub prerequisites: Vec<FlagPrerequisiteApi>,
}

/// Request to update a flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateFlagRequest {
	/// Human-readable name.
	pub name: Option<String>,
	/// Description.
	pub description: Option<String>,
	/// Tags for categorization.
	pub tags: Option<Vec<String>>,
	/// User ID of the maintainer.
	pub maintainer_user_id: Option<String>,
	/// Available variants.
	pub variants: Option<Vec<VariantApi>>,
	/// Default variant name (must exist in variants).
	pub default_variant: Option<String>,
	/// Prerequisites for this flag.
	pub prerequisites: Option<Vec<FlagPrerequisiteApi>>,
}

/// Response for listing flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListFlagsResponse {
	pub flags: Vec<FlagResponse>,
}

/// Query parameters for listing flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListFlagsQuery {
	/// Include archived flags (default: false).
	#[serde(default)]
	pub include_archived: bool,
}

// ============================================================================
// Flag Config Types
// ============================================================================

/// Per-environment configuration for a flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagConfigResponse {
	/// Unique identifier for the config.
	pub id: String,
	/// Flag ID this config belongs to.
	pub flag_id: String,
	/// Environment ID.
	pub environment_id: String,
	/// Environment name for display.
	pub environment_name: String,
	/// Whether the flag is enabled in this environment.
	pub enabled: bool,
	/// Strategy ID (optional).
	pub strategy_id: Option<String>,
	/// When the config was created.
	pub created_at: DateTime<Utc>,
	/// When the config was last updated.
	pub updated_at: DateTime<Utc>,
}

/// Request to update a flag config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateFlagConfigRequest {
	/// Whether the flag is enabled in this environment.
	pub enabled: Option<bool>,
	/// Strategy ID (set to null to clear).
	#[serde(default, deserialize_with = "deserialize_optional_nullable")]
	pub strategy_id: Option<Option<String>>,
}

/// Response for listing flag configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListFlagConfigsResponse {
	pub configs: Vec<FlagConfigResponse>,
}

fn deserialize_optional_nullable<'de, D>(
	deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let opt = Option::<Option<String>>::deserialize(deserializer)?;
	Ok(opt)
}

// ============================================================================
// Strategy Types
// ============================================================================

/// A condition type for API requests/responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "type")]
pub enum ConditionApi {
	/// Attribute-based condition.
	Attribute {
		/// The attribute name (e.g., "plan", "created_at").
		attribute: String,
		/// The comparison operator.
		operator: AttributeOperatorApi,
		/// The value to compare against.
		value: serde_json::Value,
	},
	/// Geographic-based condition.
	Geographic {
		/// The geographic field to check.
		field: GeoFieldApi,
		/// The comparison operator.
		operator: GeoOperatorApi,
		/// The values to compare against (e.g., ["US", "CA"]).
		values: Vec<String>,
	},
	/// Environment-based condition.
	Environment {
		/// The environments this condition applies to.
		environments: Vec<String>,
	},
}

/// Operators for attribute conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AttributeOperatorApi {
	Equals,
	NotEquals,
	Contains,
	StartsWith,
	EndsWith,
	GreaterThan,
	LessThan,
	GreaterThanOrEquals,
	LessThanOrEquals,
	In,
	NotIn,
}

/// Geographic targeting field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GeoFieldApi {
	Country,
	Region,
	City,
}

/// Geographic targeting operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GeoOperatorApi {
	In,
	NotIn,
}

/// The key used for percentage-based distribution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PercentageKeyApi {
	#[default]
	UserId,
	OrgId,
	SessionId,
	Custom(String),
}

/// A step in a rollout schedule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ScheduleStepApi {
	/// The percentage at this step (0-100).
	pub percentage: u32,
	/// When this step starts.
	pub start_at: DateTime<Utc>,
}

/// A schedule for gradual rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ScheduleApi {
	/// The steps in the schedule.
	pub steps: Vec<ScheduleStepApi>,
}

/// A strategy in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct StrategyResponse {
	/// Unique identifier for the strategy.
	pub id: String,
	/// Organization ID (None for platform strategies).
	pub org_id: Option<String>,
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Targeting conditions (all must match).
	pub conditions: Vec<ConditionApi>,
	/// Percentage rollout (0-100).
	pub percentage: Option<u32>,
	/// The key used for percentage-based distribution.
	pub percentage_key: PercentageKeyApi,
	/// Optional rollout schedule.
	pub schedule: Option<ScheduleApi>,
	/// When the strategy was created.
	pub created_at: DateTime<Utc>,
	/// When the strategy was last updated.
	pub updated_at: DateTime<Utc>,
}

/// Request to create a new strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateStrategyRequest {
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Targeting conditions (all must match).
	#[serde(default)]
	pub conditions: Vec<ConditionApi>,
	/// Percentage rollout (0-100).
	pub percentage: Option<u32>,
	/// The key used for percentage-based distribution.
	#[serde(default)]
	pub percentage_key: PercentageKeyApi,
	/// Optional rollout schedule.
	pub schedule: Option<ScheduleApi>,
}

/// Request to update a strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateStrategyRequest {
	/// Human-readable name.
	pub name: Option<String>,
	/// Description.
	pub description: Option<String>,
	/// Targeting conditions (all must match).
	pub conditions: Option<Vec<ConditionApi>>,
	/// Percentage rollout (0-100).
	pub percentage: Option<Option<u32>>,
	/// The key used for percentage-based distribution.
	pub percentage_key: Option<PercentageKeyApi>,
	/// Optional rollout schedule.
	pub schedule: Option<Option<ScheduleApi>>,
}

/// Response for listing strategies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListStrategiesResponse {
	pub strategies: Vec<StrategyResponse>,
}

// ============================================================================
// Kill Switch Types
// ============================================================================

/// A kill switch in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct KillSwitchResponse {
	/// Unique identifier for the kill switch.
	pub id: String,
	/// Organization ID (None for platform kill switches).
	pub org_id: Option<String>,
	/// Structured key (e.g., "disable_checkout").
	pub key: String,
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Flag keys affected by this kill switch.
	pub linked_flag_keys: Vec<String>,
	/// Whether the kill switch is currently active.
	pub is_active: bool,
	/// When the kill switch was activated.
	pub activated_at: Option<DateTime<Utc>>,
	/// User ID who activated the kill switch.
	pub activated_by: Option<String>,
	/// Reason for activation.
	pub activation_reason: Option<String>,
	/// When the kill switch was created.
	pub created_at: DateTime<Utc>,
	/// When the kill switch was last updated.
	pub updated_at: DateTime<Utc>,
}

/// Request to create a new kill switch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct CreateKillSwitchRequest {
	/// Structured key (e.g., "disable_checkout"). Must be lowercase alphanumeric
	/// with underscores, 3-100 characters.
	pub key: String,
	/// Human-readable name.
	pub name: String,
	/// Optional description.
	pub description: Option<String>,
	/// Flag keys affected by this kill switch.
	#[serde(default)]
	pub linked_flag_keys: Vec<String>,
}

/// Request to update a kill switch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct UpdateKillSwitchRequest {
	/// Human-readable name.
	pub name: Option<String>,
	/// Description.
	pub description: Option<String>,
	/// Flag keys affected by this kill switch.
	pub linked_flag_keys: Option<Vec<String>>,
}

/// Request to activate a kill switch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ActivateKillSwitchRequest {
	/// Required reason for activation. This is mandatory for audit purposes.
	pub reason: String,
}

/// Response for listing kill switches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListKillSwitchesResponse {
	pub kill_switches: Vec<KillSwitchResponse>,
}

// ============================================================================
// Common Response Types
// ============================================================================

/// Success response for flags operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagsSuccessResponse {
	pub message: String,
}

/// Error response for flags operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagsErrorResponse {
	pub error: String,
	pub message: String,
}

// ============================================================================
// Evaluation Types
// ============================================================================

/// Geographic context for flag evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct GeoContextApi {
	/// ISO 3166-1 alpha-2 country code (e.g., "US").
	pub country: Option<String>,
	/// Region/state code (e.g., "CA" for California).
	pub region: Option<String>,
	/// City name.
	pub city: Option<String>,
}

/// Context for evaluating feature flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EvaluationContextApi {
	/// User ID for user-level targeting.
	pub user_id: Option<String>,
	/// Organization ID for org-level targeting.
	pub org_id: Option<String>,
	/// Session ID for session-level targeting.
	pub session_id: Option<String>,
	/// Environment name (e.g., "prod", "dev").
	pub environment: String,
	/// Custom attributes for targeting rules.
	#[serde(default)]
	pub attributes: std::collections::HashMap<String, serde_json::Value>,
	/// Geographic context (optional, server may resolve from IP).
	pub geo: Option<GeoContextApi>,
}

/// The reason for an evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(tag = "type")]
pub enum EvaluationReasonApi {
	/// Default variant (no strategy matched).
	Default,
	/// Strategy determined the variant.
	Strategy {
		/// ID of the strategy that matched.
		strategy_id: String,
	},
	/// Kill switch forced the flag off.
	KillSwitch {
		/// ID of the kill switch that is active.
		kill_switch_id: String,
	},
	/// Prerequisite flag not met.
	Prerequisite {
		/// Key of the missing prerequisite flag.
		missing_flag: String,
	},
	/// Flag is disabled in this environment.
	Disabled,
	/// An error occurred during evaluation.
	Error {
		/// Error message.
		message: String,
	},
}

/// Result of evaluating a single flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EvaluationResultApi {
	/// The flag key that was evaluated.
	pub flag_key: String,
	/// The variant that was selected.
	pub variant: String,
	/// The value of the selected variant.
	pub value: VariantValueApi,
	/// The reason for this evaluation result.
	pub reason: EvaluationReasonApi,
}

/// Request to evaluate a single flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EvaluateFlagRequest {
	/// The evaluation context.
	pub context: EvaluationContextApi,
}

/// Request to evaluate all flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EvaluateAllFlagsRequest {
	/// The evaluation context.
	pub context: EvaluationContextApi,
}

/// Response for evaluating all flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct EvaluateAllFlagsResponse {
	/// Evaluation results for all flags.
	pub results: Vec<EvaluationResultApi>,
	/// When the evaluation was performed.
	pub evaluated_at: DateTime<Utc>,
}

// ============================================================================
// Flag Stats Types
// ============================================================================

/// Statistics for a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FlagStatsResponse {
	/// The flag key these stats are for.
	pub flag_key: String,
	/// When the flag was last evaluated (None if never evaluated).
	pub last_evaluated_at: Option<DateTime<Utc>>,
	/// Number of evaluations in the last 24 hours.
	pub evaluation_count_24h: u64,
	/// Number of evaluations in the last 7 days.
	pub evaluation_count_7d: u64,
	/// Number of evaluations in the last 30 days.
	pub evaluation_count_30d: u64,
}

/// A stale flag entry in the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct StaleFlagResponse {
	/// The flag ID.
	pub flag_id: String,
	/// The flag key.
	pub flag_key: String,
	/// Human-readable flag name.
	pub name: String,
	/// When the flag was last evaluated (None if never evaluated).
	pub last_evaluated_at: Option<DateTime<Utc>>,
	/// Number of days since the flag was last evaluated.
	pub days_since_evaluated: Option<i64>,
}

/// Response for listing stale flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListStaleFlagsResponse {
	/// List of stale flags.
	pub stale_flags: Vec<StaleFlagResponse>,
	/// The threshold used to determine staleness (in days).
	pub stale_threshold_days: u32,
}
