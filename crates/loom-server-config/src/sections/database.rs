// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Database configuration.

use serde::Deserialize;

/// Database configuration (runtime, fully resolved).
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
	pub url: String,
}

impl Default for DatabaseConfig {
	fn default() -> Self {
		Self {
			url: "sqlite:./loom.db".to_string(),
		}
	}
}

/// Database configuration layer (partial, for merging).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DatabaseConfigLayer {
	#[serde(default)]
	pub url: Option<String>,
}

impl DatabaseConfigLayer {
	pub fn merge(&mut self, other: DatabaseConfigLayer) {
		if other.url.is_some() {
			self.url = other.url;
		}
	}

	pub fn finalize(self) -> DatabaseConfig {
		DatabaseConfig {
			url: self.url.unwrap_or_else(|| "sqlite:./loom.db".to_string()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_url() {
		let config = DatabaseConfigLayer::default().finalize();
		assert_eq!(config.url, "sqlite:./loom.db");
	}

	#[test]
	fn test_custom_url() {
		let layer = DatabaseConfigLayer {
			url: Some("sqlite:/var/lib/loom/data.db".to_string()),
		};
		let config = layer.finalize();
		assert_eq!(config.url, "sqlite:/var/lib/loom/data.db");
	}
}
