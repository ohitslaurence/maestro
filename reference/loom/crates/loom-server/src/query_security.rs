// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Security hardening and validation for server queries.
//!
//! This module provides validation, sanitization, and rate limiting for server
//! queries to prevent security vulnerabilities and abuse.

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Security validation error types.
#[derive(Debug, Error, Clone)]
pub enum SecurityError {
	/// Query exceeds maximum size limit.
	#[error("Query exceeds maximum size: {0} bytes > {1} bytes")]
	QueryTooLarge(usize, usize),

	/// Query limit exceeded for session.
	#[error("Query limit exceeded for session: {0}/{1}")]
	QueryLimitExceeded(u32, u32),

	/// Timeout outside acceptable range.
	#[error("Timeout {0}s outside acceptable range {1}-{2}s")]
	InvalidTimeout(u32, u32, u32),

	/// Path attempts to escape workspace.
	#[error("Path escape detected: {0}")]
	PathEscapeAttempt(String),

	/// Path is blocked by security policy.
	#[error("Path is blocked: {0}")]
	BlockedPath(String),

	/// Rate limit exceeded.
	#[error("Rate limit exceeded")]
	RateLimitExceeded,

	/// Invalid JSON in payload.
	#[error("Invalid JSON: {0}")]
	InvalidJson(String),

	/// Result size exceeds maximum.
	#[error("Result size {0} bytes exceeds maximum {1} bytes")]
	ResultTooLarge(usize, usize),

	/// Malformed result structure.
	#[error("Malformed result: {0}")]
	MalformedResult(String),

	/// Invalid path (absolute or contains null bytes).
	#[error("Invalid path: {0}")]
	InvalidPath(String),
}

/// Query validator with configurable security constraints.
#[derive(Debug, Clone)]
pub struct QueryValidator {
	max_query_size_bytes: usize,
	max_queries_per_session: u32,
	timeout_range: (u32, u32),
	blocked_paths: Vec<String>,
}

impl QueryValidator {
	/// Create a new query validator with default settings.
	///
	/// Defaults:
	/// - Max query size: 10 KB
	/// - Max queries per session: 10
	/// - Timeout range: 1-300 seconds
	/// - Blocked paths: /etc/*, /root/*, /sys/*
	pub fn new() -> Self {
		Self {
			max_query_size_bytes: 10 * 1024, // 10 KB
			max_queries_per_session: 10,
			timeout_range: (1, 300),
			blocked_paths: vec!["/etc".to_string(), "/root".to_string(), "/sys".to_string()],
		}
	}

	/// Create a new query validator with custom settings.
	pub fn with_limits(
		max_query_size_bytes: usize,
		max_queries_per_session: u32,
		timeout_range: (u32, u32),
		blocked_paths: Vec<String>,
	) -> Self {
		Self {
			max_query_size_bytes,
			max_queries_per_session,
			timeout_range,
			blocked_paths,
		}
	}

	/// Validate query size.
	pub fn validate_size(&self, query: &str) -> Result<(), SecurityError> {
		let size = query.len();
		if size > self.max_query_size_bytes {
			warn!(
				"Query size validation failed: {} > {}",
				size, self.max_query_size_bytes
			);
			return Err(SecurityError::QueryTooLarge(
				size,
				self.max_query_size_bytes,
			));
		}
		debug!(
			size = size,
			max = self.max_query_size_bytes,
			"Query size validation passed"
		);
		Ok(())
	}

	/// Validate timeout value.
	pub fn validate_timeout(&self, timeout_seconds: u32) -> Result<(), SecurityError> {
		let (min, max) = self.timeout_range;
		if timeout_seconds < min || timeout_seconds > max {
			warn!(
				"Timeout validation failed: {} not in range [{}, {}]",
				timeout_seconds, min, max
			);
			return Err(SecurityError::InvalidTimeout(timeout_seconds, min, max));
		}
		debug!(timeout = timeout_seconds, "Timeout validation passed");
		Ok(())
	}

	/// Validate path doesn't escape workspace.
	pub fn validate_path(&self, path: &str) -> Result<(), SecurityError> {
		// Check for blocked paths
		for blocked in &self.blocked_paths {
			if path.starts_with(blocked) {
				warn!(path = path, blocked = blocked, "Blocked path detected");
				return Err(SecurityError::BlockedPath(path.to_string()));
			}
		}

		// Check for path escape attempts
		if path.contains("..") {
			warn!(path = path, "Path escape attempt detected");
			return Err(SecurityError::PathEscapeAttempt(path.to_string()));
		}

		debug!(path = path, "Path validation passed");
		Ok(())
	}

