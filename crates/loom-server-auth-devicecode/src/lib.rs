// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Device code flow for CLI authentication.
//!
//! This module implements the [Device Authorization Grant](https://www.rfc-editor.org/rfc/rfc8628)
//! used by the CLI and VS Code extension to authenticate users on devices with limited input
//! capabilities.
//!
//! # Overview
//!
//! The device code flow solves the problem of authenticating on headless or input-constrained
//! devices (like CLIs) where browser-based OAuth isn't directly possible.
//!
//! # The Flow
//!
//! ```text
//! ┌─────────┐                              ┌─────────┐                    ┌─────────┐
//! │   CLI   │                              │ Server  │                    │ Browser │
//! └────┬────┘                              └────┬────┘                    └────┬────┘
//!      │  POST /auth/device/start               │                              │
//!      │───────────────────────────────────────>│                              │
//!      │                                        │                              │
//!      │  {device_code, user_code, expires_at}  │                              │
//!      │<───────────────────────────────────────│                              │
//!      │                                        │                              │
//!      │  Display: "Enter 123-456-789 at URL"   │                              │
//!      │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─>│
//!      │                                        │                              │
//!      │                                        │   User visits /device        │
//!      │                                        │<─────────────────────────────│
//!      │                                        │                              │
//!      │                                        │   User enters code & logs in │
//!      │                                        │<─────────────────────────────│
//!      │                                        │                              │
//!      │  POST /auth/device/poll               │                              │
//!      │───────────────────────────────────────>│                              │
//!      │                                        │                              │
//!      │  {status: "completed", token: "..."}   │                              │
//!      │<───────────────────────────────────────│                              │
//! ```
//!
//! # Two Codes: Why?
//!
//! The flow uses two distinct codes for security and usability:
//!
//! - **`device_code`** (UUID): Internal identifier used by the CLI for polling. This is a
//!   cryptographically random UUID that's hard to guess. It never leaves the CLI-server
//!   communication channel.
//!
//! - **`user_code`** (XXX-XXX-XXX): Human-readable code displayed to the user. The format
//!   uses 9 digits grouped by dashes for easy reading and typing. Users enter this in the
//!   browser to link their authentication to the waiting CLI session.
//!
//! Separating these codes ensures that even if an attacker observes the user_code (e.g.,
//! shoulder surfing), they cannot poll for the result without the device_code.
//!
//! # Security Properties
//!
//! - Device codes expire after 10 minutes to limit the attack window
//! - Polling is rate-limited to 1 request per second
//! - User codes are single-use and bound to a specific device code
//! - The flow is immune to CSRF because it doesn't rely on browser cookies

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::{debug, instrument};
use uuid::Uuid;

/// Device code expiry time in minutes.
///
/// After this duration, the device code becomes invalid and the user must restart
/// the authentication flow. 10 minutes provides enough time for a user to complete
/// the browser-based authentication while limiting the window for attacks.
pub const DEVICE_CODE_EXPIRY_MINUTES: i64 = 10;

/// Recommended polling interval in seconds.
///
/// CLIs should wait at least this long between poll requests to avoid rate limiting.
/// The server may return `slow_down` if the client polls too frequently.
pub const POLL_INTERVAL_SECONDS: u64 = 1;

// =============================================================================
// UserId type
// =============================================================================

/// Unique identifier for a user.
///
/// This is a local copy of the type to avoid circular dependencies.
/// The main `UserId` type lives in `loom-auth`, which re-exports this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(Uuid);

impl UserId {
	/// Create a new ID from a UUID.
	pub fn new(id: Uuid) -> Self {
		Self(id)
	}

	/// Generate a new random ID.
	pub fn generate() -> Self {
		Self(Uuid::new_v4())
	}

	/// Get the inner UUID value.
	pub fn into_inner(self) -> Uuid {
		self.0
	}

	/// Get a reference to the inner UUID.
	pub fn as_uuid(&self) -> &Uuid {
		&self.0
	}
}

impl fmt::Display for UserId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl From<Uuid> for UserId {
	fn from(id: Uuid) -> Self {
		Self(id)
	}
}

impl From<UserId> for Uuid {
	fn from(id: UserId) -> Self {
		id.0
	}
}

// =============================================================================
// DeviceCodeStatus
// =============================================================================

/// Status of a device code authentication flow.
///
/// The state machine is simple and one-directional:
/// - A code starts as [`Pending`](DeviceCodeStatus::Pending)
/// - It transitions to either [`Completed`](DeviceCodeStatus::Completed) (user authenticated)
///   or [`Expired`](DeviceCodeStatus::Expired) (timeout without authentication)
/// - Once in a terminal state (Completed or Expired), the status never changes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceCodeStatus {
	/// Waiting for user to complete authentication in browser.
	Pending,
	/// User has completed authentication.
	Completed {
		/// The authenticated user's ID.
		user_id: UserId,
	},
	/// The device code has expired without completion.
	Expired,
}

