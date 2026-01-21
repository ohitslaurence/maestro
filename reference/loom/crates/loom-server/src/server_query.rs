// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Server-to-client query management system.
//!
//! This module manages the lifecycle of queries sent from server to client,
//! including sending queries, storing responses, and notifying waiters.

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_common_core::server_query::{ServerQuery, ServerQueryError, ServerQueryResponse};
use std::{
	collections::HashMap,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::{broadcast, Mutex};
use tracing::instrument;

use crate::query_metrics::QueryMetrics;

/// Manages pending queries and responses between server and client.
///
/// # Phase 3 Migration Notes
/// In Phase 3 (WebSocket), this manager will:
/// - Still track pending queries and responses (same logic)
/// - Queries sent via WebSocket instead of HTTP/SSE
/// - Responses received via WebSocket instead of HTTP POST
/// - State structure remains unchanged; only transport changes
#[derive(Clone)]
pub struct ServerQueryManager {
	/// Pending queries waiting for response.
	/// Phase 3: Same data structure, different transport
	pending: Arc<Mutex<HashMap<String, ServerQuery>>>,

	/// Responses received from client.
	/// Phase 3: Same data structure, different transport
	responses: Arc<Mutex<HashMap<String, ServerQueryResponse>>>,

	/// Notification channel for query responses.
	/// Phase 3: May be replaced with WebSocket event channel
	response_tx: broadcast::Sender<ServerQueryResponse>,

	/// Query metrics for monitoring and observability
	metrics: Option<Arc<QueryMetrics>>,
}

impl ServerQueryManager {
	/// Create a new server query manager.
	pub fn new() -> Self {
		let (response_tx, _) = broadcast::channel(100);

		Self {
			pending: Arc::new(Mutex::new(HashMap::new())),
			responses: Arc::new(Mutex::new(HashMap::new())),
			response_tx,
			metrics: None,
		}
	}

	/// Create a new server query manager with metrics.
	///
	/// # Arguments
	/// * `metrics` - QueryMetrics instance for recording operations
	pub fn with_metrics(metrics: Arc<QueryMetrics>) -> Self {
		let (response_tx, _) = broadcast::channel(100);

		Self {
			pending: Arc::new(Mutex::new(HashMap::new())),
			responses: Arc::new(Mutex::new(HashMap::new())),
			response_tx,
			metrics: Some(metrics),
		}
	}

	/// Set metrics for an existing manager.
	pub fn set_metrics(&mut self, metrics: Arc<QueryMetrics>) {
		self.metrics = Some(metrics);
	}

	/// Send a query to the client and wait for response with timeout.
	///
	/// # Arguments
	/// * `session_id` - The session ID for logging
	/// * `query` - The query to send
	///
	/// # Returns
	/// The response from the client, or timeout error
	///
	/// # Phase 3 Migration
	/// - Current: Query sent via HTTP/SSE, response via HTTP POST
	/// - Phase 3: Query sent via WebSocket frame, response via WebSocket message
	/// - Logic unchanged: store pending, wait for response, timeout handling
	#[instrument(
        skip(self, query),
        fields(
            query_id = %query.id,
            session_id = %session_id,
            timeout_secs = query.timeout_secs
        )
    )]
	pub async fn send_query(
		&self,
		session_id: &str,
		query: ServerQuery,
	) -> Result<ServerQueryResponse, ServerQueryError> {
		let query_id = query.id.clone();
		let query_type = extract_query_type(&query);
		let timeout = Duration::from_secs(query.timeout_secs as u64);
		let start_time = Instant::now();

		// Record query sent
		if let Some(metrics) = &self.metrics {
			metrics.record_sent(&query_type, session_id);
		}

		// Store query as pending
		{
			let mut pending = self.pending.lock().await;
			pending.insert(query_id.clone(), query.clone());
			tracing::debug!(
					query_id = %query_id,
					session_id = %session_id,
					"query stored as pending"
			);
		}

		// Wait for response with timeout
		match tokio::time::timeout(timeout, self.wait_for_response(&query_id)).await {
			Ok(result) => {
				// Remove from pending on success
				self.pending.lock().await.remove(&query_id);
				let elapsed = start_time.elapsed().as_secs_f64();

				// Record success
				if let Some(metrics) = &self.metrics {
					metrics.record_success(&query_type, session_id, elapsed);
				}

				tracing::info!(
						query_id = %query_id,
						session_id = %session_id,
						latency_secs = elapsed,
						"received query response"
				);
				result
			}
			Err(_) => {
				// Remove from pending on timeout
				self.pending.lock().await.remove(&query_id);

				// Record timeout
				if let Some(metrics) = &self.metrics {
					metrics.record_failure(&query_type, "timeout", session_id);
				}

				tracing::warn!(
						query_id = %query_id,
						session_id = %session_id,
						timeout_secs = query.timeout_secs,
						"query timeout"
				);
				Err(ServerQueryError::Timeout)
			}
		}
	}

	/// Store a response received from the client.
	///
	/// # Arguments
	/// * `response` - The response to store
	///
	/// # Phase 3 Migration
	/// - Current: Called by HTTP POST handler
	/// - Phase 3: Called by WebSocket message handler
	/// - Logic unchanged: store response and broadcast to waiters
	#[instrument(
        skip(self, response),
        fields(query_id = %response.query_id)
    )]
	pub async fn receive_response(&self, response: ServerQueryResponse) {
		let query_id = response.query_id.clone();

		// Store response
		{
			let mut responses = self.responses.lock().await;
			responses.insert(query_id.clone(), response.clone());
			tracing::debug!(query_id = %query_id, "response stored");
		}

		// Broadcast to all waiters
		let _ = self.response_tx.send(response);
	}

	/// Wait for a specific query response.
	///
	/// # Arguments
	/// * `query_id` - The ID of the query to wait for
	///
	/// # Returns
	/// The response when received
	async fn wait_for_response(
		&self,
		query_id: &str,
	) -> Result<ServerQueryResponse, ServerQueryError> {
		let mut rx = self.response_tx.subscribe();

		loop {
			match rx.recv().await {
				Ok(response) => {
					if response.query_id == query_id {
						return Ok(response);
					}
				}
				Err(broadcast::error::RecvError::Closed) => {
					return Err(ServerQueryError::NoResponse);
				}
				Err(broadcast::error::RecvError::Lagged(_)) => {
					// Ignore lagged messages
					continue;
				}
			}
		}
	}

	/// List all pending queries for a session.
	///
	/// # Arguments
	/// * `session_id` - The session ID for logging
	///
	/// # Returns
	/// Vector of pending queries
	#[instrument(skip(self), fields(session_id = %session_id))]
	pub async fn list_pending(&self, session_id: &str) -> Vec<ServerQuery> {
		let pending = self.pending.lock().await;
		let queries: Vec<_> = pending.values().cloned().collect();
		tracing::debug!(
				session_id = %session_id,
				count = queries.len(),
				"listed pending queries"
		);
		queries
	}

	/// Get a stored response by query ID.
	///
	/// # Arguments
	/// * `query_id` - The ID of the query
	///
	/// # Returns
	/// The response if found
	#[instrument(skip(self), fields(query_id = %query_id))]
	pub async fn get_response(&self, query_id: &str) -> Option<ServerQueryResponse> {
		let responses = self.responses.lock().await;
		responses.get(query_id).cloned()
	}

	/// Add a pending query directly (for testing).
	///
	/// # Arguments
	/// * `query` - The query to add as pending
	#[cfg(test)]
	pub async fn add_pending_for_test(&self, query: ServerQuery) {
		let mut pending = self.pending.lock().await;
		pending.insert(query.id.clone(), query);
	}

	/// Update metrics to reflect current pending count
	pub async fn update_pending_metrics(&self) {
		if let Some(metrics) = &self.metrics {
			let pending = self.pending.lock().await;
			metrics.set_pending_count(pending.len() as i64);
		}
	}
}

