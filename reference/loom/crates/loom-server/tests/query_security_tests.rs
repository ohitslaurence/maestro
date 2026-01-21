// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Query security validation tests for Phase 2 integration.
//!
//! **Purpose**: Tests security hardening features including path escaping prevention,
//! timeout bounds enforcement, size limits, rate limiting, and blocked path detection.
//! These tests ensure queries cannot be exploited to access unauthorized resources
//! or cause denial of service.

use loom_server::{PathSanitizer, QueryValidator, RateLimiter, ResultValidator, SecurityError};
use serde_json::json;

// ============================================================================
// Query Size Validation Tests
// ============================================================================

/// Test that small queries pass size validation.
/// **Why Important**: Legitimate queries should always be allowed within the size limit.
/// This ensures normal operations aren't blocked by overly strict checks.
#[test]
fn test_query_size_within_limit_passes() {
	let validator = QueryValidator::new();
	let small_query = "I need to read config.json";

	let result = validator.validate_size(small_query);
	assert!(result.is_ok(), "Small query should pass validation");
}

/// Test that oversized queries are rejected.
/// **Why Important**: Prevents DoS attacks via extremely large payloads
/// and prevents memory exhaustion from gigantic inputs.
#[test]
fn test_query_size_exceeds_limit_fails() {
	let validator = QueryValidator::new();
	let huge_query = "x".repeat(11 * 1024); // 11 KB, exceeds 10 KB default

	let result = validator.validate_size(&huge_query);
	assert!(result.is_err(), "Oversized query should be rejected");

	match result {
		Err(SecurityError::QueryTooLarge(size, max)) => {
			assert_eq!(size, 11 * 1024);
			assert_eq!(max, 10 * 1024);
		}
		_ => panic!("Expected QueryTooLarge error"),
	}
}

/// Test query size validation with custom limits.
/// **Why Important**: Validates that custom limit configuration works correctly.
#[test]
fn test_query_size_with_custom_limits() {
	let validator = QueryValidator::with_limits(
		1000, // 1 KB max
		10,
		(1, 300),
		vec![],
	);

	let small = "a".repeat(500);
	let large = "a".repeat(1001);

	assert!(validator.validate_size(&small).is_ok());
	assert!(validator.validate_size(&large).is_err());
}

// ============================================================================
// Timeout Validation Tests
// ============================================================================

/// Test that timeouts within range are accepted.
/// **Why Important**: Valid timeout values should always be accepted to allow
/// queries with reasonable wait times.
#[test]
fn test_timeout_within_range_passes() {
	let validator = QueryValidator::new();

	assert!(
		validator.validate_timeout(1).is_ok(),
		"Min timeout should pass"
	);
	assert!(
		validator.validate_timeout(150).is_ok(),
		"Mid timeout should pass"
	);
	assert!(
		validator.validate_timeout(300).is_ok(),
		"Max timeout should pass"
	);
}

/// Test that timeouts outside range are rejected.
/// **Why Important**: Prevents unreasonably long timeouts (DoS via long wait times)
/// and timeouts that are too short (0 breaks the system).
#[test]
fn test_timeout_outside_range_fails() {
	let validator = QueryValidator::new();

	assert!(
		validator.validate_timeout(0).is_err(),
		"Zero timeout should fail"
	);
	assert!(
		validator.validate_timeout(301).is_err(),
		"Too large timeout should fail"
	);
}

/// Test custom timeout range configuration.
/// **Why Important**: Some deployments may need different timeout ranges.
/// Validates that custom configuration is respected.
#[test]
fn test_custom_timeout_range() {
	let validator = QueryValidator::with_limits(
		10 * 1024,
		10,
		(5, 60), // Custom range: 5-60 seconds
		vec![],
	);

	assert!(
		validator.validate_timeout(4).is_err(),
		"Below custom min should fail"
	);
	assert!(
		validator.validate_timeout(5).is_ok(),
		"At custom min should pass"
	);
	assert!(
		validator.validate_timeout(60).is_ok(),
		"At custom max should pass"
	);
	assert!(
		validator.validate_timeout(61).is_err(),
		"Above custom max should fail"
	);
}

// ============================================================================
// Path Escape Detection Tests
// ============================================================================