	/// Validate JSON payload.
	pub fn validate_json(&self, payload: &str) -> Result<Value, SecurityError> {
		serde_json::from_str(payload).map_err(|e| {
			warn!(error = %e, "JSON validation failed");
			SecurityError::InvalidJson(e.to_string())
		})
	}

	/// Get maximum queries per session.
	pub fn max_queries_per_session(&self) -> u32 {
		self.max_queries_per_session
	}
}

impl Default for QueryValidator {
	fn default() -> Self {
		Self::new()
	}
}

/// Path sanitizer that resolves and validates paths.
#[derive(Debug, Clone)]
pub struct PathSanitizer {
	workspace_root: PathBuf,
}

impl PathSanitizer {
	/// Create a new path sanitizer with a workspace root.
	pub fn new<P: AsRef<Path>>(workspace_root: P) -> Self {
		Self {
			workspace_root: workspace_root.as_ref().to_path_buf(),
		}
	}

	/// Sanitize and validate a path.
	///
	/// Returns an error if:
	/// - Path contains null bytes
	/// - Path is absolute
	/// - Path escapes the workspace root
	pub fn sanitize(&self, path: &str) -> Result<PathBuf, SecurityError> {
		// Check for null bytes
		if path.contains('\0') {
			warn!(path = path, "Path contains null bytes");
			return Err(SecurityError::InvalidPath(path.to_string()));
		}

		// Check for absolute paths
		let path_buf = PathBuf::from(path);
		if path_buf.is_absolute() {
			warn!(path = path, "Absolute path rejected");
			return Err(SecurityError::InvalidPath(path.to_string()));
		}

		// Normalize and join with workspace root
		let joined = self.workspace_root.join(&path_buf);

		// Resolve the canonical path
		let canonical = joined.canonicalize().map_err(|_| {
			warn!(path = path, "Path canonicalization failed");
			SecurityError::InvalidPath(path.to_string())
		})?;

		// Verify it's within workspace root
		if !canonical.starts_with(&self.workspace_root) {
			warn!(
					path = path,
					workspace = ?self.workspace_root,
					canonical = ?canonical,
					"Path escape attempt detected"
			);
			return Err(SecurityError::PathEscapeAttempt(path.to_string()));
		}

		debug!(path = path, canonical = ?canonical, "Path sanitization passed");
		Ok(canonical)
	}

	/// Get workspace root.
	pub fn workspace_root(&self) -> &Path {
		&self.workspace_root
	}
}

/// Token bucket rate limiter for per-session query rate limiting.
#[derive(Debug)]
pub struct RateLimiter {
	buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
	refill_rate: f64, // tokens per second
	max_tokens: u32,  // max tokens in bucket
}

#[derive(Debug, Clone)]
struct TokenBucket {
	tokens: f64,
	last_refill: SystemTime,
}

impl RateLimiter {
	/// Create a new rate limiter with default settings (100 queries/minute).
	pub fn new() -> Self {
		// 100 queries per minute = 100/60 ≈ 1.67 tokens/second
		Self::with_rate(100.0 / 60.0, 100)
	}

	/// Create a new rate limiter with custom rate.
	///
	/// # Arguments
	/// * `refill_rate` - tokens per second
	/// * `max_tokens` - maximum tokens in bucket
	pub fn with_rate(refill_rate: f64, max_tokens: u32) -> Self {
		Self {
			buckets: Arc::new(RwLock::new(HashMap::new())),
			refill_rate,
			max_tokens,
		}
	}

	/// Check if a session can make another query.
	///
	/// Returns Ok(()) if allowed, Err if rate limited.
	pub async fn check_rate_limit(&self, session_id: &str) -> Result<(), SecurityError> {
		let mut buckets = self.buckets.write().await;

		let now = SystemTime::now();
		let bucket = buckets
			.entry(session_id.to_string())
			.or_insert_with(|| TokenBucket {
				tokens: self.max_tokens as f64,
				last_refill: now,
			});

		// Refill tokens based on elapsed time
		if let Ok(elapsed) = now.duration_since(bucket.last_refill) {
			let refill_amount = elapsed.as_secs_f64() * self.refill_rate;
			bucket.tokens = (bucket.tokens + refill_amount).min(self.max_tokens as f64);
			bucket.last_refill = now;
		}

		// Check if token available
		if bucket.tokens >= 1.0 {
			bucket.tokens -= 1.0;
			debug!(
					session_id = session_id,
					remaining = ?bucket.tokens,
					"Rate limit check passed"
			);
			Ok(())
		} else {
			warn!(session_id = session_id, "Rate limit exceeded");
			Err(SecurityError::RateLimitExceeded)
		}
	}

