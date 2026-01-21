// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Shared validation utilities for API handlers.
//!
//! This module provides common validation functions for slugs, emails, and IDs.
//! Use these utilities to ensure consistent validation across all handlers.

use loom_server_auth::types::{OrgId, OrgRole, TeamId, TeamRole, UserId};
use regex::Regex;
use std::sync::LazyLock;
use uuid::Uuid;

static SLUG_REGEX: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$").unwrap());

/// Validate a slug against format and length constraints.
///
/// Slugs must:
/// - Be between `min_len` and `max_len` characters
/// - Start and end with alphanumeric characters
/// - Contain only lowercase letters, numbers, and hyphens
pub fn validate_slug(slug: &str, min_len: usize, max_len: usize) -> bool {
	slug.len() >= min_len && slug.len() <= max_len && SLUG_REGEX.is_match(slug)
}

/// Sanitize an email address by trimming whitespace and lowercasing.
pub fn sanitize_email(email: &str) -> String {
	email.trim().to_lowercase()
}

/// Error type for ID parsing failures.
#[derive(Debug, Clone)]
pub struct IdParseError {
	pub error: String,
	pub message: String,
}

impl IdParseError {
	pub fn invalid_org_id(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_id".to_string(),
			message: message.into(),
		}
	}

	pub fn invalid_team_id(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_id".to_string(),
			message: message.into(),
		}
	}

	pub fn invalid_user_id(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_id".to_string(),
			message: message.into(),
		}
	}

	pub fn invalid_uuid(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_id".to_string(),
			message: message.into(),
		}
	}
}

/// Parse a string as an OrgId.
///
/// Returns an error with a localized message if parsing fails.
pub fn parse_org_id(id_str: &str, error_message: &str) -> Result<OrgId, IdParseError> {
	Uuid::parse_str(id_str)
		.map(OrgId::new)
		.map_err(|_| IdParseError::invalid_org_id(error_message))
}

/// Parse a string as a TeamId.
///
/// Returns an error with a localized message if parsing fails.
pub fn parse_team_id(id_str: &str, error_message: &str) -> Result<TeamId, IdParseError> {
	Uuid::parse_str(id_str)
		.map(TeamId::new)
		.map_err(|_| IdParseError::invalid_team_id(error_message))
}

/// Parse a string as a UserId.
///
/// Returns an error with a localized message if parsing fails.
pub fn parse_user_id(id_str: &str, error_message: &str) -> Result<UserId, IdParseError> {
	Uuid::parse_str(id_str)
		.map(UserId::new)
		.map_err(|_| IdParseError::invalid_user_id(error_message))
}

/// Parse a string as a raw UUID.
///
/// Returns an error with a localized message if parsing fails.
pub fn parse_uuid(id_str: &str, error_message: &str) -> Result<Uuid, IdParseError> {
	Uuid::parse_str(id_str).map_err(|_| IdParseError::invalid_uuid(error_message))
}

/// Error type for role parsing failures.
#[derive(Debug, Clone)]
pub struct RoleParseError {
	pub error: String,
	pub message: String,
}

impl RoleParseError {
	pub fn invalid_org_role(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_role".to_string(),
			message: message.into(),
		}
	}

	pub fn invalid_team_role(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_role".to_string(),
			message: message.into(),
		}
	}
}

/// Parse a string as an OrgRole.
///
/// Accepts: "owner", "admin", "member" (case-insensitive)
pub fn parse_org_role(role_str: &str, error_message: &str) -> Result<OrgRole, RoleParseError> {
	match role_str.to_lowercase().as_str() {
		"owner" => Ok(OrgRole::Owner),
		"admin" => Ok(OrgRole::Admin),
		"member" => Ok(OrgRole::Member),
		_ => Err(RoleParseError::invalid_org_role(error_message)),
	}
}

/// Parse a string as a TeamRole.
///
/// Accepts: "maintainer", "member" (case-insensitive)
pub fn parse_team_role(role_str: &str, error_message: &str) -> Result<TeamRole, RoleParseError> {
	match role_str.to_lowercase().as_str() {
		"maintainer" => Ok(TeamRole::Maintainer),
		"member" => Ok(TeamRole::Member),
		_ => Err(RoleParseError::invalid_team_role(error_message)),
	}
}

/// Slug validation result with structured error info.
#[derive(Debug, Clone)]
pub struct SlugValidationError {
	pub error: String,
	pub message: String,
}

impl SlugValidationError {
	pub fn invalid_length(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_slug".to_string(),
			message: message.into(),
		}
	}

	pub fn invalid_format(message: impl Into<String>) -> Self {
		Self {
			error: "invalid_slug".to_string(),
			message: message.into(),
		}
	}
}