// =============================================================================
// DeviceCode
// =============================================================================

/// A device code for CLI/extension authentication.
///
/// Represents a pending, completed, or expired device code authentication flow.
/// Created when a CLI starts authentication and updated when the user completes
/// the browser-based login.
///
/// # Example
///
/// ```
/// use loom_server_auth_devicecode::{DeviceCode, DeviceCodeStatus};
///
/// // CLI requests a new code
/// let mut code = DeviceCode::new();
/// assert!(matches!(code.status(), DeviceCodeStatus::Pending));
///
/// // User completes authentication in browser
/// let user_id = loom_server_auth_devicecode::UserId::generate();
/// code.complete(user_id);
/// assert!(matches!(code.status(), DeviceCodeStatus::Completed { .. }));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
	/// Internal tracking code (UUID) - used by CLI for polling.
	///
	/// This is a cryptographically random identifier that the CLI uses to poll
	/// for authentication completion. It should never be displayed to users.
	pub device_code: String,

	/// Human-readable code in format "123-456-789" - displayed to user.
	///
	/// This code is what users type into the browser. The format is designed
	/// to be easy to read aloud and type on any keyboard.
	pub user_code: String,

	/// The user who completed authentication (None until completed).
	pub user_id: Option<UserId>,

	/// When the device code was created.
	pub created_at: DateTime<Utc>,

	/// When the device code expires.
	pub expires_at: DateTime<Utc>,

	/// When the user completed authentication (None if pending/expired).
	pub completed_at: Option<DateTime<Utc>>,
}

impl DeviceCode {
	/// Create a new pending device code.
	///
	/// Generates both a device_code (internal UUID) and user_code (human-readable).
	/// The code expires after [`DEVICE_CODE_EXPIRY_MINUTES`].
	#[instrument(name = "device_code.new", skip_all, fields(device_code, user_code))]
	pub fn new() -> Self {
		let now = Utc::now();
		let device_code = generate_device_code();
		let user_code = generate_user_code();

		debug!(
				device_code = %device_code,
				user_code = %user_code,
				expires_in_minutes = DEVICE_CODE_EXPIRY_MINUTES,
				"created new device code"
		);

		Self {
			device_code,
			user_code,
			user_id: None,
			created_at: now,
			expires_at: now + Duration::minutes(DEVICE_CODE_EXPIRY_MINUTES),
			completed_at: None,
		}
	}

	/// Check if the device code has expired.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Check if the device code authentication has been completed.
	pub fn is_completed(&self) -> bool {
		self.completed_at.is_some() && self.user_id.is_some()
	}

	/// Check if the device code is still pending (not expired, not completed).
	pub fn is_pending(&self) -> bool {
		!self.is_expired() && !self.is_completed()
	}

	/// Get the current status of the device code.
	///
	/// Returns the appropriate status based on the code's state:
	/// - [`Completed`](DeviceCodeStatus::Completed) if a user has authenticated
	/// - [`Expired`](DeviceCodeStatus::Expired) if the expiry time has passed
	/// - [`Pending`](DeviceCodeStatus::Pending) otherwise
	#[instrument(
        name = "device_code.status",
        skip_all,
        fields(device_code = %self.device_code, status)
    )]
	pub fn status(&self) -> DeviceCodeStatus {
		let status = if let Some(user_id) = self.user_id {
			if self.completed_at.is_some() {
				DeviceCodeStatus::Completed { user_id }
			} else if self.is_expired() {
				DeviceCodeStatus::Expired
			} else {
				DeviceCodeStatus::Pending
			}
		} else if self.is_expired() {
			DeviceCodeStatus::Expired
		} else {
			DeviceCodeStatus::Pending
		};

		debug!(
				device_code = %self.device_code,
				status = ?status,
				"checked device code status"
		);

		status
	}

	/// Mark the device code as completed by the given user.
	///
	/// This is called when the user successfully authenticates in the browser
	/// and enters the correct user code. After this, polling will return success
	/// with the authentication token.
	///
	/// # State Transition
	///
	/// `Pending` → `Completed { user_id }`
	#[instrument(
        name = "device_code.complete",
        skip_all,
        fields(device_code = %self.device_code, user_id = %user_id)
    )]
	pub fn complete(&mut self, user_id: UserId) {
		debug!(
				device_code = %self.device_code,
				user_id = %user_id,
				"device code authentication completed"
		);

		self.user_id = Some(user_id);
		self.completed_at = Some(Utc::now());
	}
}