	/// Get remaining tokens for a session.
	pub async fn get_remaining_tokens(&self, session_id: &str) -> u32 {
		let buckets = self.buckets.read().await;
		buckets
			.get(session_id)
			.map(|b| b.tokens.floor() as u32)
			.unwrap_or(self.max_tokens)
	}

	/// Reset rate limit for a session.
	pub async fn reset(&self, session_id: &str) {
		let mut buckets = self.buckets.write().await;
		buckets.remove(session_id);
		info!(session_id = session_id, "Rate limit reset");
	}
}

impl Default for RateLimiter {
	fn default() -> Self {
		Self::new()
	}
}

/// Result validator for response validation and sanitization.
#[derive(Debug, Clone)]
pub struct ResultValidator {
	max_result_size_bytes: usize,
}

impl ResultValidator {
	/// Create a new result validator with default settings (100 MB).
	pub fn new() -> Self {
		Self {
			max_result_size_bytes: 100 * 1024 * 1024,
		}
	}

	/// Create a new result validator with custom maximum size.
	pub fn with_max_size(max_size_bytes: usize) -> Self {
		Self {
			max_result_size_bytes: max_size_bytes,
		}
	}

	/// Validate result structure and size.
	pub fn validate(&self, result: &Value) -> Result<(), SecurityError> {
		// Check if it's valid JSON
		let json_str = serde_json::to_string(result).map_err(|e| {
			warn!(error = %e, "Result JSON serialization failed");
			SecurityError::MalformedResult(e.to_string())
		})?;

		// Check size
		let size = json_str.len();
		if size > self.max_result_size_bytes {
			warn!(
				size = size,
				max = self.max_result_size_bytes,
				"Result size validation failed"
			);
			return Err(SecurityError::ResultTooLarge(
				size,
				self.max_result_size_bytes,
			));
		}

		// Verify structure
		if !result.is_object() && !result.is_array() && !result.is_null() {
			warn!("Result is not a valid JSON object or array");
			return Err(SecurityError::MalformedResult(
				"Result must be a JSON object or array".to_string(),
			));
		}

		debug!(size = size, "Result validation passed");
		Ok(())
	}

	/// Sanitize error message to remove sensitive information.
	pub fn sanitize_error(&self, error_msg: &str) -> String {
		// Remove file paths and stack traces
		let sanitized = error_msg
			.lines()
			.filter(|line| {
				// Filter out common sensitive patterns
				!line.contains("at ") && !line.contains("stack") && !line.contains("/home/")
			})
			.collect::<Vec<_>>()
			.join("\n");

		if sanitized.is_empty() {
			"An error occurred".to_string()
		} else {
			sanitized
		}
	}
}

impl Default for ResultValidator {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use serde_json::json;

	#[test]
	fn test_query_size_validation_passes_within_limit() {
		let validator = QueryValidator::new();
		let query = "a".repeat(1000); // 1 KB, well under 10 KB limit
		assert!(validator.validate_size(&query).is_ok());
	}

	#[test]
	fn test_query_size_validation_fails_exceeds_limit() {
		let validator = QueryValidator::new();
		let query = "a".repeat(11 * 1024); // 11 KB, exceeds 10 KB limit
		assert!(validator.validate_size(&query).is_err());
	}

	#[test]
	fn test_timeout_validation_passes_within_range() {
		let validator = QueryValidator::new();
		assert!(validator.validate_timeout(1).is_ok());
		assert!(validator.validate_timeout(150).is_ok());
		assert!(validator.validate_timeout(300).is_ok());
	}

	#[test]
	fn test_timeout_validation_fails_outside_range() {
		let validator = QueryValidator::new();
		assert!(validator.validate_timeout(0).is_err());
		assert!(validator.validate_timeout(301).is_err());
	}

	#[test]
	fn test_path_escape_rejected() {
		let validator = QueryValidator::new();
		assert!(validator.validate_path("../etc/passwd").is_err());
		assert!(validator
			.validate_path("./data/../../../etc/passwd")
			.is_err());
	}

	#[test]
	fn test_blocked_paths_rejected() {
		let validator = QueryValidator::new();
		assert!(validator.validate_path("/etc/passwd").is_err());
		assert!(validator.validate_path("/root/.ssh/id_rsa").is_err());
		assert!(validator.validate_path("/sys/kernel/config").is_err());
	}