/// Validate and return a structured error for slug validation.
///
/// # Arguments
/// * `slug` - The slug to validate
/// * `min_len` - Minimum allowed length
/// * `max_len` - Maximum allowed length
/// * `length_error` - Error message for length violations
/// * `format_error` - Error message for format violations
pub fn validate_slug_with_error(
	slug: &str,
	min_len: usize,
	max_len: usize,
	length_error: &str,
	format_error: &str,
) -> Result<(), SlugValidationError> {
	if slug.len() < min_len || slug.len() > max_len {
		return Err(SlugValidationError::invalid_length(length_error));
	}
	if !SLUG_REGEX.is_match(slug) {
		return Err(SlugValidationError::invalid_format(format_error));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_validate_slug() {
		assert!(validate_slug("a", 1, 50));
		assert!(validate_slug("abc", 1, 50));
		assert!(validate_slug("abc-def", 1, 50));
		assert!(validate_slug("a1b2c3", 1, 50));

		assert!(!validate_slug("", 1, 50));
		assert!(!validate_slug("-abc", 1, 50));
		assert!(!validate_slug("abc-", 1, 50));
		assert!(!validate_slug("ABC", 1, 50));
		assert!(!validate_slug("ab", 3, 50));
	}

	#[test]
	fn test_sanitize_email() {
		assert_eq!(sanitize_email("  Test@Example.COM  "), "test@example.com");
	}

	#[test]
	fn test_parse_org_id() {
		let valid = "550e8400-e29b-41d4-a716-446655440000";
		let result = parse_org_id(valid, "Invalid org ID");
		assert!(result.is_ok());

		let invalid = "not-a-uuid";
		let result = parse_org_id(invalid, "Invalid org ID");
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, "invalid_id");
	}

	#[test]
	fn test_parse_team_id() {
		let valid = "550e8400-e29b-41d4-a716-446655440000";
		let result = parse_team_id(valid, "Invalid team ID");
		assert!(result.is_ok());

		let invalid = "not-a-uuid";
		let result = parse_team_id(invalid, "Invalid team ID");
		assert!(result.is_err());
	}

	#[test]
	fn test_parse_user_id() {
		let valid = "550e8400-e29b-41d4-a716-446655440000";
		let result = parse_user_id(valid, "Invalid user ID");
		assert!(result.is_ok());

		let invalid = "not-a-uuid";
		let result = parse_user_id(invalid, "Invalid user ID");
		assert!(result.is_err());
	}

	#[test]
	fn test_parse_uuid() {
		let valid = "550e8400-e29b-41d4-a716-446655440000";
		let result = parse_uuid(valid, "Invalid UUID");
		assert!(result.is_ok());

		let invalid = "not-a-uuid";
		let result = parse_uuid(invalid, "Invalid UUID");
		assert!(result.is_err());
	}

	#[test]
	fn test_parse_org_role() {
		assert_eq!(parse_org_role("owner", "err").unwrap(), OrgRole::Owner);
		assert_eq!(parse_org_role("OWNER", "err").unwrap(), OrgRole::Owner);
		assert_eq!(parse_org_role("admin", "err").unwrap(), OrgRole::Admin);
		assert_eq!(parse_org_role("Admin", "err").unwrap(), OrgRole::Admin);
		assert_eq!(parse_org_role("member", "err").unwrap(), OrgRole::Member);
		assert_eq!(parse_org_role("MEMBER", "err").unwrap(), OrgRole::Member);

		let result = parse_org_role("invalid", "Invalid role");
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, "invalid_role");
	}

	#[test]
	fn test_parse_team_role() {
		assert_eq!(
			parse_team_role("maintainer", "err").unwrap(),
			TeamRole::Maintainer
		);
		assert_eq!(
			parse_team_role("MAINTAINER", "err").unwrap(),
			TeamRole::Maintainer
		);
		assert_eq!(parse_team_role("member", "err").unwrap(), TeamRole::Member);
		assert_eq!(parse_team_role("MEMBER", "err").unwrap(), TeamRole::Member);

		let result = parse_team_role("invalid", "Invalid role");
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().error, "invalid_role");
	}

	#[test]
	fn test_validate_slug_with_error() {
		assert!(validate_slug_with_error("abc", 1, 50, "length err", "format err").is_ok());
		assert!(validate_slug_with_error("abc-def", 1, 50, "length err", "format err").is_ok());

		let result = validate_slug_with_error("ab", 3, 50, "too short", "format err");
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().message, "too short");

		let result = validate_slug_with_error("-abc", 1, 50, "length err", "bad format");
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().message, "bad format");
	}
}
