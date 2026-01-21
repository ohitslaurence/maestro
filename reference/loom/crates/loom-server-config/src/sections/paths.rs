// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Server paths configuration section.

use serde::{Deserialize, Serialize};

fn default_bin_dir() -> String {
	"./bin".to_string()
}

fn default_data_dir() -> String {
	"./data".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PathsConfigLayer {
	pub bin_dir: Option<String>,
	pub web_dir: Option<String>,
	pub data_dir: Option<String>,
}

impl PathsConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.bin_dir.is_some() {
			self.bin_dir = other.bin_dir;
		}
		if other.web_dir.is_some() {
			self.web_dir = other.web_dir;
		}
		if other.data_dir.is_some() {
			self.data_dir = other.data_dir;
		}
	}

	pub fn finalize(self) -> PathsConfig {
		PathsConfig {
			bin_dir: self.bin_dir.unwrap_or_else(default_bin_dir),
			web_dir: self.web_dir,
			data_dir: self.data_dir.unwrap_or_else(default_data_dir),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathsConfig {
	pub bin_dir: String,
	pub web_dir: Option<String>,
	pub data_dir: String,
}

impl Default for PathsConfig {
	fn default() -> Self {
		Self {
			bin_dir: default_bin_dir(),
			web_dir: None,
			data_dir: default_data_dir(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = PathsConfig::default();
		assert_eq!(config.bin_dir, "./bin");
		assert!(config.web_dir.is_none());
		assert_eq!(config.data_dir, "./data");
	}

	#[test]
	fn test_layer_finalize_defaults() {
		let layer = PathsConfigLayer::default();
		let config = layer.finalize();
		assert_eq!(config.bin_dir, "./bin");
		assert!(config.web_dir.is_none());
		assert_eq!(config.data_dir, "./data");
	}

	#[test]
	fn test_layer_finalize_with_values() {
		let layer = PathsConfigLayer {
			bin_dir: Some("/opt/loom/bin".to_string()),
			web_dir: Some("/opt/loom/web".to_string()),
			data_dir: Some("/var/lib/loom".to_string()),
		};
		let config = layer.finalize();
		assert_eq!(config.bin_dir, "/opt/loom/bin");
		assert_eq!(config.web_dir, Some("/opt/loom/web".to_string()));
		assert_eq!(config.data_dir, "/var/lib/loom");
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = PathsConfigLayer {
			bin_dir: Some("/old/bin".to_string()),
			web_dir: Some("/old/web".to_string()),
			data_dir: Some("/old/data".to_string()),
		};
		let overlay = PathsConfigLayer {
			bin_dir: Some("/new/bin".to_string()),
			web_dir: None,
			data_dir: None,
		};
		base.merge(overlay);
		assert_eq!(base.bin_dir, Some("/new/bin".to_string()));
		assert_eq!(base.web_dir, Some("/old/web".to_string()));
		assert_eq!(base.data_dir, Some("/old/data".to_string()));
	}

	#[test]
	fn test_serde_roundtrip() {
		let config = PathsConfig {
			bin_dir: "/opt/loom/bin".to_string(),
			web_dir: Some("/opt/loom/web".to_string()),
			data_dir: "/var/lib/loom".to_string(),
		};
		let toml_str = toml::to_string(&config).unwrap();
		let parsed: PathsConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, parsed);
	}

	#[test]
	fn test_deserialize_layer_empty() {
		let layer: PathsConfigLayer = toml::from_str("").unwrap();
		assert!(layer.bin_dir.is_none());
		assert!(layer.web_dir.is_none());
		assert!(layer.data_dir.is_none());
	}

	#[test]
	fn test_deserialize_layer_partial() {
		let toml_str = r#"
bin_dir = "/custom/bin"
"#;
		let layer: PathsConfigLayer = toml::from_str(toml_str).unwrap();
		assert_eq!(layer.bin_dir, Some("/custom/bin".to_string()));
		assert!(layer.web_dir.is_none());
		assert!(layer.data_dir.is_none());
	}

	#[test]
	fn test_deserialize_with_web_dir() {
		let toml_str = r#"
bin_dir = "/bin"
web_dir = "/web"
data_dir = "/data"
"#;
		let layer: PathsConfigLayer = toml::from_str(toml_str).unwrap();
		let config = layer.finalize();
		assert_eq!(config.bin_dir, "/bin");
		assert_eq!(config.web_dir, Some("/web".to_string()));
		assert_eq!(config.data_dir, "/data");
	}
}
