// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;

use crate::error::K8sError;
use crate::token_review::TokenReviewResult;
use crate::types::{AttachedProcess, LogOptions, LogStream, Namespace, Pod};

/// Trait for K8s client operations.
///
/// This abstraction allows for easy mocking in tests while providing
/// a clean interface for K8s operations needed by the agent provisioner.
#[async_trait]
pub trait K8sClient: Send + Sync {
	/// Create a new pod in the specified namespace.
	async fn create_pod(&self, namespace: &str, pod: Pod) -> Result<Pod, K8sError>;

	/// Delete a pod by name from the specified namespace.
	async fn delete_pod(
		&self,
		name: &str,
		namespace: &str,
		grace_period_seconds: u32,
	) -> Result<(), K8sError>;

	/// List pods in a namespace matching the given label selector.
	async fn list_pods(&self, namespace: &str, label_selector: &str) -> Result<Vec<Pod>, K8sError>;

	/// Get a specific pod by name from the specified namespace.
	async fn get_pod(&self, name: &str, namespace: &str) -> Result<Pod, K8sError>;

	/// Get a namespace by name.
	async fn get_namespace(&self, name: &str) -> Result<Namespace, K8sError>;

	/// Stream logs from a container in a pod.
	async fn stream_logs(
		&self,
		name: &str,
		namespace: &str,
		container: &str,
		opts: LogOptions,
	) -> Result<LogStream, K8sError>;

	/// Attach to a running container's stdin/stdout for interactive I/O.
	async fn exec_attach(
		&self,
		name: &str,
		namespace: &str,
		container: &str,
	) -> Result<AttachedProcess, K8sError>;

	/// Validate a K8s service account token using the TokenReview API.
	///
	/// This is used to authenticate weavers presenting their K8s SA JWT.
	/// The token is validated against the cluster's authentication system,
	/// and if valid, returns information about the authenticated identity.
	///
	/// # Arguments
	/// * `token` - The service account JWT to validate
	/// * `audiences` - Expected audiences for the token (e.g., ["https://kubernetes.default.svc"])
	///
	/// # Returns
	/// * `Ok(TokenReviewResult)` - The result of the token validation
	/// * `Err(K8sError)` - If the API call fails
	async fn validate_token(
		&self,
		token: &str,
		audiences: &[&str],
	) -> Result<TokenReviewResult, K8sError>;
}
