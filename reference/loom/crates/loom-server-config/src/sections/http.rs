// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! HTTP server configuration.

use serde::Deserialize;

/// HTTP server configuration (runtime, fully resolved).
#[derive(Debug, Clone)]
pub struct HttpConfig {
	pub host: String,
	pub port: u16,
	pub base_url: String,
}

impl Default for HttpConfig {
	fn default() -> Self {
		Self {
			host: "0.0.0.0".to_string(),
			port: 8080,
			base_url: "http://localhost:8080".to_string(),
		}
	}
}

/// HTTP configuration layer (partial, for merging).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HttpConfigLayer {
	#[serde(default)]
	pub host: Option<String>,
	#[serde(default)]
	pub port: Option<u16>,
	#[serde(default)]
	pub base_url: Option<String>,
}

impl HttpConfigLayer {
	pub fn merge(&mut self, other: HttpConfigLayer) {
		if other.host.is_some() {
			self.host = other.host;
		}
		if other.port.is_some() {
			self.port = other.port;
		}
		if other.base_url.is_some() {
			self.base_url = other.base_url;
		}
	}

	pub fn finalize(self) -> HttpConfig {
		let port = self.port.unwrap_or(8080);
		HttpConfig {
			host: self.host.unwrap_or_else(|| "0.0.0.0".to_string()),
			port,
			base_url: self
				.base_url
				.unwrap_or_else(|| format!("http://localhost:{}", port)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = HttpConfigLayer::default().finalize();
		assert_eq!(config.host, "0.0.0.0");
		assert_eq!(config.port, 8080);
		assert_eq!(config.base_url, "http://localhost:8080");
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = HttpConfigLayer {
			host: Some("127.0.0.1".to_string()),
			port: Some(3000),
			base_url: None,
		};
		let overlay = HttpConfigLayer {
			host: None,
			port: Some(9000),
			base_url: Some("https://example.com".to_string()),
		};
		base.merge(overlay);
		assert_eq!(base.host, Some("127.0.0.1".to_string()));
		assert_eq!(base.port, Some(9000));
		assert_eq!(base.base_url, Some("https://example.com".to_string()));
	}

	#[test]
	fn test_base_url_defaults_to_port() {
		let layer = HttpConfigLayer {
			host: None,
			port: Some(9090),
			base_url: None,
		};
		let config = layer.finalize();
		assert_eq!(config.base_url, "http://localhost:9090");
	}
}
