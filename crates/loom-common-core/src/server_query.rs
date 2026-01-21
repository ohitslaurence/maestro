// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Server-to-Client Query Bridge
//!
//! This module defines types for bi-directional communication where the server
//! can send queries to clients and receive structured responses.
//!
//! ## Purpose
//! Enables the server to request information or actions from the client during
//! a turn, including file reads, command execution, user input, and environment
//! queries.
//!
//! ## Key Types
//! - `ServerQuery`: A request sent from server to client
//! - `ServerQueryKind`: Discriminator enum for query types
//! - `ServerQueryResponse`: Response from client back to server
//! - `ServerQueryResult`: Typed result data from the client
//! - `ServerQueryError`: Error conditions during query processing

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// A query sent from server to client
///
/// Each query has a unique ID for correlation, a kind (discriminator for what
/// action is being requested), timeout constraints, and optional metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerQuery {
	/// Unique query ID for correlation (format: "Q-{32 hex digits}")
	pub id: String,

	/// Query type discriminator
	pub kind: ServerQueryKind,

	/// Timestamp when query was sent (RFC3339 format)
	pub sent_at: String,

	/// Timeout in seconds; client should abandon after this duration
	pub timeout_secs: u32,

	/// Optional metadata for debugging and context
	pub metadata: serde_json::Value,
}

/// Discriminator enum for different query types
///
/// Each variant represents a different kind of query the server might send
/// to the client, with variant-specific payload data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerQueryKind {
	/// Server requests client to read a file from filesystem
	ReadFile { path: String },

	/// Server requests client to execute a local command
	ExecuteCommand {
		command: String,
		args: Vec<String>,
		timeout_secs: u32,
	},

	/// Server requests human input (pause & ask user)
	RequestUserInput {
		prompt: String,
		input_type: String,           // "text", "yes_no", "selection"
		options: Option<Vec<String>>, // For selection
	},

	/// Server requests client environment information
	GetEnvironment { keys: Vec<String> },

	/// Server requests workspace context
	GetWorkspaceContext,

	/// Extensible: custom queries
	Custom {
		name: String,
		payload: serde_json::Value,
	},
}

/// Response from client back to server
///
/// Correlates to a ServerQuery via query_id and contains the result
/// or error information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerQueryResponse {
	/// Correlate back to ServerQuery::id
	pub query_id: String,

	/// Timestamp when response was sent (RFC3339 format)
	pub sent_at: String,

	/// Result data (type depends on query kind)
	pub result: ServerQueryResult,

	/// Optional error message if query processing failed
	pub error: Option<String>,
}

/// Typed result data from a server query
///
/// The variant type depends on which ServerQueryKind was used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerQueryResult {
	/// Result from ReadFile query
	FileContent(String),

	/// Result from ExecuteCommand query
	CommandOutput {
		exit_code: i32,
		stdout: String,
		stderr: String,
	},

	/// Result from RequestUserInput query
	UserInput(String),

	/// Result from GetEnvironment query
	Environment(HashMap<String, String>),

	/// Result from GetWorkspaceContext query
	WorkspaceContext(serde_json::Value),

	/// Result from Custom query
	Custom {
		name: String,
		payload: serde_json::Value,
	},
}

/// Error type for server query operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerQueryError {
	/// Query timed out waiting for response
	#[error("server query timeout")]
	Timeout,

	/// No response received for query
	#[error("no response received for server query")]
	NoResponse,

	/// Query was cancelled
	#[error("server query was cancelled")]
	Cancelled,

	/// Invalid query format or data
	#[error("invalid server query: {0}")]
	InvalidQuery(String),

	/// Query processing failed on client
	#[error("server query processing failed: {0}")]
	ProcessingFailed(String),

	/// Resource not found
	#[error("server query resource not found: {0}")]
	NotFound(String),

	/// Execution failed on client
	#[error("server query execution failed: {0}")]
	ExecutionFailed(String),

	/// Generic error
	#[error("server query error: {0}")]
	Other(String),
}

/// Handler trait for processing server queries on the client side.
///
/// Implementations should handle different query types and return appropriate
/// responses. This trait is implemented by agents that run on clients to
/// enable server-to-client communication.
#[async_trait]
pub trait ServerQueryHandler: Send + Sync {
	/// Process a server query and return a response.
	///
	/// # Arguments
	/// * `query` - The server query to process
	///
	/// # Returns
	/// A ServerQueryResponse containing the result or error information
	async fn handle_query(&self, query: ServerQuery)
		-> Result<ServerQueryResponse, ServerQueryError>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	/// Helper to generate valid file paths
	fn arb_path() -> impl Strategy<Value = String> {
		r"[a-zA-Z0-9_\-/\.]{1,50}".prop_map(|s| format!("/{s}"))
	}

	/// Helper to generate valid timeout values (1-300 seconds)
	fn arb_timeout_secs() -> impl Strategy<Value = u32> {
		1u32..=300u32
	}

