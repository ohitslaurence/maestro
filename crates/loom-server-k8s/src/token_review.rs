// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! K8s TokenReview API support for validating service account JWTs.
//!
//! This module provides types and implementations for validating K8s service
//! account tokens using the TokenReview API. Weavers present their K8s SA JWT
//! and the server validates it to establish identity.

use std::collections::HashMap;

/// Result of a TokenReview validation request.
#[derive(Debug, Clone)]
pub struct TokenReviewResult {
	/// Whether the token was authenticated successfully.
	pub authenticated: bool,
	/// The username from the token (e.g., "system:serviceaccount:loom-weavers:default").
	pub username: Option<String>,
	/// Groups the user belongs to.
	pub groups: Vec<String>,
	/// Extra information from the token.
	/// May contain pod name, namespace, UID, etc.
	pub extra: HashMap<String, Vec<String>>,
	/// Audiences that the token is valid for.
	pub audiences: Vec<String>,
	/// Error message if authentication failed.
	pub error: Option<String>,
}

impl TokenReviewResult {
	/// Create a successful authentication result.
	pub fn authenticated(
		username: String,
		groups: Vec<String>,
		extra: HashMap<String, Vec<String>>,
		audiences: Vec<String>,
	) -> Self {
		Self {
			authenticated: true,
			username: Some(username),
			groups,
			extra,
			audiences,
			error: None,
		}
	}

	/// Create a failed authentication result.
	pub fn unauthenticated(error: Option<String>) -> Self {
		Self {
			authenticated: false,
			username: None,
			groups: Vec::new(),
			extra: HashMap::new(),
			audiences: Vec::new(),
			error,
		}
	}

	/// Get the pod name from the extra claims, if present.
	pub fn pod_name(&self) -> Option<&str> {
		self
			.extra
			.get("authentication.kubernetes.io/pod-name")
			.and_then(|v| v.first())
			.map(|s| s.as_str())
	}

	/// Get the pod UID from the extra claims, if present.
	pub fn pod_uid(&self) -> Option<&str> {
		self
			.extra
			.get("authentication.kubernetes.io/pod-uid")
			.and_then(|v| v.first())
			.map(|s| s.as_str())
	}

	/// Get the namespace from the extra claims or parse from username.
	pub fn namespace(&self) -> Option<&str> {
		if let Some(ns) = self
			.extra
			.get("authentication.kubernetes.io/namespace")
			.and_then(|v| v.first())
		{
			return Some(ns.as_str());
		}
		self.parse_namespace_from_username()
	}

	/// Parse the namespace from username format: "system:serviceaccount:{namespace}:{name}"
	fn parse_namespace_from_username(&self) -> Option<&str> {
		let username = self.username.as_ref()?;
		let parts: Vec<&str> = username.split(':').collect();
		if parts.len() >= 3 && parts[0] == "system" && parts[1] == "serviceaccount" {
			Some(parts[2])
		} else {
			None
		}
	}

	/// Parse the service account name from username.
	pub fn service_account_name(&self) -> Option<&str> {
		let username = self.username.as_ref()?;
		let parts: Vec<&str> = username.split(':').collect();
		if parts.len() >= 4 && parts[0] == "system" && parts[1] == "serviceaccount" {
			Some(parts[3])
		} else {
			None
		}
	}

	/// Check if the token is for a service account.
	pub fn is_service_account(&self) -> bool {
		self
			.username
			.as_ref()
			.is_some_and(|u| u.starts_with("system:serviceaccount:"))
	}
}

/// A mock K8s client that can be used for testing TokenReview validation.
///
/// This mock allows configuring predetermined responses for token validation
/// without requiring a real K8s cluster.
#[derive(Debug, Clone, Default)]
pub struct MockTokenReviewer {
	responses: std::sync::Arc<std::sync::Mutex<Vec<TokenReviewResult>>>,
}

impl MockTokenReviewer {
	/// Create a new mock token reviewer.
	pub fn new() -> Self {
		Self::default()
	}

	/// Add a response to be returned by the next call to `validate_token`.
	/// Responses are returned in FIFO order.
	pub fn add_response(&self, result: TokenReviewResult) {
		self.responses.lock().unwrap().push(result);
	}

	/// Validate a token by returning the next configured response.
	/// If no responses are configured, returns an unauthenticated result.
	pub fn validate_token(&self, _token: &str, _audiences: &[&str]) -> TokenReviewResult {
		let mut responses = self.responses.lock().unwrap();
		if responses.is_empty() {
			TokenReviewResult::unauthenticated(Some("No mock response configured".to_string()))
		} else {
			responses.remove(0)
		}
	}

