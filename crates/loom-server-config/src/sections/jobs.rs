// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Jobs configuration section.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JobsConfigLayer {
	pub alert_enabled: Option<bool>,
	pub alert_recipients: Option<Vec<String>>,
	pub history_retention_days: Option<u32>,
	pub scm_maintenance_enabled: Option<bool>,
	pub scm_maintenance_interval_secs: Option<u64>,
	pub scm_maintenance_stagger_ms: Option<u64>,
}

impl JobsConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.alert_enabled.is_some() {
			self.alert_enabled = other.alert_enabled;
		}
		if other.alert_recipients.is_some() {
			self.alert_recipients = other.alert_recipients;
		}
		if other.history_retention_days.is_some() {
			self.history_retention_days = other.history_retention_days;
		}
		if other.scm_maintenance_enabled.is_some() {
			self.scm_maintenance_enabled = other.scm_maintenance_enabled;
		}
		if other.scm_maintenance_interval_secs.is_some() {
			self.scm_maintenance_interval_secs = other.scm_maintenance_interval_secs;
		}
		if other.scm_maintenance_stagger_ms.is_some() {
			self.scm_maintenance_stagger_ms = other.scm_maintenance_stagger_ms;
		}
	}

	pub fn finalize(self) -> JobsConfig {
		JobsConfig {
			alert_enabled: self.alert_enabled.unwrap_or(false),
			alert_recipients: self.alert_recipients.unwrap_or_default(),
			history_retention_days: self.history_retention_days.unwrap_or(90),
			scm_maintenance_enabled: self.scm_maintenance_enabled.unwrap_or(true),
			scm_maintenance_interval_secs: self.scm_maintenance_interval_secs.unwrap_or(86400), // 24 hours
			scm_maintenance_stagger_ms: self.scm_maintenance_stagger_ms.unwrap_or(100),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobsConfig {
	pub alert_enabled: bool,
	pub alert_recipients: Vec<String>,
	pub history_retention_days: u32,
	pub scm_maintenance_enabled: bool,
	pub scm_maintenance_interval_secs: u64,
	pub scm_maintenance_stagger_ms: u64,
}

impl Default for JobsConfig {
	fn default() -> Self {
		Self {
			alert_enabled: false,
			alert_recipients: Vec::new(),
			history_retention_days: 90,
			scm_maintenance_enabled: true,
			scm_maintenance_interval_secs: 86400, // 24 hours
			scm_maintenance_stagger_ms: 100,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = JobsConfig::default();
		assert!(!config.alert_enabled);
		assert!(config.alert_recipients.is_empty());
		assert_eq!(config.history_retention_days, 90);
	}

	#[test]
	fn test_layer_finalize_defaults() {
		let layer = JobsConfigLayer::default();
		let config = layer.finalize();
		assert!(!config.alert_enabled);
		assert!(config.alert_recipients.is_empty());
		assert_eq!(config.history_retention_days, 90);
	}

	#[test]
	fn test_layer_finalize_with_values() {
		let layer = JobsConfigLayer {
			alert_enabled: Some(true),
			alert_recipients: Some(vec!["admin@example.com".to_string()]),
			history_retention_days: Some(30),
			..Default::default()
		};
		let config = layer.finalize();
		assert!(config.alert_enabled);
		assert_eq!(
			config.alert_recipients,
			vec!["admin@example.com".to_string()]
		);
		assert_eq!(config.history_retention_days, 30);
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = JobsConfigLayer {
			alert_enabled: Some(false),
			alert_recipients: Some(vec!["old@example.com".to_string()]),
			history_retention_days: Some(90),
			..Default::default()
		};
		let overlay = JobsConfigLayer {
			alert_enabled: Some(true),
			alert_recipients: None,
			history_retention_days: Some(30),
			..Default::default()
		};
		base.merge(overlay);
		assert_eq!(base.alert_enabled, Some(true));
		assert_eq!(
			base.alert_recipients,
			Some(vec!["old@example.com".to_string()])
		);
		assert_eq!(base.history_retention_days, Some(30));
	}

	#[test]
	fn test_serde_roundtrip() {
		let config = JobsConfig {
			alert_enabled: true,
			alert_recipients: vec![
				"admin@example.com".to_string(),
				"ops@example.com".to_string(),
			],
			history_retention_days: 30,
			..Default::default()
		};
		let toml_str = toml::to_string(&config).unwrap();
		let parsed: JobsConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, parsed);
	}

	#[test]
	fn test_deserialize_layer_empty() {
		let layer: JobsConfigLayer = toml::from_str("").unwrap();
		assert!(layer.alert_enabled.is_none());
		assert!(layer.alert_recipients.is_none());
		assert!(layer.history_retention_days.is_none());
	}

	#[test]
	fn test_deserialize_layer_partial() {
		let toml_str = r#"
alert_enabled = true
"#;
		let layer: JobsConfigLayer = toml::from_str(toml_str).unwrap();
		assert_eq!(layer.alert_enabled, Some(true));
		assert!(layer.alert_recipients.is_none());
		assert!(layer.history_retention_days.is_none());
	}
}