	/// Helper to generate arbitrary ServerQuery
	fn arb_server_query() -> impl Strategy<Value = ServerQuery> {
		(
			0u128..,
			arb_path(),
			"2025-01-01T00:00:00Z",
			arb_timeout_secs(),
		)
			.prop_map(|(id_num, path, sent_at, timeout_secs)| ServerQuery {
				id: format!("Q-{id_num:032x}"),
				kind: ServerQueryKind::ReadFile { path },
				sent_at: sent_at.to_string(),
				timeout_secs,
				metadata: serde_json::json!({}),
			})
	}

	/// Helper to generate arbitrary ServerQueryResponse
	fn arb_server_query_response() -> impl Strategy<Value = ServerQueryResponse> {
		(0u128.., r"[a-zA-Z0-9]{1,100}", "2025-01-01T00:00:00Z").prop_map(
			|(id_num, content, sent_at)| ServerQueryResponse {
				query_id: format!("Q-{id_num:032x}"),
				sent_at: sent_at.to_string(),
				result: ServerQueryResult::FileContent(content),
				error: None,
			},
		)
	}

	/// Helper to generate arbitrary ServerQueryError
	fn arb_server_query_error() -> impl Strategy<Value = ServerQueryError> {
		prop_oneof![
			Just(ServerQueryError::Timeout),
			Just(ServerQueryError::NoResponse),
			Just(ServerQueryError::Cancelled),
			r"[a-zA-Z0-9 ]{1,50}".prop_map(ServerQueryError::InvalidQuery),
			r"[a-zA-Z0-9 ]{1,50}".prop_map(ServerQueryError::ProcessingFailed),
			r"[a-zA-Z0-9 ]{1,50}".prop_map(ServerQueryError::Other),
		]
	}

	proptest! {
			/// **Purpose**: Ensure ServerQuery IDs are always in the correct format
			///
			/// **Why Important**: IDs are used for correlation between requests and responses.
			/// A malformed ID could break the query-response matching mechanism.
			#[test]
			fn server_query_id_always_valid(id_num in 0u128..) {
					let query = ServerQuery {
							id: format!("Q-{id_num:032x}"),
							kind: ServerQueryKind::ReadFile { path: "/test".to_string() },
							sent_at: "2025-01-01T00:00:00Z".to_string(),
							timeout_secs: 30,
							metadata: serde_json::json!({}),
					};
					prop_assert!(query.id.starts_with("Q-"));
					prop_assert_eq!(query.id.len(), 34); // "Q-" + 32 hex chars
			}

			/// **Purpose**: Verify ServerQuery roundtrips through JSON serialization
			///
			/// **Why Important**: Ensures data integrity when serializing/deserializing
			/// queries during network transmission.
			#[test]
			fn server_query_json_roundtrip(query in arb_server_query()) {
					let json = serde_json::to_string(&query).unwrap();
					let decoded: ServerQuery = serde_json::from_str(&json).unwrap();
					prop_assert_eq!(query, decoded);
			}

			/// **Purpose**: Verify ServerQueryResponse roundtrips through JSON serialization
			///
			/// **Why Important**: Ensures data integrity when responses travel from
			/// client back to server.
			#[test]
			fn server_query_response_json_roundtrip(resp in arb_server_query_response()) {
					let json = serde_json::to_string(&resp).unwrap();
					let decoded: ServerQueryResponse = serde_json::from_str(&json).unwrap();
					prop_assert_eq!(resp, decoded);
			}

			/// **Purpose**: Verify ServerQueryError Display and Debug never panic
			///
			/// **Why Important**: Error display is used in logging and error reporting.
			/// Panics in display impl would break error handling.
			#[test]
			fn error_display_never_panics(error in arb_server_query_error()) {
					let _ = format!("{error}");
					let _ = format!("{error:?}");
			}
	}

	#[test]
	fn server_query_id_format_validation() {
		let query = ServerQuery {
			id: "Q-0123456789abcdef0123456789abcdef".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "/file.txt".to_string(),
			},
			sent_at: "2025-01-01T00:00:00Z".to_string(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};
		assert!(query.id.starts_with("Q-"));
		assert_eq!(query.id.len(), 34);
	}

	#[test]
	fn server_query_response_error_handling() {
		let response = ServerQueryResponse {
			query_id: "Q-0123456789abcdef0123456789abcdef".to_string(),
			sent_at: "2025-01-01T00:00:01Z".to_string(),
			result: ServerQueryResult::FileContent("content".to_string()),
			error: Some("file not found".to_string()),
		};
		assert_eq!(response.error, Some("file not found".to_string()));
	}

	#[test]
	fn server_query_kind_serialization() {
		let kind = ServerQueryKind::ExecuteCommand {
			command: "ls".to_string(),
			args: vec!["-la".to_string()],
			timeout_secs: 10,
		};
		let json = serde_json::to_string(&kind).unwrap();
		let decoded: ServerQueryKind = serde_json::from_str(&json).unwrap();
		assert_eq!(kind, decoded);
	}
}