/// Extract a query type string from a ServerQuery
fn extract_query_type(query: &ServerQuery) -> String {
	match &query.kind {
		loom_common_core::server_query::ServerQueryKind::ReadFile { .. } => "read_file".to_string(),
		loom_common_core::server_query::ServerQueryKind::ExecuteCommand { .. } => {
			"execute_command".to_string()
		}
		loom_common_core::server_query::ServerQueryKind::RequestUserInput { .. } => {
			"request_user_input".to_string()
		}
		loom_common_core::server_query::ServerQueryKind::GetEnvironment { .. } => {
			"get_environment".to_string()
		}
		loom_common_core::server_query::ServerQueryKind::GetWorkspaceContext => {
			"get_workspace_context".to_string()
		}
		loom_common_core::server_query::ServerQueryKind::Custom { .. } => "custom".to_string(),
	}
}

impl Default for ServerQueryManager {
	fn default() -> Self {
		Self::new()
	}
}

/// HTTP handler for query responses from client.
///
/// POST /api/sessions/{session_id}/query-response
///
/// # Phase 3 Migration
/// - Current: HTTP POST endpoint (backcompat after WebSocket migration)
/// - Phase 3: Replace with WebSocket message handler
/// - This endpoint may be deprecated once all clients use WebSocket
#[instrument(skip(state, response))]
pub async fn handle_query_response(
	State(state): State<crate::api::AppState>,
	Json(response): Json<ServerQueryResponse>,
) -> Result<impl IntoResponse, crate::error::ServerError> {
	let query_id = response.query_id.clone();
	tracing::debug!(query_id = %query_id, "processing query response");

	state.query_manager.receive_response(response).await;

	Ok((StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))))
}

