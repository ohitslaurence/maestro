// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for ACP integration.

use thiserror::Error;

/// Errors that can occur during ACP operations.
#[derive(Debug, Error)]
pub enum AcpError {
	#[error("session not found: {0}")]
	SessionNotFound(String),

	#[error("invalid session ID: {0}")]
	InvalidSessionId(String),

	#[error("LLM error: {0}")]
	Llm(#[from] loom_common_core::LlmError),

	#[error("tool error: {0}")]
	Tool(#[from] loom_common_core::ToolError),

	#[error("thread store error: {0}")]
	ThreadStore(#[from] loom_common_thread::ThreadStoreError),

	#[error("notification channel closed")]
	NotificationChannelClosed,

	#[error("cancelled")]
	Cancelled,

	#[error("internal error: {0}")]
	Internal(String),
}

impl From<AcpError> for agent_client_protocol::Error {
	fn from(err: AcpError) -> Self {
		match err {
			AcpError::SessionNotFound(_) => agent_client_protocol::Error::invalid_params(),
			AcpError::InvalidSessionId(_) => agent_client_protocol::Error::invalid_params(),
			AcpError::Cancelled => agent_client_protocol::Error::internal_error(),
			_ => agent_client_protocol::Error::internal_error(),
		}
	}
}
