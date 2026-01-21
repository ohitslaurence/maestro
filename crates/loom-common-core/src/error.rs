// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use thiserror::Error;

/// Result type alias for agent operations.
pub type AgentResult<T> = Result<T, AgentError>;

/// Top-level error type for agent operations.
#[derive(Error, Debug)]
pub enum AgentError {
	#[error("LLM error: {0}")]
	Llm(#[from] LlmError),

	#[error("Tool error: {0}")]
	Tool(#[from] ToolError),

	#[error("Invalid state: {0}")]
	InvalidState(String),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Operation timed out: {0}")]
	Timeout(String),

	#[error("Internal error: {0}")]
	Internal(String),
}

/// Errors that can occur during LLM interactions.
#[derive(Clone, Error, Debug)]
pub enum LlmError {
	#[error("HTTP error: {0}")]
	Http(String),

	#[error("API error: {0}")]
	Api(String),

	#[error("Request timed out")]
	Timeout,

	#[error("Invalid response: {0}")]
	InvalidResponse(String),

	#[error("Rate limited: retry after {retry_after_secs:?} seconds")]
	RateLimited { retry_after_secs: Option<u64> },
}

/// Errors that can occur during tool execution.
#[derive(Clone, Error, Debug)]
pub enum ToolError {
	#[error("Tool not found: {0}")]
	NotFound(String),

	#[error("Invalid arguments: {0}")]
	InvalidArguments(String),

	#[error("IO error: {0}")]
	Io(String),

	#[error("Tool execution timed out")]
	Timeout,

	#[error("Internal error: {0}")]
	Internal(String),

	#[error("Target not found: {0}")]
	TargetNotFound(String),

	#[error("Path outside workspace: {0}")]
	PathOutsideWorkspace(std::path::PathBuf),

	#[error("File not found: {0}")]
	FileNotFound(std::path::PathBuf),

	#[error("Serialization error: {0}")]
	Serialization(String),
}

impl From<std::io::Error> for ToolError {
	fn from(err: std::io::Error) -> Self {
		ToolError::Io(err.to_string())
	}
}