impl Default for DeviceCode {
	fn default() -> Self {
		Self::new()
	}
}

// =============================================================================
// Helper functions
// =============================================================================

/// Generate a human-readable user code in format "123-456-789" (9 digits).
///
/// This code is displayed to the user and entered in the browser.
/// The format was chosen for:
/// - Easy to read aloud (groups of 3)
/// - Easy to type on any keyboard (digits only)
/// - Sufficient entropy (10^9 possibilities = ~30 bits)
pub fn generate_user_code() -> String {
	let mut rng = rand::thread_rng();
	format!(
		"{:03}-{:03}-{:03}",
		rng.gen_range(0..1000),
		rng.gen_range(0..1000),
		rng.gen_range(0..1000)
	)
}

/// Generate an internal device code (UUID) for tracking.
///
/// This code is used by the CLI for polling and is not shown to the user.
/// It provides 128 bits of entropy, making it impossible to guess.
pub fn generate_device_code() -> String {
	Uuid::new_v4().to_string()
}

/// Validate that a user code is in the correct format "123-456-789".
///
/// Returns `true` if the code matches the expected pattern:
/// - Exactly 11 characters (9 digits + 2 dashes)
/// - Three groups of exactly 3 digits
/// - Groups separated by dashes
pub fn is_valid_user_code_format(code: &str) -> bool {
	if code.len() != 11 {
		return false;
	}
	let parts: Vec<&str> = code.split('-').collect();
	if parts.len() != 3 {
		return false;
	}
	parts
		.iter()
		.all(|part| part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
	use super::*;

	mod user_code {
		use super::*;

		#[test]
		fn generates_correct_format() {
			for _ in 0..100 {
				let code = generate_user_code();
				assert!(is_valid_user_code_format(&code), "Invalid format: {code}");
			}
		}

		#[test]
		fn validates_correct_format() {
			assert!(is_valid_user_code_format("123-456-789"));
			assert!(is_valid_user_code_format("000-000-000"));
			assert!(is_valid_user_code_format("999-999-999"));
		}

		#[test]
		fn rejects_invalid_formats() {
			assert!(!is_valid_user_code_format(""));
			assert!(!is_valid_user_code_format("123456789"));
			assert!(!is_valid_user_code_format("12-345-678"));
			assert!(!is_valid_user_code_format("1234-56-789"));
			assert!(!is_valid_user_code_format("abc-def-ghi"));
			assert!(!is_valid_user_code_format("123-456-78"));
			assert!(!is_valid_user_code_format("123-456-7890"));
			assert!(!is_valid_user_code_format("123_456_789"));
		}
	}

	mod device_code_struct {
		use super::*;

		#[test]
		fn new_creates_pending_code() {
			let code = DeviceCode::new();
			assert!(code.is_pending());
			assert!(!code.is_expired());
			assert!(!code.is_completed());
			assert!(code.user_id.is_none());
			assert!(code.completed_at.is_none());
			assert!(is_valid_user_code_format(&code.user_code));
			assert!(!code.device_code.is_empty());
		}

		#[test]
		fn status_returns_pending_for_new_code() {
			let code = DeviceCode::new();
			assert_eq!(code.status(), DeviceCodeStatus::Pending);
		}

		#[test]
		fn complete_sets_user_and_timestamp() {
			let mut code = DeviceCode::new();
			let user_id = UserId::generate();

			code.complete(user_id);

			assert!(code.is_completed());
			assert!(!code.is_pending());
			assert_eq!(code.user_id, Some(user_id));
			assert!(code.completed_at.is_some());
		}

		#[test]
		fn status_returns_completed_after_complete() {
			let mut code = DeviceCode::new();
			let user_id = UserId::generate();
			code.complete(user_id);

			assert_eq!(code.status(), DeviceCodeStatus::Completed { user_id });
		}

		#[test]
		fn is_expired_returns_false_for_new_code() {
			let code = DeviceCode::new();
			assert!(!code.is_expired());
		}

		#[test]
		fn is_expired_returns_true_for_past_expiry() {
			let mut code = DeviceCode::new();
			code.expires_at = Utc::now() - Duration::seconds(1);
			assert!(code.is_expired());
		}

		#[test]
		fn status_returns_expired_for_expired_code() {
			let mut code = DeviceCode::new();
			code.expires_at = Utc::now() - Duration::seconds(1);
			assert_eq!(code.status(), DeviceCodeStatus::Expired);
		}

		#[test]
		fn expiry_is_10_minutes_from_creation() {
			let code = DeviceCode::new();
			let expected_duration = Duration::minutes(DEVICE_CODE_EXPIRY_MINUTES);
			let actual_duration = code.expires_at - code.created_at;
			assert_eq!(actual_duration, expected_duration);
		}

		#[test]
		fn device_codes_are_unique() {
			let code1 = DeviceCode::new();
			let code2 = DeviceCode::new();
			assert_ne!(code1.device_code, code2.device_code);
		}

		#[test]
		fn user_codes_are_likely_unique() {
			let codes: Vec<String> = (0..100).map(|_| generate_user_code()).collect();
			let unique: std::collections::HashSet<_> = codes.iter().collect();
			assert!(unique.len() > 90, "Expected most codes to be unique");
		}
	}

	mod serialization {
		use super::*;

		#[test]
		fn device_code_status_pending_serializes() {
			let status = DeviceCodeStatus::Pending;
			let json = serde_json::to_string(&status).unwrap();
			assert!(json.contains("\"status\":\"pending\""));
		}

		#[test]
		fn device_code_status_expired_serializes() {
			let status = DeviceCodeStatus::Expired;
			let json = serde_json::to_string(&status).unwrap();
			assert!(json.contains("\"status\":\"expired\""));
		}

		#[test]
		fn device_code_status_completed_serializes_with_user_id() {
			let user_id = UserId::generate();
			let status = DeviceCodeStatus::Completed { user_id };
			let json = serde_json::to_string(&status).unwrap();
			assert!(json.contains("\"status\":\"completed\""));
			assert!(json.contains(&user_id.to_string()));
		}

		#[test]
		fn device_code_roundtrips() {
			let code = DeviceCode::new();
			let json = serde_json::to_string(&code).unwrap();
			let parsed: DeviceCode = serde_json::from_str(&json).unwrap();
			assert_eq!(code.device_code, parsed.device_code);
			assert_eq!(code.user_code, parsed.user_code);
		}
	}

	mod constants {
		use super::*;

		#[test]
		fn expiry_is_10_minutes() {
			assert_eq!(DEVICE_CODE_EXPIRY_MINUTES, 10);
		}

		#[test]
		fn poll_interval_is_1_second() {
			assert_eq!(POLL_INTERVAL_SECONDS, 1);
		}
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
			#[test]
			fn device_code_is_valid_uuid(_seed in 0u64..1000) {
					let code = generate_device_code();
					prop_assert!(Uuid::parse_str(&code).is_ok(), "device_code should be valid UUID: {}", code);
			}

			#[test]
			fn user_code_always_valid_format(_seed in 0u64..1000) {
					let code = generate_user_code();
					prop_assert!(is_valid_user_code_format(&code), "user_code should match XXX-XXX-XXX: {}", code);
					prop_assert_eq!(code.len(), 11);

					let parts: Vec<&str> = code.split('-').collect();
					prop_assert_eq!(parts.len(), 3);
					for part in parts {
							prop_assert_eq!(part.len(), 3);
							prop_assert!(part.chars().all(|c| c.is_ascii_digit()));
					}
			}

			#[test]
			fn pending_to_completed_transition(_seed in 0u64..1000) {
					let mut code = DeviceCode::new();
					prop_assert_eq!(code.status(), DeviceCodeStatus::Pending);

					let user_id = UserId::generate();
					code.complete(user_id);

					prop_assert_eq!(code.status(), DeviceCodeStatus::Completed { user_id });
					prop_assert!(code.is_completed());
					prop_assert!(!code.is_pending());
			}

			#[test]
			fn pending_to_expired_transition(_seed in 0u64..1000) {
					let mut code = DeviceCode::new();
					prop_assert_eq!(code.status(), DeviceCodeStatus::Pending);

					code.expires_at = Utc::now() - Duration::seconds(1);

					prop_assert_eq!(code.status(), DeviceCodeStatus::Expired);
					prop_assert!(code.is_expired());
					prop_assert!(!code.is_pending());
			}

			#[test]
			fn expiry_is_always_future(offset_seconds in 0i64..60) {
					let code = DeviceCode::new();
					let min_expected = Utc::now() + Duration::minutes(DEVICE_CODE_EXPIRY_MINUTES) - Duration::seconds(offset_seconds + 1);
					prop_assert!(code.expires_at >= min_expected, "expires_at should be ~10 minutes in future");
			}

			#[test]
			fn completed_beats_expired(_seed in 0u64..1000) {
					let mut code = DeviceCode::new();
					let user_id = UserId::generate();

					code.complete(user_id);
					code.expires_at = Utc::now() - Duration::seconds(1);

					prop_assert_eq!(code.status(), DeviceCodeStatus::Completed { user_id });
			}
	}
}
