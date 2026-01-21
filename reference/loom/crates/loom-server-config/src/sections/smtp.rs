// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SMTP configuration section for email delivery.

use crate::error::ConfigError;
use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};

/// TLS mode for SMTP connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TlsMode {
	/// No TLS (plain text connection).
	None,
	/// STARTTLS upgrade after connecting.
	StartTls,
	/// Direct TLS connection.
	#[default]
	Tls,
}

impl TlsMode {
	/// Parse TLS mode from string value.
	pub fn from_str_value(value: &str) -> Result<Self, ConfigError> {
		match value.to_lowercase().as_str() {
			"true" | "tls" => Ok(TlsMode::Tls),
			"starttls" => Ok(TlsMode::StartTls),
			"false" | "none" => Ok(TlsMode::None),
			_ => Err(ConfigError::InvalidValue {
				key: "tls_mode".to_string(),
				message: format!("Invalid value: '{value}'. Expected: true, tls, starttls, false, none"),
			}),
		}
	}
}

/// Configuration layer for SMTP settings (all fields optional for layering).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmtpConfigLayer {
	/// SMTP server hostname.
	pub host: Option<String>,
	/// SMTP server port.
	pub port: Option<u16>,
	/// Username for SMTP authentication.
	pub username: Option<String>,
	/// Password for SMTP authentication.
	#[serde(skip_serializing)]
	pub password: Option<SecretString>,
	/// Email address to send from.
	pub from_address: Option<String>,
	/// Display name for sent emails.
	pub from_name: Option<String>,
	/// Whether to use TLS for SMTP connection.
	pub use_tls: Option<bool>,
	/// TLS mode for the connection.
	pub tls_mode: Option<TlsMode>,
}

impl SmtpConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: SmtpConfigLayer) {
		if other.host.is_some() {
			self.host = other.host;
		}
		if other.port.is_some() {
			self.port = other.port;
		}
		if other.username.is_some() {
			self.username = other.username;
		}
		if other.password.is_some() {
			self.password = other.password;
		}
		if other.from_address.is_some() {
			self.from_address = other.from_address;
		}
		if other.from_name.is_some() {
			self.from_name = other.from_name;
		}
		if other.use_tls.is_some() {
			self.use_tls = other.use_tls;
		}
		if other.tls_mode.is_some() {
			self.tls_mode = other.tls_mode;
		}
	}

	/// Check if SMTP is configured (host is set).
	pub fn is_configured(&self) -> bool {
		self.host.as_ref().is_some_and(|h| !h.is_empty())
	}

	/// Finalize the layer into a runtime configuration.
	pub fn finalize(self) -> Option<SmtpConfig> {
		self.build().ok().flatten()
	}

	/// Build the final config, returning None if SMTP is not configured.
	pub fn build(self) -> Result<Option<SmtpConfig>, ConfigError> {
		let Some(host) = self.host.filter(|h| !h.is_empty()) else {
			return Ok(None);
		};

		let from_address = self.from_address.ok_or_else(|| {
			ConfigError::Validation("SMTP from_address is required when host is configured".to_string())
		})?;

		if from_address.is_empty() {
			return Err(ConfigError::Validation(
				"SMTP from_address cannot be empty".to_string(),
			));
		}

		let tls_mode = self.tls_mode.unwrap_or_else(|| {
			if self.use_tls.unwrap_or(true) {
				TlsMode::Tls
			} else {
				TlsMode::None
			}
		});

		Ok(Some(SmtpConfig {
			host,
			port: self.port.unwrap_or(587),
			username: self.username,
			password: self.password,
			from_address,
			from_name: self.from_name.unwrap_or_else(|| "Loom".to_string()),
			tls_mode,
		}))
	}
}

/// Validated SMTP configuration.
#[derive(Debug, Clone)]
pub struct SmtpConfig {
	/// SMTP server hostname.
	pub host: String,
	/// SMTP server port.
	pub port: u16,
	/// Optional username for SMTP authentication.
	pub username: Option<String>,
	/// Optional password for SMTP authentication.
	pub password: Option<SecretString>,
	/// From address for outgoing emails.
	pub from_address: String,
	/// Display name for sent emails.
	pub from_name: String,
	/// TLS mode for the connection.
	pub tls_mode: TlsMode,
}

