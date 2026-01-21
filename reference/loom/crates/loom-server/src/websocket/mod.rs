// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! WebSocket communication module with first-message authentication.
//!
//! # Overview
//!
//! This module provides WebSocket support for low-latency bi-directional
//! communication between server and client.
//!
//! # Authentication
//!
//! All WebSocket connections require first-message authentication:
//!
//! 1. Client connects to `/v1/ws/sessions/{session_id}` - NO auth required at HTTP level
//! 2. Server starts 5-second auth timer
//! 3. Client must send first message: `{"type": "auth", "token": "lt_xxx"}` or
//!    `{"type": "auth", "session_token": "cookie_value"}`
//! 4. Server validates token using existing auth logic (hash + DB lookup)
//! 5. Success: `{"type": "auth_ok", "user_id": "..."}`
//! 6. Failure: `{"type": "auth_error", "message": "..."}` then close
//! 7. After auth_ok, normal message flow proceeds
//!
//! # State Machine
//!
//! ```text
//! ┌─────────────┐  auth msg   ┌───────────────┐
//! │   Pending   │────────────▶│ Authenticated │
//! │  (5s timer) │             │   (active)    │
//! └─────────────┘             └───────────────┘
//!       │
//!       │ timeout/invalid
//!       ▼
//! ┌─────────────┐
//! │   Closed    │
//! └─────────────┘
//! ```
//!
//! # Benefits
//!
//! 1. **Lower Latency**: No HTTP round-trip overhead per query
//! 2. **Bidirectional**: Server can push to client without polling
//! 3. **Persistent**: Eliminates connection overhead for each message
//! 4. **Backpressure**: Built-in flow control via WebSocket frame buffering
//! 5. **Unified Protocol**: Single connection replaces SSE + HTTP query endpoints

pub mod auth;
pub mod handler;

pub use auth::{
	auth_timeout, close_code_for_error, AuthError, AuthMessage, AuthResponse, AuthenticatedContext,
	WebSocketAuthState, AUTH_TIMEOUT_SECS,
};
pub use handler::{ws_upgrade_handler, WebSocketConnection, WsQueryParams};

pub mod config {
	use std::time::Duration;

	pub const PING_INTERVAL: Duration = Duration::from_secs(30);
	pub const PONG_TIMEOUT: Duration = Duration::from_secs(10);
	pub const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
	pub const MAX_QUEUE_SIZE: usize = 1000;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
	Auth(auth::AuthMessage),
	AuthOk {
		user_id: String,
	},
	AuthError {
		message: String,
	},
	ServerQuery {
		id: String,
		kind: serde_json::Value,
	},
	QueryResponse {
		query_id: String,
		result: serde_json::Value,
	},
	LlmEvent {
		event_type: String,
		data: serde_json::Value,
	},
	Ping {
		timestamp: i64,
	},
	Pong {
		timestamp: i64,
	},
	Control {
		command: String,
		payload: Option<serde_json::Value>,
	},
}

#[cfg(test)]
mod tests {
	use super::*;

	mod websocket_message {
		use super::*;

		#[test]
		fn ping_serialization() {
			let msg = WebSocketMessage::Ping {
				timestamp: 1234567890,
			};
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains("\"type\":\"ping\""));
			assert!(json.contains("\"timestamp\":1234567890"));

			let parsed: WebSocketMessage = serde_json::from_str(&json).unwrap();
			if let WebSocketMessage::Ping { timestamp } = parsed {
				assert_eq!(timestamp, 1234567890);
			} else {
				panic!("Expected Ping message");
			}
		}

		#[test]
		fn auth_ok_serialization() {
			let msg = WebSocketMessage::AuthOk {
				user_id: "user-123".to_string(),
			};
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains("\"type\":\"auth_ok\""));
			assert!(json.contains("\"user_id\":\"user-123\""));
		}

		#[test]
		fn auth_error_serialization() {
			let msg = WebSocketMessage::AuthError {
				message: "invalid token".to_string(),
			};
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains("\"type\":\"auth_error\""));
			assert!(json.contains("\"message\":\"invalid token\""));
		}

		#[test]
		fn query_response_serialization() {
			let msg = WebSocketMessage::QueryResponse {
				query_id: "Q-abc123".to_string(),
				result: serde_json::json!({"content": "file contents"}),
			};
			let json = serde_json::to_string(&msg).unwrap();
			assert!(json.contains("\"type\":\"query_response\""));
			assert!(json.contains("\"query_id\":\"Q-abc123\""));

			let parsed: WebSocketMessage = serde_json::from_str(&json).unwrap();
			if let WebSocketMessage::QueryResponse {
				query_id,
				result: _,
			} = parsed
			{
				assert_eq!(query_id, "Q-abc123");
			} else {
				panic!("Expected QueryResponse message");
			}
		}
	}

	mod config_tests {
		use super::*;

		#[test]
		fn ping_interval() {
			assert_eq!(config::PING_INTERVAL.as_secs(), 30);
		}

		#[test]
		fn pong_timeout() {
			assert_eq!(config::PONG_TIMEOUT.as_secs(), 10);
		}

		#[test]
		fn max_message_size() {
			assert_eq!(config::MAX_MESSAGE_SIZE, 10 * 1024 * 1024);
		}

		#[test]
		fn max_queue_size() {
			assert_eq!(config::MAX_QUEUE_SIZE, 1000);
		}

		#[test]
		fn auth_timeout_is_5_seconds() {
			assert_eq!(AUTH_TIMEOUT_SECS, 5);
		}
	}
}
