// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication configuration.

use serde::Deserialize;

/// Authentication configuration (runtime, fully resolved).
#[derive(Debug, Clone)]
pub struct AuthConfig {
	pub dev_mode: bool,
	pub environment: String,
	pub session_cleanup_interval_secs: u64,
	pub oauth_state_cleanup_interval_secs: u64,
	pub signups_disabled: bool,
}

impl Default for AuthConfig {
	fn default() -> Self {
		Self {
			dev_mode: false,
			environment: "development".to_string(),
			session_cleanup_interval_secs: 3600,
			oauth_state_cleanup_interval_secs: 900,
			signups_disabled: false,
		}
	}
}

/// Authentication configuration layer (partial, for merging).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfigLayer {
	#[serde(default)]
	pub dev_mode: Option<bool>,
	#[serde(default)]
	pub environment: Option<String>,
	#[serde(default)]
	pub session_cleanup_interval_secs: Option<u64>,
	#[serde(default)]
	pub oauth_state_cleanup_interval_secs: Option<u64>,
	#[serde(default)]
	pub signups_disabled: Option<bool>,
}

impl AuthConfigLayer {
	pub fn merge(&mut self, other: AuthConfigLayer) {
		if other.dev_mode.is_some() {
			self.dev_mode = other.dev_mode;
		}
		if other.environment.is_some() {
			self.environment = other.environment;
		}
		if other.session_cleanup_interval_secs.is_some() {
			self.session_cleanup_interval_secs = other.session_cleanup_interval_secs;
		}
		if other.oauth_state_cleanup_interval_secs.is_some() {
			self.oauth_state_cleanup_interval_secs = other.oauth_state_cleanup_interval_secs;
		}
		if other.signups_disabled.is_some() {
			self.signups_disabled = other.signups_disabled;
		}
	}

	pub fn finalize(self) -> AuthConfig {
		AuthConfig {
			dev_mode: self.dev_mode.unwrap_or(false),
			environment: self
				.environment
				.unwrap_or_else(|| "development".to_string()),
			session_cleanup_interval_secs: self.session_cleanup_interval_secs.unwrap_or(3600),
			oauth_state_cleanup_interval_secs: self.oauth_state_cleanup_interval_secs.unwrap_or(900),
			signups_disabled: self.signups_disabled.unwrap_or(false),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_dev_mode_default_false() {
		let config = AuthConfigLayer::default().finalize();
		assert!(!config.dev_mode);
	}

	#[test]
	fn test_dev_mode_enabled() {
		let layer = AuthConfigLayer {
			dev_mode: Some(true),
			..Default::default()
		};
		let config = layer.finalize();
		assert!(config.dev_mode);
	}
}