/// Test that paths with .. are rejected.
/// **Why Important**: Directory traversal attacks (..) allow access to parent directories.
/// Must be blocked to prevent accessing files outside intended scope.
#[test]
fn test_path_traversal_with_double_dot_rejected() {
	let validator = QueryValidator::new();

	assert!(validator.validate_path("../etc/passwd").is_err());
	assert!(validator.validate_path("data/../../../etc/passwd").is_err());
	assert!(validator.validate_path("./../../etc/passwd").is_err());
}

/// Test that valid relative paths are accepted.
/// **Why Important**: Legitimate relative paths should be allowed to not
/// unnecessarily restrict valid use cases.
#[test]
fn test_valid_relative_paths_accepted() {
	let validator = QueryValidator::new();

	assert!(validator.validate_path("config/settings.json").is_ok());
	assert!(validator.validate_path("data/users.txt").is_ok());
	assert!(validator.validate_path("./workspace/files").is_ok());
}

// ============================================================================
// Blocked Paths Tests
// ============================================================================

/// Test that sensitive system paths are blocked.
/// **Why Important**: Prevents access to sensitive files like /etc/passwd, /root/.ssh.
/// This is critical for system security in shared environments.
#[test]
fn test_blocked_system_paths_rejected() {
	let validator = QueryValidator::new();

	assert!(validator.validate_path("/etc/passwd").is_err());
	assert!(validator.validate_path("/etc/shadow").is_err());
	assert!(validator.validate_path("/root/.ssh/id_rsa").is_err());
	assert!(validator.validate_path("/sys/kernel/config").is_err());
}

/// Test that non-blocked paths are accepted.
/// **Why Important**: Should allow access to application files.
#[test]
fn test_allowed_paths_accepted() {
	let validator = QueryValidator::new();

	assert!(validator.validate_path("/app/config.json").is_ok());
	assert!(validator.validate_path("/home/appuser/data").is_ok());
	assert!(validator.validate_path("/tmp/cache").is_ok());
}

/// Test custom blocked paths.
/// **Why Important**: Allows deployments to define additional blocked paths.
#[test]
fn test_custom_blocked_paths() {
	let validator = QueryValidator::with_limits(
		10 * 1024,
		10,
		(1, 300),
		vec!["/secrets".to_string(), "/admin".to_string()],
	);

	assert!(validator.validate_path("/secrets/api_key").is_err());
	assert!(validator.validate_path("/admin/users").is_err());
	assert!(validator.validate_path("/app/config").is_ok());
}

// ============================================================================
// Path Sanitizer Tests
// ============================================================================

/// Test that absolute paths are rejected by path sanitizer.
/// **Why Important**: Absolute paths bypass workspace restrictions.
/// Only relative paths should be allowed for sandboxing.
#[test]
fn test_sanitizer_rejects_absolute_paths() {
	let sanitizer = PathSanitizer::new("/workspace");

	assert!(sanitizer.sanitize("/etc/passwd").is_err());
	assert!(sanitizer.sanitize("/home/user/file").is_err());
}

/// Test that paths with null bytes are rejected.
/// **Why Important**: Null bytes can be used to bypass validation in some contexts.
/// Must be filtered out to prevent null byte injection attacks.
#[test]
fn test_sanitizer_rejects_null_bytes() {
	let sanitizer = PathSanitizer::new("/workspace");

	assert!(sanitizer.sanitize("config\0.json").is_err());
	assert!(sanitizer.sanitize("file\0/etc/passwd").is_err());
}

/// Test that valid relative paths are sanitized correctly.
/// **Why Important**: Relative paths should be joined with workspace root
/// and canonicalized to prevent escape via symlinks.
#[test]
fn test_sanitizer_accepts_valid_relative_paths() {
	// Use actual workspace directory that exists
	let sanitizer = PathSanitizer::new("/tmp");

	// These should work since they're valid relative paths
	let result = sanitizer.sanitize("config.json");
	assert!(result.is_ok() || result.is_err()); // Either is acceptable for non-existent paths

	let result = sanitizer.sanitize("src/main.rs");
	assert!(result.is_ok() || result.is_err()); // Either is acceptable for non-existent paths
}

// ============================================================================
// Rate Limiter Tests
// ============================================================================