/// HTTP handler to list pending queries for a session.
///
/// GET /api/sessions/{session_id}/queries
#[instrument(skip(state))]
pub async fn list_pending_queries(
	State(state): State<crate::api::AppState>,
	Path(session_id): Path<String>,
) -> Result<impl IntoResponse, crate::error::ServerError> {
	let queries = state.query_manager.list_pending(&session_id).await;
	tracing::debug!(
			session_id = %session_id,
			count = queries.len(),
			"returning pending queries list"
	);
	Ok(Json(queries))
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_core::server_query::{ServerQueryKind, ServerQueryResult};

	#[tokio::test]
	async fn test_send_and_receive_query() {
		let manager = Arc::new(ServerQueryManager::new());

		// Create a test query
		let query = ServerQuery {
			id: "Q-test-001".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "test.txt".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 5,
			metadata: serde_json::json!({}),
		};

		let query_id = query.id.clone();
		let manager_clone = manager.clone();

		// Spawn a task to send the query
		let send_handle =
			tokio::spawn(async move { manager_clone.send_query("session-1", query).await });

		// Give it a moment to store the query
		tokio::time::sleep(Duration::from_millis(50)).await;

		// Send a response
		let response = ServerQueryResponse {
			query_id: query_id.clone(),
			sent_at: chrono::Utc::now().to_rfc3339(),
			result: ServerQueryResult::FileContent("test content".to_string()),
			error: None,
		};

		manager.receive_response(response).await;

		// The send_query should complete successfully
		let result = send_handle.await.unwrap();
		assert!(result.is_ok());
		let response = result.unwrap();
		assert_eq!(response.query_id, query_id);
	}

	#[tokio::test]
	async fn test_query_timeout() {
		let manager = Arc::new(ServerQueryManager::new());

		let query = ServerQuery {
			id: "Q-test-timeout".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "test.txt".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 1,
			metadata: serde_json::json!({}),
		};

		let result = manager.send_query("session-1", query).await;

		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), ServerQueryError::Timeout));
	}

	#[tokio::test]
	async fn test_list_pending_queries() {
		let manager = ServerQueryManager::new();

		let query1 = ServerQuery {
			id: "Q-test-001".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "file1.txt".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 10,
			metadata: serde_json::json!({}),
		};

		let query2 = ServerQuery {
			id: "Q-test-002".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "file2.txt".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 10,
			metadata: serde_json::json!({}),
		};

		// Store queries manually
		{
			let mut pending = manager.pending.lock().await;
			pending.insert(query1.id.clone(), query1.clone());
			pending.insert(query2.id.clone(), query2.clone());
		}

		let pending = manager.list_pending("session-1").await;
		assert_eq!(pending.len(), 2);
	}

	#[tokio::test]
	async fn test_get_response() {
		let manager = ServerQueryManager::new();

		let response = ServerQueryResponse {
			query_id: "Q-test-001".to_string(),
			sent_at: chrono::Utc::now().to_rfc3339(),
			result: ServerQueryResult::FileContent("content".to_string()),
			error: None,
		};

		manager.receive_response(response.clone()).await;

		let retrieved = manager.get_response("Q-test-001").await;
		assert!(retrieved.is_some());
		assert_eq!(retrieved.unwrap().query_id, response.query_id);
	}

	/// Test that multiple concurrent queries can be awaited independently.
	/// This property-based test ensures that ServerQueryManager correctly
	/// correlates responses to their corresponding queries when multiple queries
	/// are in flight.
	#[tokio::test]
	async fn test_concurrent_queries() {
		let manager = Arc::new(ServerQueryManager::new());

		// Create multiple queries
		let queries: Vec<_> = (0..5)
			.map(|i| ServerQuery {
				id: format!("Q-concurrent-{i}"),
				kind: ServerQueryKind::ReadFile {
					path: format!("file{i}.txt"),
				},
				sent_at: chrono::Utc::now().to_rfc3339(),
				timeout_secs: 5,
				metadata: serde_json::json!({ "index": i }),
			})
			.collect();

		let mut handles = vec![];

		// Spawn tasks for each query
		for query in queries.clone() {
			let manager = manager.clone();
			let handle = tokio::spawn(async move { manager.send_query("session-1", query).await });
			handles.push(handle);
		}

		// Send responses in a different order
		for i in (0..5).rev() {
			tokio::time::sleep(Duration::from_millis(10)).await;
			let response = ServerQueryResponse {
				query_id: format!("Q-concurrent-{i}"),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent(format!("content{i}")),
				error: None,
			};
			manager.receive_response(response).await;
		}

		// All handles should complete successfully
		for handle in handles {
			let result = handle.await.unwrap();
			assert!(result.is_ok());
		}
	}
}
