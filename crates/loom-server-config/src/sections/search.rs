// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Search provider configuration section.

use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchConfigLayer {
	#[serde(default)]
	pub google_cse: Option<GoogleCseConfigLayer>,
	#[serde(default)]
	pub serper: Option<SerperConfigLayer>,
}

impl SearchConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if let Some(other_google) = other.google_cse {
			let google = self.google_cse.get_or_insert_with(Default::default);
			google.merge(other_google);
		}
		if let Some(other_serper) = other.serper {
			let serper = self.serper.get_or_insert_with(Default::default);
			serper.merge(other_serper);
		}
	}

	pub fn finalize(self) -> SearchConfig {
		SearchConfig {
			google_cse: self.google_cse.map(|g| g.finalize()).unwrap_or_default(),
			serper: self.serper.map(|s| s.finalize()).unwrap_or_default(),
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleCseConfigLayer {
	pub api_key: Option<SecretString>,
	pub search_engine_id: Option<String>,
}

impl GoogleCseConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.api_key.is_some() {
			self.api_key = other.api_key;
		}
		if other.search_engine_id.is_some() {
			self.search_engine_id = other.search_engine_id;
		}
	}

	pub fn finalize(self) -> GoogleCseConfig {
		GoogleCseConfig {
			api_key: self.api_key,
			search_engine_id: self.search_engine_id,
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SerperConfigLayer {
	pub api_key: Option<SecretString>,
}

impl SerperConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.api_key.is_some() {
			self.api_key = other.api_key;
		}
	}

	pub fn finalize(self) -> SerperConfig {
		SerperConfig {
			api_key: self.api_key,
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchConfig {
	#[serde(default)]
	pub google_cse: GoogleCseConfig,
	#[serde(default)]
	pub serper: SerperConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleCseConfig {
	pub api_key: Option<SecretString>,
	pub search_engine_id: Option<String>,
}

impl GoogleCseConfig {
	pub fn is_configured(&self) -> bool {
		self.api_key.is_some() && self.search_engine_id.is_some()
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SerperConfig {
	pub api_key: Option<SecretString>,
}

impl SerperConfig {
	pub fn is_configured(&self) -> bool {
		self.api_key.is_some()
	}
}

impl SearchConfig {
	pub fn has_any_provider(&self) -> bool {
		self.google_cse.is_configured() || self.serper.is_configured()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_config::Secret;

	#[test]
	fn test_default_no_providers() {
		let config = SearchConfig::default();
		assert!(!config.has_any_provider());
		assert!(!config.google_cse.is_configured());
		assert!(!config.serper.is_configured());
	}

	#[test]
	fn test_google_cse_requires_both_fields() {
		let config = GoogleCseConfig {
			api_key: Some(Secret::new("key".to_string())),
			search_engine_id: None,
		};
		assert!(!config.is_configured());

		let config = GoogleCseConfig {
			api_key: None,
			search_engine_id: Some("engine_id".to_string()),
		};
		assert!(!config.is_configured());

		let config = GoogleCseConfig {
			api_key: Some(Secret::new("key".to_string())),
			search_engine_id: Some("engine_id".to_string()),
		};
		assert!(config.is_configured());
	}

	#[test]
	fn test_serper_configured() {
		let config = SerperConfig {
			api_key: Some(Secret::new("key".to_string())),
		};
		assert!(config.is_configured());
	}

	#[test]
	fn test_has_any_provider() {
		let mut config = SearchConfig::default();
		assert!(!config.has_any_provider());

		config.serper.api_key = Some(Secret::new("key".to_string()));
		assert!(config.has_any_provider());
	}

	#[test]
	fn test_deserialize_empty() {
		let config: SearchConfig = toml::from_str("").unwrap();
		assert!(!config.has_any_provider());
	}

	#[test]
	fn test_deserialize_with_providers() {
		let toml_str = r#"
[google_cse]
search_engine_id = "abc123"

[serper]
"#;
		let config: SearchConfig = toml::from_str(toml_str).unwrap();
		assert!(!config.google_cse.is_configured());
		assert_eq!(
			config.google_cse.search_engine_id,
			Some("abc123".to_string())
		);
	}

	#[test]
	fn test_layer_merge() {
		let mut base = SearchConfigLayer {
			google_cse: Some(GoogleCseConfigLayer {
				api_key: Some(Secret::new("old-key".to_string())),
				search_engine_id: Some("old-engine".to_string()),
			}),
			serper: None,
		};
		let overlay = SearchConfigLayer {
			google_cse: Some(GoogleCseConfigLayer {
				api_key: None,
				search_engine_id: Some("new-engine".to_string()),
			}),
			serper: Some(SerperConfigLayer {
				api_key: Some(Secret::new("serper-key".to_string())),
			}),
		};
		base.merge(overlay);

		let google = base.google_cse.as_ref().unwrap();
		assert!(google.api_key.is_some());
		assert_eq!(google.search_engine_id, Some("new-engine".to_string()));
		assert!(base.serper.is_some());
	}

	#[test]
	fn test_layer_finalize() {
		let layer = SearchConfigLayer {
			google_cse: Some(GoogleCseConfigLayer {
				api_key: Some(Secret::new("key".to_string())),
				search_engine_id: Some("engine".to_string()),
			}),
			serper: Some(SerperConfigLayer {
				api_key: Some(Secret::new("serper-key".to_string())),
			}),
		};
		let config = layer.finalize();
		assert!(config.google_cse.is_configured());
		assert!(config.serper.is_configured());
	}
}