	#[test]
	fn test_valid_paths_accepted() {
		let validator = QueryValidator::new();
		assert!(validator.validate_path("data/file.txt").is_ok());
		assert!(validator.validate_path("./workspace/queries").is_ok());
	}

	#[test]
	fn test_json_validation_valid() {
		let validator = QueryValidator::new();
		let result = validator.validate_json(r#"{"key": "value"}"#);
		assert!(result.is_ok());
	}

	#[test]
	fn test_json_validation_invalid() {
		let validator = QueryValidator::new();
		let result = validator.validate_json(r#"{"key": invalid}"#);
		assert!(result.is_err());
	}

	#[test]
	fn test_path_sanitizer_absolute_path_rejected() {
		let sanitizer = PathSanitizer::new("/workspace");
		assert!(sanitizer.sanitize("/etc/passwd").is_err());
	}

	#[test]
	fn test_path_sanitizer_null_bytes_rejected() {
		let sanitizer = PathSanitizer::new("/workspace");
		assert!(sanitizer.sanitize("data\0file.txt").is_err());
	}

	#[tokio::test]
	async fn test_rate_limiter_allows_within_rate() {
		let limiter = RateLimiter::with_rate(10.0, 10); // 10 tokens max
		let session = "test_session";

		// Should allow up to 10 requests
		for _ in 0..10 {
			assert!(limiter.check_rate_limit(session).await.is_ok());
		}
	}

	#[tokio::test]
	async fn test_rate_limiter_blocks_exceeding_rate() {
		let limiter = RateLimiter::with_rate(1.0, 2); // 2 tokens max
		let session = "test_session";

		// Use both tokens
		assert!(limiter.check_rate_limit(session).await.is_ok());
		assert!(limiter.check_rate_limit(session).await.is_ok());

		// Should be rate limited
		assert!(limiter.check_rate_limit(session).await.is_err());
	}

	#[tokio::test]
	async fn test_rate_limiter_reset() {
		let limiter = RateLimiter::with_rate(1.0, 2);
		let session = "test_session";

		// Use both tokens
		limiter.check_rate_limit(session).await.ok();
		limiter.check_rate_limit(session).await.ok();
		assert!(limiter.check_rate_limit(session).await.is_err());

		// Reset and try again
		limiter.reset(session).await;
		assert!(limiter.check_rate_limit(session).await.is_ok());
	}

	#[test]
	fn test_result_validator_accepts_valid_json() {
		let validator = ResultValidator::new();
		let result = json!({"data": "value"});
		assert!(validator.validate(&result).is_ok());
	}

	#[test]
	fn test_result_validator_rejects_oversized_result() {
		let validator = ResultValidator::with_max_size(100);
		let result = json!({"data": "a".repeat(1000)});
		assert!(validator.validate(&result).is_err());
	}

	#[test]
	fn test_result_sanitizer_removes_file_paths() {
		let validator = ResultValidator::new();
		let error = "Error at /home/user/project/src/main.rs:42";
		let sanitized = validator.sanitize_error(error);
		assert!(!sanitized.contains("/home/user"));
	}

	#[test]
	fn test_result_sanitizer_removes_stack_traces() {
		let validator = ResultValidator::new();
		let error = "thread 'main' panicked at 'something'\nstack backtrace:";
		let sanitized = validator.sanitize_error(error);
		assert!(!sanitized.contains("stack"));
	}

	// Property-based tests

	// Property test: Any query smaller than the limit should pass validation.
	// This is important to ensure queries of reasonable size are always allowed.
	proptest! {
			#[test]
			fn prop_small_queries_always_pass(
					query_size in 0usize..5000  // Generate sizes from 0 to 5KB
			) {
					let validator = QueryValidator::new();
					let query = "x".repeat(query_size);
					prop_assert!(validator.validate_size(&query).is_ok());
			}
	}

	// Property test: Queries with .. should always be rejected.
	// This is important for preventing directory traversal attacks.
	proptest! {
			#[test]
			fn prop_paths_with_traversal_always_rejected(
					suffix in r"[a-zA-Z0-9_]*"
			) {
					let validator = QueryValidator::new();
					let path = format!("..{suffix}");
					prop_assert!(validator.validate_path(&path).is_err());
			}
	}

	// Property test: Any timeout outside 1-300s range should be rejected.
	// This is important to ensure timeout bounds are enforced consistently.
	proptest! {
			#[test]
			fn prop_invalid_timeouts_always_rejected(
					timeout in (u32::MAX - 1000u32)..=u32::MAX
			) {
					let validator = QueryValidator::new();
					prop_assert!(validator.validate_timeout(timeout).is_err());
			}
	}
}
