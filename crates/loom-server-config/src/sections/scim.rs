// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SCIM (System for Cross-domain Identity Management) configuration.

use loom_common_config::SecretString;
use serde::Deserialize;

/// SCIM configuration (runtime, fully resolved).
#[derive(Debug, Clone, Default)]
pub struct ScimConfig {
	#[allow(dead_code)]
	pub enabled: bool,
	pub token: Option<SecretString>,
	pub org_id: Option<String>,
}

/// SCIM configuration layer (partial, for merging).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScimConfigLayer {
	#[serde(default)]
	pub enabled: Option<bool>,
	#[serde(default)]
	pub org_id: Option<String>,
}

impl ScimConfigLayer {
	pub fn merge(&mut self, other: ScimConfigLayer) {
		if other.enabled.is_some() {
			self.enabled = other.enabled;
		}
		if other.org_id.is_some() {
			self.org_id = other.org_id;
		}
	}

	pub fn finalize(self, token: Option<SecretString>) -> ScimConfig {
		ScimConfig {
			enabled: self.enabled.unwrap_or(false),
			token,
			org_id: self.org_id,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_scim_disabled_by_default() {
		let config = ScimConfigLayer::default().finalize(None);
		assert!(!config.enabled);
		assert!(config.token.is_none());
		assert!(config.org_id.is_none());
	}

	#[test]
	fn test_scim_enabled() {
		let layer = ScimConfigLayer {
			enabled: Some(true),
			org_id: None,
		};
		let config = layer.finalize(Some(SecretString::new("test-token".to_string())));
		assert!(config.enabled);
		assert!(config.token.is_some());
	}
}
