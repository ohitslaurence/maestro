// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Session management for web and CLI authentication.
//!
//! This module provides session lifecycle management including:
//!
//! - **Session creation**: New sessions with 60-day sliding expiry
//! - **Session validation**: Expiry checking and automatic extension
//! - **Token generation**: Cryptographically secure random tokens
//!
//! # Security Model
//!
//! - Session tokens are generated using 32 bytes of cryptographic randomness
//! - Tokens are stored hashed (see api_key and access_token modules for storage)
//! - Sessions use sliding expiry: each use extends the session by 60 days
//! - Metadata (IP, user agent, geo) is tracked for security auditing
//!
//! # PII Considerations
//!
//! Sessions contain potentially identifying metadata:
//! - `ip_address`: Client IP (may identify location)
//! - `user_agent`: Browser/client info
//! - `geo_city`, `geo_country`: Geolocation data

use crate::{SessionId, SessionType, UserId};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Duration for session sliding expiry (60 days).
pub const SESSION_EXPIRY_DAYS: i64 = 60;

/// A user session (web cookie or CLI token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
	pub id: SessionId,
	pub user_id: UserId,
	pub session_type: SessionType,
	pub created_at: DateTime<Utc>,
	pub last_used_at: DateTime<Utc>,
	pub expires_at: DateTime<Utc>,
	pub ip_address: Option<String>,
	pub user_agent: Option<String>,
	pub geo_city: Option<String>,
	pub geo_country: Option<String>,
}

impl Session {
	/// Create a new session with default 60-day expiry.
	///
	/// The session is created with the current timestamp and will expire
	/// in 60 days unless extended via [`Session::extend`].
	#[instrument(level = "debug", skip(user_id), fields(user_id = %user_id, session_type = ?session_type))]
	pub fn new(user_id: UserId, session_type: SessionType) -> Self {
		let now = Utc::now();
		let expires_at = now + Duration::days(SESSION_EXPIRY_DAYS);

		Self {
			id: SessionId::generate(),
			user_id,
			session_type,
			created_at: now,
			last_used_at: now,
			expires_at,
			ip_address: None,
			user_agent: None,
			geo_city: None,
			geo_country: None,
		}
	}

	/// Set IP address.
	pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
		self.ip_address = Some(ip.into());
		self
	}

	/// Set user agent.
	pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
		self.user_agent = Some(ua.into());
		self
	}

	/// Set geo location.
	pub fn with_geo(mut self, city: Option<String>, country: Option<String>) -> Self {
		self.geo_city = city;
		self.geo_country = country;
		self
	}

	/// Check if session is expired.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Extend the session (sliding expiry).
	pub fn extend(&mut self) {
		let now = Utc::now();
		self.last_used_at = now;
		self.expires_at = now + Duration::days(SESSION_EXPIRY_DAYS);
	}
}

/// Generates a cryptographically secure random session token.
pub fn generate_session_token() -> String {
	use rand::Rng;
	let mut rng = rand::thread_rng();
	let bytes: [u8; 32] = rng.gen();
	hex::encode(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;

	mod session_creation {
		use super::*;

		#[test]
		fn creates_session_with_correct_user_and_type() {
			let user_id = UserId::generate();
			let session = Session::new(user_id, SessionType::Web);

			assert_eq!(session.user_id, user_id);
			assert_eq!(session.session_type, SessionType::Web);
		}

		#[test]
		fn creates_session_with_60_day_expiry() {
			let session = Session::new(UserId::generate(), SessionType::Cli);

			let expected_expiry = session.created_at + Duration::days(SESSION_EXPIRY_DAYS);
			let diff = (session.expires_at - expected_expiry).num_seconds().abs();
			assert!(diff < 1, "Expiry should be ~60 days from creation");
		}

		#[test]
		fn creates_session_with_unique_ids() {
			let user_id = UserId::generate();
			let session1 = Session::new(user_id, SessionType::Web);
			let session2 = Session::new(user_id, SessionType::Web);

			assert_ne!(session1.id, session2.id);
		}

		#[test]
		fn builder_methods_set_metadata() {
			let session = Session::new(UserId::generate(), SessionType::Web)
				.with_ip("192.168.1.1")
				.with_user_agent("Mozilla/5.0")
				.with_geo(Some("Sydney".to_string()), Some("Australia".to_string()));

			assert_eq!(session.ip_address, Some("192.168.1.1".to_string()));
			assert_eq!(session.user_agent, Some("Mozilla/5.0".to_string()));
			assert_eq!(session.geo_city, Some("Sydney".to_string()));
			assert_eq!(session.geo_country, Some("Australia".to_string()));
		}
	}

	mod session_expiry {
		use super::*;

		#[test]
		fn new_session_is_not_expired() {
			let session = Session::new(UserId::generate(), SessionType::Web);
			assert!(!session.is_expired());
		}

		#[test]
		fn expired_session_is_detected() {
			let mut session = Session::new(UserId::generate(), SessionType::Web);
			session.expires_at = Utc::now() - Duration::seconds(1);
			assert!(session.is_expired());
		}
	}

	mod session_extension {
		use super::*;

		#[test]
		fn extend_updates_last_used_at() {
			let mut session = Session::new(UserId::generate(), SessionType::Web);
			let original_last_used = session.last_used_at;

			std::thread::sleep(std::time::Duration::from_millis(10));
			session.extend();

			assert!(session.last_used_at >= original_last_used);
		}

		#[test]
		fn extend_resets_expiry_to_60_days() {
			let mut session = Session::new(UserId::generate(), SessionType::Web);
			session.expires_at = Utc::now() + Duration::days(1);

			session.extend();

			let expected_expiry = Utc::now() + Duration::days(SESSION_EXPIRY_DAYS);
			let diff = (session.expires_at - expected_expiry).num_seconds().abs();
			assert!(diff < 1, "Expiry should be reset to ~60 days");
		}
	}

	mod token_generation {
		use super::*;
		use std::collections::HashSet;

		#[test]
		fn generates_64_char_hex_string() {
			let token = generate_session_token();
			assert_eq!(token.len(), 64);
			assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn generates_unique_tokens() {
			let tokens: HashSet<_> = (0..100).map(|_| generate_session_token()).collect();
			assert_eq!(tokens.len(), 100, "All tokens should be unique");
		}
	}
}