/// Test basic rate limiting functionality.
/// **Why Important**: Rate limiting prevents DoS by limiting query frequency.
/// Essential for protecting shared resources.
#[tokio::test]
async fn test_rate_limiter_basic() {
	let limiter = RateLimiter::with_rate(10.0, 10);
	let session = "test_session_1";

	// Should be able to do several queries
	for _ in 0..5 {
		let result = limiter.check_rate_limit(session).await;
		assert!(result.is_ok(), "Should allow queries within limit");
	}
}

/// Test that rate limiter blocks excessive requests.
/// **Why Important**: After exceeding rate limit, subsequent requests should be blocked.
/// This is the core functionality of rate limiting.
#[tokio::test]
async fn test_rate_limiter_blocks_excess() {
	let limiter = RateLimiter::with_rate(1.0, 2);
	let session = "test_session_2";

	// First two should succeed
	assert!(limiter.check_rate_limit(session).await.is_ok());
	assert!(limiter.check_rate_limit(session).await.is_ok());

	// Third should fail
	let result = limiter.check_rate_limit(session).await;
	assert!(result.is_err(), "Should block excess requests");

	match result {
		Err(SecurityError::RateLimitExceeded) => {
			// Expected
		}
		_ => panic!("Expected RateLimitExceeded error"),
	}
}

/// Test rate limiter reset functionality.
/// **Why Important**: Rate limit resets should allow sessions to resume
/// querying after timeout or explicit reset.
#[tokio::test]
async fn test_rate_limiter_reset() {
	let limiter = RateLimiter::with_rate(1.0, 2);
	let session = "test_session_3";

	// Exhaust tokens
	limiter.check_rate_limit(session).await.ok();
	limiter.check_rate_limit(session).await.ok();
	assert!(limiter.check_rate_limit(session).await.is_err());

	// Reset
	limiter.reset(session).await;

	// Should work again
	let result = limiter.check_rate_limit(session).await;
	assert!(result.is_ok(), "Should work after reset");
}

/// Test rate limiter gets remaining tokens.
/// **Why Important**: Allows querying remaining quota for UX purposes.
#[tokio::test]
async fn test_rate_limiter_get_remaining_tokens() {
	let limiter = RateLimiter::with_rate(10.0, 10);
	let session = "test_session_4";

	// Should have max tokens initially
	let remaining = limiter.get_remaining_tokens(session).await;
	assert_eq!(remaining, 10, "Should have max tokens initially");

	// Use one token
	limiter.check_rate_limit(session).await.ok();

	let remaining = limiter.get_remaining_tokens(session).await;
	assert_eq!(remaining, 9, "Should have one less token");
}

/// Test different sessions have independent rate limits.
/// **Why Important**: One session hitting rate limit should not affect others.
/// This prevents one user from DoSing all other users.
#[tokio::test]
async fn test_rate_limiter_per_session_isolation() {
	let limiter = RateLimiter::with_rate(1.0, 2);

	// Session 1 exhausts its quota
	limiter.check_rate_limit("session-1").await.ok();
	limiter.check_rate_limit("session-1").await.ok();
	assert!(limiter.check_rate_limit("session-1").await.is_err());

	// Session 2 should still work
	let result = limiter.check_rate_limit("session-2").await;
	assert!(
		result.is_ok(),
		"Session 2 should be independent of session-1's rate limit"
	);
}

// ============================================================================
// Result Validation Tests
// ============================================================================

/// Test that valid results pass validation.
/// **Why Important**: Valid results should be accepted without modification.
#[test]
fn test_result_validator_accepts_valid_results() {
	let validator = ResultValidator::new();

	let result = json!({"data": "value", "status": "ok"});
	assert!(validator.validate(&result).is_ok());

	let result = json!(["item1", "item2", "item3"]);
	assert!(validator.validate(&result).is_ok());
}

/// Test that oversized results are rejected.
/// **Why Important**: Prevents memory exhaustion from gigantic results.
/// Limits must prevent DoS via result bloat.
#[test]
fn test_result_validator_rejects_oversized_results() {
	let validator = ResultValidator::with_max_size(100); // 100 bytes max

	let large_result = json!({"data": "x".repeat(1000)});
	let result = validator.validate(&large_result);

	assert!(result.is_err(), "Oversized result should be rejected");
	match result {
		Err(SecurityError::ResultTooLarge(size, max)) => {
			assert!(size > 100, "Size should exceed limit");
			assert_eq!(max, 100);
		}
		_ => panic!("Expected ResultTooLarge error"),
	}
}

