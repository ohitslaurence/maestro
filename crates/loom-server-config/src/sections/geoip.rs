// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! GeoIP configuration section.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GeoIpConfigLayer {
	pub database_path: Option<String>,
}

impl GeoIpConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.database_path.is_some() {
			self.database_path = other.database_path;
		}
	}

	pub fn finalize(self) -> Option<GeoIpConfig> {
		self
			.database_path
			.map(|database_path| GeoIpConfig { database_path })
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoIpConfig {
	pub database_path: String,
}

impl GeoIpConfig {
	pub fn is_enabled(&self) -> bool {
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_layer() {
		let layer = GeoIpConfigLayer::default();
		assert!(layer.database_path.is_none());
	}

	#[test]
	fn test_finalize_none_when_no_path() {
		let layer = GeoIpConfigLayer::default();
		assert!(layer.finalize().is_none());
	}

	#[test]
	fn test_finalize_some_when_path_set() {
		let layer = GeoIpConfigLayer {
			database_path: Some("/var/lib/geoip/GeoLite2-City.mmdb".to_string()),
		};
		let config = layer.finalize();
		assert!(config.is_some());
		assert_eq!(
			config.unwrap().database_path,
			"/var/lib/geoip/GeoLite2-City.mmdb"
		);
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = GeoIpConfigLayer {
			database_path: Some("/old/path.mmdb".to_string()),
		};
		let overlay = GeoIpConfigLayer {
			database_path: Some("/new/path.mmdb".to_string()),
		};
		base.merge(overlay);
		assert_eq!(base.database_path, Some("/new/path.mmdb".to_string()));
	}

	#[test]
	fn test_merge_preserves_base_when_none() {
		let mut base = GeoIpConfigLayer {
			database_path: Some("/old/path.mmdb".to_string()),
		};
		let overlay = GeoIpConfigLayer {
			database_path: None,
		};
		base.merge(overlay);
		assert_eq!(base.database_path, Some("/old/path.mmdb".to_string()));
	}

	#[test]
	fn test_serde_roundtrip() {
		let layer = GeoIpConfigLayer {
			database_path: Some("/path/to/db.mmdb".to_string()),
		};
		let toml_str = toml::to_string(&layer).unwrap();
		let parsed: GeoIpConfigLayer = toml::from_str(&toml_str).unwrap();
		assert_eq!(layer, parsed);
	}

	#[test]
	fn test_deserialize_empty() {
		let layer: GeoIpConfigLayer = toml::from_str("").unwrap();
		assert!(layer.database_path.is_none());
	}
}