impl SmtpConfig {
	/// Check if authentication credentials are configured.
	pub fn has_auth(&self) -> bool {
		self.username.is_some() && self.password.is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_config::Secret;

	mod tls_mode {
		use super::*;

		#[test]
		fn parses_tls_variants() {
			assert_eq!(TlsMode::from_str_value("tls").unwrap(), TlsMode::Tls);
			assert_eq!(TlsMode::from_str_value("TLS").unwrap(), TlsMode::Tls);
			assert_eq!(TlsMode::from_str_value("true").unwrap(), TlsMode::Tls);
			assert_eq!(TlsMode::from_str_value("TRUE").unwrap(), TlsMode::Tls);
		}

		#[test]
		fn parses_starttls_variants() {
			assert_eq!(
				TlsMode::from_str_value("starttls").unwrap(),
				TlsMode::StartTls
			);
			assert_eq!(
				TlsMode::from_str_value("STARTTLS").unwrap(),
				TlsMode::StartTls
			);
		}

		#[test]
		fn parses_none_variants() {
			assert_eq!(TlsMode::from_str_value("none").unwrap(), TlsMode::None);
			assert_eq!(TlsMode::from_str_value("NONE").unwrap(), TlsMode::None);
			assert_eq!(TlsMode::from_str_value("false").unwrap(), TlsMode::None);
			assert_eq!(TlsMode::from_str_value("FALSE").unwrap(), TlsMode::None);
		}

		#[test]
		fn rejects_invalid_value() {
			assert!(TlsMode::from_str_value("invalid").is_err());
		}

		#[test]
		fn default_is_tls() {
			assert_eq!(TlsMode::default(), TlsMode::Tls);
		}
	}

	mod smtp_config_layer {
		use super::*;

		#[test]
		fn returns_none_when_host_not_set() {
			let layer = SmtpConfigLayer::default();
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn returns_none_when_host_is_empty() {
			let layer = SmtpConfigLayer {
				host: Some(String::new()),
				..Default::default()
			};
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn requires_from_address_when_host_set() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn rejects_empty_from_address() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				from_address: Some(String::new()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn builds_minimal_config() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				from_address: Some("noreply@example.com".to_string()),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.host, "smtp.example.com");
			assert_eq!(config.port, 587);
			assert_eq!(config.from_address, "noreply@example.com");
			assert_eq!(config.from_name, "Loom");
			assert_eq!(config.tls_mode, TlsMode::Tls);
			assert!(config.username.is_none());
			assert!(config.password.is_none());
			assert!(!config.has_auth());
		}

		#[test]
		fn builds_full_config() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				port: Some(465),
				username: Some("user@example.com".to_string()),
				password: Some(Secret::new("secret123".to_string())),
				from_address: Some("noreply@example.com".to_string()),
				from_name: Some("My App".to_string()),
				tls_mode: Some(TlsMode::StartTls),
				use_tls: None,
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.host, "smtp.example.com");
			assert_eq!(config.port, 465);
			assert_eq!(config.username, Some("user@example.com".to_string()));
			assert!(config.password.is_some());
			assert_eq!(config.from_address, "noreply@example.com");
			assert_eq!(config.from_name, "My App");
			assert_eq!(config.tls_mode, TlsMode::StartTls);
			assert!(config.has_auth());
		}

		#[test]
		fn merge_prefers_other_values() {
			let mut base = SmtpConfigLayer {
				host: Some("base.example.com".to_string()),
				port: Some(25),
				from_address: Some("base@example.com".to_string()),
				..Default::default()
			};

			let overlay = SmtpConfigLayer {
				host: Some("overlay.example.com".to_string()),
				from_name: Some("Overlay".to_string()),
				..Default::default()
			};

			base.merge(overlay);

			assert_eq!(base.host, Some("overlay.example.com".to_string()));
			assert_eq!(base.port, Some(25)); // Not overwritten
			assert_eq!(base.from_address, Some("base@example.com".to_string()));
			assert_eq!(base.from_name, Some("Overlay".to_string()));
		}

		#[test]
		fn use_tls_sets_tls_mode() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				from_address: Some("noreply@example.com".to_string()),
				use_tls: Some(false),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.tls_mode, TlsMode::None);
		}

		#[test]
		fn tls_mode_overrides_use_tls() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				from_address: Some("noreply@example.com".to_string()),
				use_tls: Some(false),
				tls_mode: Some(TlsMode::StartTls),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.tls_mode, TlsMode::StartTls);
		}
	}

	mod secret_redaction {
		use super::*;

		#[test]
		fn password_not_in_debug_output() {
			let layer = SmtpConfigLayer {
				password: Some(Secret::new("super_secret_password".to_string())),
				..Default::default()
			};

			let debug_output = format!("{layer:?}");
			assert!(!debug_output.contains("super_secret_password"));
			assert!(debug_output.contains("[REDACTED]"));
		}

		#[test]
		fn password_not_serialized() {
			let layer = SmtpConfigLayer {
				host: Some("smtp.example.com".to_string()),
				password: Some(Secret::new("secret".to_string())),
				..Default::default()
			};

			let json = serde_json::to_string(&layer).unwrap();
			assert!(!json.contains("secret"));
			assert!(!json.contains("password"));
		}
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use loom_common_config::Secret;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn valid_layer_with_host_and_from_builds_successfully(
			host in "[a-z]{3,10}\\.[a-z]{2,5}",
			from_address in "[a-z]{3,10}@[a-z]{3,10}\\.[a-z]{2,3}",
		) {
			let layer = SmtpConfigLayer {
				host: Some(host.clone()),
				from_address: Some(from_address.clone()),
				..Default::default()
			};

			let result = layer.build();
			prop_assert!(result.is_ok());

			let config = result.unwrap().unwrap();
			prop_assert_eq!(config.host, host);
			prop_assert_eq!(config.from_address, from_address);
		}

		#[test]
		fn password_never_in_debug(
			password in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!password.contains("REDACTED"));

			let layer = SmtpConfigLayer {
				password: Some(Secret::new(password.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&password));
		}

		#[test]
		fn port_defaults_to_587(
			host in "[a-z]{3,10}\\.[a-z]{2,5}",
			from_address in "[a-z]{3,10}@[a-z]{3,10}\\.[a-z]{2,3}",
		) {
			let layer = SmtpConfigLayer {
				host: Some(host),
				from_address: Some(from_address),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			prop_assert_eq!(config.port, 587);
		}
	}
}