	/// Create a mock authenticated response for a weaver service account.
	pub fn weaver_response(
		namespace: &str,
		service_account: &str,
		pod_name: &str,
		pod_uid: &str,
	) -> TokenReviewResult {
		let mut extra = HashMap::new();
		extra.insert(
			"authentication.kubernetes.io/pod-name".to_string(),
			vec![pod_name.to_string()],
		);
		extra.insert(
			"authentication.kubernetes.io/pod-uid".to_string(),
			vec![pod_uid.to_string()],
		);
		extra.insert(
			"authentication.kubernetes.io/namespace".to_string(),
			vec![namespace.to_string()],
		);

		TokenReviewResult::authenticated(
			format!("system:serviceaccount:{namespace}:{service_account}"),
			vec![
				"system:serviceaccounts".to_string(),
				format!("system:serviceaccounts:{namespace}"),
				"system:authenticated".to_string(),
			],
			extra,
			vec!["https://kubernetes.default.svc".to_string()],
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn authenticated_result_has_correct_fields() {
		let mut extra = HashMap::new();
		extra.insert(
			"authentication.kubernetes.io/pod-name".to_string(),
			vec!["test-pod".to_string()],
		);
		extra.insert(
			"authentication.kubernetes.io/pod-uid".to_string(),
			vec!["abc-123".to_string()],
		);

		let result = TokenReviewResult::authenticated(
			"system:serviceaccount:loom-weavers:default".to_string(),
			vec![
				"system:serviceaccounts".to_string(),
				"system:serviceaccounts:loom-weavers".to_string(),
			],
			extra,
			vec!["https://kubernetes.default.svc".to_string()],
		);

		assert!(result.authenticated);
		assert_eq!(
			result.username,
			Some("system:serviceaccount:loom-weavers:default".to_string())
		);
		assert_eq!(result.groups.len(), 2);
		assert_eq!(result.pod_name(), Some("test-pod"));
		assert_eq!(result.pod_uid(), Some("abc-123"));
		assert_eq!(result.namespace(), Some("loom-weavers"));
		assert_eq!(result.service_account_name(), Some("default"));
		assert!(result.is_service_account());
		assert!(result.error.is_none());
	}

	#[test]
	fn unauthenticated_result_has_correct_fields() {
		let result = TokenReviewResult::unauthenticated(Some("token expired".to_string()));

		assert!(!result.authenticated);
		assert!(result.username.is_none());
		assert!(result.groups.is_empty());
		assert!(result.extra.is_empty());
		assert!(result.audiences.is_empty());
		assert_eq!(result.error, Some("token expired".to_string()));
		assert!(result.pod_name().is_none());
		assert!(result.namespace().is_none());
		assert!(!result.is_service_account());
	}

	#[test]
	fn parse_namespace_from_username() {
		let result = TokenReviewResult::authenticated(
			"system:serviceaccount:custom-namespace:my-sa".to_string(),
			Vec::new(),
			HashMap::new(),
			Vec::new(),
		);

		assert_eq!(result.namespace(), Some("custom-namespace"));
		assert_eq!(result.service_account_name(), Some("my-sa"));
	}

	#[test]
	fn non_service_account_username() {
		let result =
			TokenReviewResult::authenticated("admin".to_string(), Vec::new(), HashMap::new(), Vec::new());

		assert!(!result.is_service_account());
		assert!(result.namespace().is_none());
		assert!(result.service_account_name().is_none());
	}

	#[test]
	fn namespace_from_extra_takes_precedence() {
		let mut extra = HashMap::new();
		extra.insert(
			"authentication.kubernetes.io/namespace".to_string(),
			vec!["extra-namespace".to_string()],
		);

		let result = TokenReviewResult::authenticated(
			"system:serviceaccount:username-namespace:sa".to_string(),
			Vec::new(),
			extra,
			Vec::new(),
		);

		assert_eq!(result.namespace(), Some("extra-namespace"));
	}

	#[test]
	fn mock_token_reviewer_returns_configured_responses() {
		let mock = MockTokenReviewer::new();

		let response1 =
			TokenReviewResult::authenticated("user1".to_string(), Vec::new(), HashMap::new(), Vec::new());
		let response2 = TokenReviewResult::unauthenticated(Some("expired".to_string()));

		mock.add_response(response1);
		mock.add_response(response2);

		let result1 = mock.validate_token("token1", &[]);
		assert!(result1.authenticated);
		assert_eq!(result1.username, Some("user1".to_string()));

		let result2 = mock.validate_token("token2", &[]);
		assert!(!result2.authenticated);
		assert_eq!(result2.error, Some("expired".to_string()));
	}

	#[test]
	fn mock_token_reviewer_returns_unauthenticated_when_empty() {
		let mock = MockTokenReviewer::new();
		let result = mock.validate_token("any-token", &["audience"]);

		assert!(!result.authenticated);
		assert!(result.error.is_some());
	}

	#[test]
	fn mock_weaver_response_creates_valid_result() {
		let result = MockTokenReviewer::weaver_response(
			"loom-weavers",
			"weaver-sa",
			"weaver-pod-123",
			"pod-uid-456",
		);

		assert!(result.authenticated);
		assert_eq!(
			result.username,
			Some("system:serviceaccount:loom-weavers:weaver-sa".to_string())
		);
		assert_eq!(result.namespace(), Some("loom-weavers"));
		assert_eq!(result.pod_name(), Some("weaver-pod-123"));
		assert_eq!(result.pod_uid(), Some("pod-uid-456"));
		assert_eq!(result.service_account_name(), Some("weaver-sa"));
		assert!(result.is_service_account());
		assert!(result
			.groups
			.contains(&"system:serviceaccounts".to_string()));
		assert!(result
			.groups
			.contains(&"system:serviceaccounts:loom-weavers".to_string()));
	}
}