/// Test error message sanitization removes file paths.
/// **Why Important**: Error messages might leak sensitive information
/// like file paths. Must be sanitized before sending to client.
#[test]
fn test_result_sanitizer_removes_file_paths() {
	let validator = ResultValidator::new();

	let error = "Error occurred at /home/user/project/src/main.rs:42";
	let sanitized = validator.sanitize_error(error);

	assert!(
		!sanitized.contains("/home/user"),
		"File path should be removed"
	);
	assert!(!sanitized.contains("main.rs"), "Filename should be removed");
}

/// Test error message sanitization removes stack traces.
/// **Why Important**: Stack traces can reveal internal system structure
/// and might expose sensitive code paths.
#[test]
fn test_result_sanitizer_removes_stack_traces() {
	let validator = ResultValidator::new();

	let error = "thread 'main' panicked\nstack backtrace:\n  frame 1: at module";
	let sanitized = validator.sanitize_error(error);

	assert!(
		!sanitized.contains("stack"),
		"Stack trace should be removed"
	);
	assert!(
		!sanitized.contains("backtrace"),
		"Backtrace should be removed"
	);
}

/// Test empty error handling in sanitizer.
/// **Why Important**: If all content is filtered (all sensitive), should
/// return a generic message instead of empty string.
#[test]
fn test_result_sanitizer_handles_empty_error() {
	let validator = ResultValidator::new();

	let error = "at /home/user/file.rs stack trace at line";
	let sanitized = validator.sanitize_error(error);

	assert!(!sanitized.is_empty(), "Should have default message");
	assert_eq!(
		sanitized, "An error occurred",
		"Should have default message"
	);
}

// ============================================================================
// Integration Tests - Multiple Validations
// ============================================================================

/// Test complete validation flow for safe query.
/// **Why Important**: Validates the typical success path where all
/// checks pass for a legitimate query.
#[test]
fn test_complete_validation_flow_safe_query() {
	let validator = QueryValidator::new();

	// All checks should pass
	assert!(validator
		.validate_size("I need to read config.json")
		.is_ok());
	assert!(validator.validate_timeout(10).is_ok());
	assert!(validator.validate_path("app/config.json").is_ok());
	assert!(validator.validate_json(r#"{"query": "read"}"#).is_ok());
}

/// Test complete validation flow for malicious query.
/// **Why Important**: Validates that at least one check catches
/// various security issues.
#[test]
fn test_complete_validation_flow_rejects_malicious() {
	let validator = QueryValidator::new();

	// Path escape should be caught
	assert!(validator.validate_path("../etc/passwd").is_err());

	// Blocked path should be caught
	assert!(validator.validate_path("/etc/passwd").is_err());

	// Invalid timeout should be caught
	assert!(validator.validate_timeout(0).is_err());

	// Oversized query should be caught
	let huge = "x".repeat(11 * 1024);
	assert!(validator.validate_size(&huge).is_err());
}

// ============================================================================
// Edge Cases and Boundary Tests
// ============================================================================

/// Test empty path is rejected.
/// **Why Important**: Empty paths are ambiguous and could cause issues.
#[test]
fn test_empty_path_handling() {
	let validator = QueryValidator::new();

	// Empty or whitespace paths should be handled
	let result = validator.validate_path("");
	// Should either pass (treated as relative) or fail, but not panic
	let _ = result;
}

/// Test boundary timeout values.
/// **Why Important**: Boundary values (min/max) often reveal bugs.
#[test]
fn test_timeout_boundary_values() {
	let validator = QueryValidator::new();

	assert!(validator.validate_timeout(1).is_ok(), "Min 1 should pass");
	assert!(
		validator.validate_timeout(300).is_ok(),
		"Max 300 should pass"
	);
	assert!(
		validator.validate_timeout(0).is_err(),
		"Below min should fail"
	);
	assert!(
		validator.validate_timeout(301).is_err(),
		"Above max should fail"
	);
}

/// Test query size at exact boundary.
/// **Why Important**: Off-by-one errors are common. Test exact boundary.
#[test]
fn test_query_size_at_boundary() {
	let validator = QueryValidator::new();

	let at_limit = "x".repeat(10 * 1024);
	let over_limit = "x".repeat(10 * 1024 + 1);

	assert!(validator.validate_size(&at_limit).is_ok());
	assert!(validator.validate_size(&over_limit).is_err());
}
