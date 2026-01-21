// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Logging configuration section.

use serde::{Deserialize, Serialize};

fn default_level() -> String {
	"info,tower_http::trace=debug,reqwest=debug,hyper=debug".to_string()
}

fn default_locale() -> String {
	"en".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LoggingConfigLayer {
	pub level: Option<String>,
	pub locale: Option<String>,
}

impl LoggingConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.level.is_some() {
			self.level = other.level;
		}
		if other.locale.is_some() {
			self.locale = other.locale;
		}
	}

	pub fn finalize(self) -> LoggingConfig {
		LoggingConfig {
			level: self.level.unwrap_or_else(default_level),
			locale: self.locale.unwrap_or_else(default_locale),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoggingConfig {
	pub level: String,
	pub locale: String,
}

impl Default for LoggingConfig {
	fn default() -> Self {
		Self {
			level: default_level(),
			locale: default_locale(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = LoggingConfig::default();
		assert_eq!(
			config.level,
			"info,tower_http::trace=debug,reqwest=debug,hyper=debug"
		);
		assert_eq!(config.locale, "en");
	}

	#[test]
	fn test_layer_finalize_defaults() {
		let layer = LoggingConfigLayer::default();
		let config = layer.finalize();
		assert_eq!(
			config.level,
			"info,tower_http::trace=debug,reqwest=debug,hyper=debug"
		);
		assert_eq!(config.locale, "en");
	}

	#[test]
	fn test_layer_finalize_with_values() {
		let layer = LoggingConfigLayer {
			level: Some("debug".to_string()),
			locale: Some("es".to_string()),
		};
		let config = layer.finalize();
		assert_eq!(config.level, "debug");
		assert_eq!(config.locale, "es");
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = LoggingConfigLayer {
			level: Some("info".to_string()),
			locale: Some("en".to_string()),
		};
		let overlay = LoggingConfigLayer {
			level: Some("warn".to_string()),
			locale: None,
		};
		base.merge(overlay);
		assert_eq!(base.level, Some("warn".to_string()));
		assert_eq!(base.locale, Some("en".to_string()));
	}

	#[test]
	fn test_serde_roundtrip() {
		let config = LoggingConfig {
			level: "debug".to_string(),
			locale: "es".to_string(),
		};
		let toml_str = toml::to_string(&config).unwrap();
		let parsed: LoggingConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, parsed);
	}

	#[test]
	fn test_deserialize_layer_empty() {
		let layer: LoggingConfigLayer = toml::from_str("").unwrap();
		assert!(layer.level.is_none());
		assert!(layer.locale.is_none());
	}

	#[test]
	fn test_deserialize_layer_partial() {
		let toml_str = r#"
level = "warn"
"#;
		let layer: LoggingConfigLayer = toml::from_str(toml_str).unwrap();
		assert_eq!(layer.level, Some("warn".to_string()));
		assert!(layer.locale.is_none());
	}

	#[test]
	fn test_custom_locale() {
		let layer = LoggingConfigLayer {
			level: None,
			locale: Some("ar".to_string()),
		};
		let config = layer.finalize();
		assert_eq!(
			config.level,
			"info,tower_http::trace=debug,reqwest=debug,hyper=debug"
		);
		assert_eq!(config.locale, "ar");
	}
}
