// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Provisioner error types.

/// Errors that can occur during weaver provisioning operations.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionerError {
	/// Weaver not found
	#[error("Weaver not found: {id}")]
	WeaverNotFound { id: String },

	/// Too many concurrent weavers
	#[error("Too many weavers: {current} running (max: {max})")]
	TooManyWeavers { current: u32, max: u32 },

	/// Requested lifetime exceeds maximum
	#[error("Invalid lifetime: {requested} hours (max: {max} hours)")]
	InvalidLifetime { requested: u32, max: u32 },

	/// Weaver failed to start
	#[error("Weaver failed: {id} - {reason}")]
	WeaverFailed { id: String, reason: String },

	/// Weaver timed out waiting for ready state
	#[error("Weaver timed out waiting for ready state: {id}")]
	WeaverTimeout { id: String },

	/// Weaver is not in Running state
	#[error("Weaver not running: {id} (status: {status})")]
	WeaverNotRunning { id: String, status: String },

	/// Kubernetes error
	#[error(transparent)]
	K8sError(#[from] loom_server_k8s::K8sError),

	/// Namespace not found
	#[error("Namespace not found: {name}")]
	NamespaceNotFound { name: String },
}
