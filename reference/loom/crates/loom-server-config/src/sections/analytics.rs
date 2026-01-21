// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Analytics configuration section.

use serde::{Deserialize, Serialize};

const DEFAULT_BATCH_SIZE: usize = 100;
const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 10;
const DEFAULT_EVENT_RETENTION_DAYS: i64 = 365;

fn default_batch_size() -> usize {
	DEFAULT_BATCH_SIZE
}

fn default_flush_interval_secs() -> u64 {
	DEFAULT_FLUSH_INTERVAL_SECS
}

fn default_event_retention_days() -> i64 {
	DEFAULT_EVENT_RETENTION_DAYS
}

/// Configuration layer for analytics (all fields optional for merging).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsConfigLayer {
	/// Enable the analytics system.
	pub enabled: Option<bool>,
	/// Maximum events per batch.
	pub batch_size: Option<usize>,
	/// SDK flush interval in seconds.
	pub flush_interval_secs: Option<u64>,
	/// Event retention period in days.
	pub event_retention_days: Option<i64>,
}

impl AnalyticsConfigLayer {
	/// Merge another layer into this one. Other layer takes precedence.
	pub fn merge(&mut self, other: Self) {
		if other.enabled.is_some() {
			self.enabled = other.enabled;
		}
		if other.batch_size.is_some() {
			self.batch_size = other.batch_size;
		}
		if other.flush_interval_secs.is_some() {
			self.flush_interval_secs = other.flush_interval_secs;
		}
		if other.event_retention_days.is_some() {
			self.event_retention_days = other.event_retention_days;
		}
	}

	/// Convert to resolved configuration with defaults applied.
	pub fn finalize(self) -> AnalyticsConfig {
		AnalyticsConfig {
			enabled: self.enabled.unwrap_or(true),
			batch_size: self.batch_size.unwrap_or_else(default_batch_size),
			flush_interval_secs: self
				.flush_interval_secs
				.unwrap_or_else(default_flush_interval_secs),
			event_retention_days: self
				.event_retention_days
				.unwrap_or_else(default_event_retention_days),
		}
	}
}

/// Resolved analytics configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsConfig {
	/// Enable the analytics system.
	pub enabled: bool,
	/// Maximum events per batch.
	pub batch_size: usize,
	/// SDK flush interval in seconds.
	pub flush_interval_secs: u64,
	/// Event retention period in days.
	pub event_retention_days: i64,
}

impl Default for AnalyticsConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			batch_size: default_batch_size(),
			flush_interval_secs: default_flush_interval_secs(),
			event_retention_days: default_event_retention_days(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = AnalyticsConfig::default();
		assert!(config.enabled);
		assert_eq!(config.batch_size, 100);
		assert_eq!(config.flush_interval_secs, 10);
		assert_eq!(config.event_retention_days, 365);
	}

	#[test]
	fn test_layer_finalize_defaults() {
		let layer = AnalyticsConfigLayer::default();
		let config = layer.finalize();
		assert!(config.enabled);
		assert_eq!(config.batch_size, 100);
		assert_eq!(config.flush_interval_secs, 10);
		assert_eq!(config.event_retention_days, 365);
	}

	#[test]
	fn test_layer_finalize_with_values() {
		let layer = AnalyticsConfigLayer {
			enabled: Some(false),
			batch_size: Some(50),
			flush_interval_secs: Some(5),
			event_retention_days: Some(30),
		};
		let config = layer.finalize();
		assert!(!config.enabled);
		assert_eq!(config.batch_size, 50);
		assert_eq!(config.flush_interval_secs, 5);
		assert_eq!(config.event_retention_days, 30);
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = AnalyticsConfigLayer {
			enabled: Some(true),
			batch_size: Some(100),
			..Default::default()
		};
		let overlay = AnalyticsConfigLayer {
			enabled: Some(false),
			flush_interval_secs: Some(20),
			..Default::default()
		};
		base.merge(overlay);
		assert_eq!(base.enabled, Some(false));
		assert_eq!(base.batch_size, Some(100));
		assert_eq!(base.flush_interval_secs, Some(20));
		assert_eq!(base.event_retention_days, None);
	}

	#[test]
	fn test_merge_preserves_base_when_other_none() {
		let mut base = AnalyticsConfigLayer {
			enabled: Some(true),
			batch_size: Some(200),
			flush_interval_secs: Some(15),
			event_retention_days: Some(180),
		};
		let overlay = AnalyticsConfigLayer::default();
		base.merge(overlay);
		assert_eq!(base.enabled, Some(true));
		assert_eq!(base.batch_size, Some(200));
		assert_eq!(base.flush_interval_secs, Some(15));
		assert_eq!(base.event_retention_days, Some(180));
	}

	#[test]
	fn test_toml_roundtrip() {
		let config = AnalyticsConfig {
			enabled: true,
			batch_size: 50,
			flush_interval_secs: 5,
			event_retention_days: 90,
		};
		let toml_str = toml::to_string(&config).unwrap();
		let parsed: AnalyticsConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, parsed);
	}

	#[test]
	fn test_serde_json_roundtrip() {
		let layer = AnalyticsConfigLayer {
			enabled: Some(true),
			batch_size: Some(75),
			flush_interval_secs: Some(30),
			event_retention_days: Some(180),
		};
		let json = serde_json::to_string(&layer).unwrap();
		let parsed: AnalyticsConfigLayer = serde_json::from_str(&json).unwrap();
		assert_eq!(layer, parsed);
	}
}
