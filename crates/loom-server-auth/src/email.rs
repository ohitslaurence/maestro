// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Email client module for sending magic links and notifications.
//!
//! This module provides SMTP configuration, email templates, and rendering
//! for authentication-related emails including magic links, security
//! notifications, organization invitations, and account deletion warnings.

use crate::AuthError;
use loom_common_secret::SecretString;
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
	/// Parse TLS mode from environment variable value.
	///
	/// - "true" or "tls" -> Tls
	/// - "starttls" -> StartTls
	/// - "false" or "none" -> None
	pub fn from_env_value(value: &str) -> Result<Self, AuthError> {
		match value.to_lowercase().as_str() {
			"true" | "tls" => Ok(TlsMode::Tls),
			"starttls" => Ok(TlsMode::StartTls),
			"false" | "none" => Ok(TlsMode::None),
			_ => Err(AuthError::Configuration(format!(
				"Invalid LOOM_SERVER_SMTP_TLS value: '{value}'. Expected: true, tls, starttls, false, none"
			))),
		}
	}
}

/// SMTP configuration for sending emails.
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
	/// TLS mode for the connection.
	pub tls_mode: TlsMode,
}

impl SmtpConfig {
	/// Load SMTP configuration from environment variables.
	///
	/// Returns `Ok(None)` if SMTP is not configured (LOOM_SERVER_SMTP_HOST not set).
	/// Returns `Err` if configuration is incomplete or invalid.
	///
	/// Environment variables:
	/// - `LOOM_SERVER_SMTP_HOST` - SMTP server hostname (required)
	/// - `LOOM_SERVER_SMTP_PORT` - SMTP server port (default: 587)
	/// - `LOOM_SERVER_SMTP_USERNAME` - Username for authentication (optional)
	/// - `LOOM_SERVER_SMTP_PASSWORD` - Password for authentication (optional)
	/// - `LOOM_SERVER_SMTP_FROM` - From address (required if host is set)
	/// - `LOOM_SERVER_SMTP_TLS` - TLS mode: true/tls, starttls, false/none (default: tls)
	pub fn from_env() -> Result<Option<Self>, AuthError> {
		let host = match std::env::var("LOOM_SERVER_SMTP_HOST") {
			Ok(h) if !h.is_empty() => h,
			Ok(_) => return Ok(None),
			Err(std::env::VarError::NotPresent) => return Ok(None),
			Err(e) => {
				return Err(AuthError::Configuration(format!(
					"Failed to read LOOM_SERVER_SMTP_HOST: {e}"
				)))
			}
		};

		let port = match std::env::var("LOOM_SERVER_SMTP_PORT") {
			Ok(p) => p
				.parse::<u16>()
				.map_err(|e| AuthError::Configuration(format!("Invalid LOOM_SERVER_SMTP_PORT: {e}")))?,
			Err(std::env::VarError::NotPresent) => 587,
			Err(e) => {
				return Err(AuthError::Configuration(format!(
					"Failed to read LOOM_SERVER_SMTP_PORT: {e}"
				)))
			}
		};

		let from_address = std::env::var("LOOM_SERVER_SMTP_FROM").map_err(|e| {
			AuthError::Configuration(format!(
				"LOOM_SERVER_SMTP_FROM is required when LOOM_SERVER_SMTP_HOST is set: {e}"
			))
		})?;

		if from_address.is_empty() {
			return Err(AuthError::Configuration(
				"LOOM_SERVER_SMTP_FROM cannot be empty".to_string(),
			));
		}

		let username = std::env::var("LOOM_SERVER_SMTP_USERNAME")
			.ok()
			.filter(|s| !s.is_empty());

		let password = std::env::var("LOOM_SERVER_SMTP_PASSWORD")
			.ok()
			.filter(|s| !s.is_empty())
			.map(SecretString::new);

		let tls_mode = match std::env::var("LOOM_SERVER_SMTP_TLS") {
			Ok(v) => TlsMode::from_env_value(&v)?,
			Err(std::env::VarError::NotPresent) => TlsMode::Tls,
			Err(e) => {
				return Err(AuthError::Configuration(format!(
					"Failed to read LOOM_SERVER_SMTP_TLS: {e}"
				)))
			}
		};

		Ok(Some(Self {
			host,
			port,
			username,
			password,
			from_address,
			tls_mode,
		}))
	}

	/// Check if authentication credentials are configured.
	pub fn has_auth(&self) -> bool {
		self.username.is_some() && self.password.is_some()
	}
}

/// Email templates for authentication-related emails.
#[derive(Debug, Clone)]
pub enum EmailTemplate {
	/// Magic link login email.
	MagicLink {
		/// Email address receiving the magic link.
		email: String,
		/// Magic link token.
		token: String,
		/// Minutes until the link expires.
		expires_minutes: i64,
	},
	/// Security notification email.
	SecurityNotification {
		/// Type of security event.
		event: String,
		/// Details about the event.
		details: String,
	},
	/// Organization invitation email.
	OrgInvitation {
		/// Name of the organization.
		org_name: String,
		/// Name of the person who sent the invitation.
		inviter_name: String,
		/// Invitation token.
		token: String,
	},
	/// Account deletion warning email.
	AccountDeletionWarning {
		/// Days remaining before permanent deletion.
		days_remaining: i64,
	},
}

/// Render an email template to subject and body.
///
/// Returns a tuple of (subject, body) strings.
///
/// # Arguments
/// * `template` - The email template with variable data
/// * `locale` - The locale code (e.g., "en", "es", "ar")
pub fn render_email(template: &EmailTemplate, locale: &str) -> (String, String) {
	use loom_common_i18n::{t, t_fmt};

	match template {
		EmailTemplate::MagicLink {
			email: _,
			token,
			expires_minutes,
		} => {
			let subject = t(locale, "server.email.magic_link.subject");
			let body = format!(
				"{}\n\nhttps://loom.example/auth/magic-link/verify?token={}\n\n{}\n\n{}",
				t(locale, "server.email.magic_link.body"),
				token,
				t_fmt(
					locale,
					"server.email.magic_link.expires",
					&[("minutes", &expires_minutes.to_string())]
				),
				t(locale, "server.email.magic_link.ignore"),
			);
			(subject, body)
		}
		EmailTemplate::SecurityNotification { event, details } => {
			let subject = t_fmt(
				locale,
				"server.email.security_alert.subject",
				&[("event", event)],
			);
			let body = format!(
				"{}\n\n{}\n\n{}\n{}\n\n{}",
				t(locale, "server.email.security_alert.body"),
				t_fmt(
					locale,
					"server.email.security_alert.event",
					&[("event", event)]
				),
				t(locale, "server.email.security_alert.details"),
				details,
				t(locale, "server.email.security_alert.action"),
			);
			(subject, body)
		}
		EmailTemplate::OrgInvitation {
			org_name,
			inviter_name,
			token,
		} => {
			let subject = t_fmt(
				locale,
				"server.email.invitation.subject",
				&[("org_name", org_name)],
			);
			let body = format!(
				"{}\n\n{}\n\nhttps://loom.example/api/invitations/{}/accept\n\n{}",
				t_fmt(
					locale,
					"server.email.invitation.body",
					&[("inviter_name", inviter_name), ("org_name", org_name),]
				),
				t(locale, "server.email.invitation.action"),
				token,
				t(locale, "server.email.invitation.no_expiry"),
			);
			(subject, body)
		}
		EmailTemplate::AccountDeletionWarning { days_remaining } => {
			let subject = t(locale, "server.email.deletion_warning.subject");
			let body = format!(
				"{}\n\n{}\n\n{}",
				t_fmt(
					locale,
					"server.email.deletion_warning.body",
					&[("days", &days_remaining.to_string())]
				),
				t(locale, "server.email.deletion_warning.restore"),
				t(locale, "server.email.deletion_warning.permanent"),
			);
			(subject, body)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;
	use std::sync::Mutex;

	static ENV_MUTEX: Mutex<()> = Mutex::new(());

	fn clear_smtp_env() {
		env::remove_var("LOOM_SERVER_SMTP_HOST");
		env::remove_var("LOOM_SERVER_SMTP_PORT");
		env::remove_var("LOOM_SERVER_SMTP_USERNAME");
		env::remove_var("LOOM_SERVER_SMTP_PASSWORD");
		env::remove_var("LOOM_SERVER_SMTP_FROM");
		env::remove_var("LOOM_SERVER_SMTP_TLS");
	}

	mod tls_mode {
		use super::*;

		#[test]
		fn parses_true_as_tls() {
			assert_eq!(TlsMode::from_env_value("true").unwrap(), TlsMode::Tls);
			assert_eq!(TlsMode::from_env_value("TRUE").unwrap(), TlsMode::Tls);
		}

		#[test]
		fn parses_tls_as_tls() {
			assert_eq!(TlsMode::from_env_value("tls").unwrap(), TlsMode::Tls);
			assert_eq!(TlsMode::from_env_value("TLS").unwrap(), TlsMode::Tls);
		}

		#[test]
		fn parses_starttls() {
			assert_eq!(
				TlsMode::from_env_value("starttls").unwrap(),
				TlsMode::StartTls
			);
			assert_eq!(
				TlsMode::from_env_value("STARTTLS").unwrap(),
				TlsMode::StartTls
			);
		}

		#[test]
		fn parses_false_as_none() {
			assert_eq!(TlsMode::from_env_value("false").unwrap(), TlsMode::None);
			assert_eq!(TlsMode::from_env_value("FALSE").unwrap(), TlsMode::None);
		}

		#[test]
		fn parses_none_as_none() {
			assert_eq!(TlsMode::from_env_value("none").unwrap(), TlsMode::None);
			assert_eq!(TlsMode::from_env_value("NONE").unwrap(), TlsMode::None);
		}

		#[test]
		fn rejects_invalid_value() {
			let result = TlsMode::from_env_value("invalid");
			assert!(result.is_err());
		}

		#[test]
		fn default_is_tls() {
			assert_eq!(TlsMode::default(), TlsMode::Tls);
		}
	}

	mod smtp_config {
		use super::*;

		#[test]
		fn returns_none_when_host_not_set() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			let config = SmtpConfig::from_env().unwrap();
			assert!(config.is_none());
		}

		#[test]
		fn returns_none_when_host_is_empty() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "");
			let config = SmtpConfig::from_env().unwrap();
			assert!(config.is_none());
			clear_smtp_env();
		}

		#[test]
		fn requires_from_address_when_host_set() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			let result = SmtpConfig::from_env();
			assert!(result.is_err());
			clear_smtp_env();
		}

		#[test]
		fn rejects_empty_from_address() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_FROM", "");
			let result = SmtpConfig::from_env();
			assert!(result.is_err());
			clear_smtp_env();
		}

		#[test]
		fn parses_minimal_config() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");

			let config = SmtpConfig::from_env().unwrap().unwrap();
			assert_eq!(config.host, "smtp.example.com");
			assert_eq!(config.port, 587);
			assert_eq!(config.from_address, "noreply@example.com");
			assert_eq!(config.tls_mode, TlsMode::Tls);
			assert!(config.username.is_none());
			assert!(config.password.is_none());
			assert!(!config.has_auth());

			clear_smtp_env();
		}

		#[test]
		fn parses_full_config() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_PORT", "465");
			env::set_var("LOOM_SERVER_SMTP_USERNAME", "user@example.com");
			env::set_var("LOOM_SERVER_SMTP_PASSWORD", "secret123");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");
			env::set_var("LOOM_SERVER_SMTP_TLS", "starttls");

			let config = SmtpConfig::from_env().unwrap().unwrap();
			assert_eq!(config.host, "smtp.example.com");
			assert_eq!(config.port, 465);
			assert_eq!(config.username, Some("user@example.com".to_string()));
			assert!(config.password.is_some());
			assert_eq!(config.from_address, "noreply@example.com");
			assert_eq!(config.tls_mode, TlsMode::StartTls);
			assert!(config.has_auth());

			clear_smtp_env();
		}

		#[test]
		fn rejects_invalid_port() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_PORT", "not_a_number");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");

			let result = SmtpConfig::from_env();
			assert!(result.is_err());

			clear_smtp_env();
		}

		#[test]
		fn rejects_invalid_tls_mode() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");
			env::set_var("LOOM_SERVER_SMTP_TLS", "invalid");

			let result = SmtpConfig::from_env();
			assert!(result.is_err());

			clear_smtp_env();
		}

		#[test]
		fn ignores_empty_username() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");
			env::set_var("LOOM_SERVER_SMTP_USERNAME", "");

			let config = SmtpConfig::from_env().unwrap().unwrap();
			assert!(config.username.is_none());

			clear_smtp_env();
		}

		#[test]
		fn ignores_empty_password() {
			let _guard = ENV_MUTEX.lock().unwrap();
			clear_smtp_env();
			env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
			env::set_var("LOOM_SERVER_SMTP_FROM", "noreply@example.com");
			env::set_var("LOOM_SERVER_SMTP_PASSWORD", "");

			let config = SmtpConfig::from_env().unwrap().unwrap();
			assert!(config.password.is_none());

			clear_smtp_env();
		}
	}

	mod email_templates {
		use super::*;

		#[test]
		fn renders_magic_link() {
			let template = EmailTemplate::MagicLink {
				email: "user@example.com".to_string(),
				token: "abc123".to_string(),
				expires_minutes: 10,
			};

			let (subject, body) = render_email(&template, "en");

			assert_eq!(subject, "Sign in to Loom");
			assert!(body.contains("abc123"));
			assert!(body.contains("10"));
			assert!(body.contains("magic-link/verify"));
		}

		#[test]
		fn renders_security_notification() {
			let template = EmailTemplate::SecurityNotification {
				event: "New login from unknown device".to_string(),
				details: "IP: 192.168.1.1\nLocation: San Francisco, CA".to_string(),
			};

			let (subject, body) = render_email(&template, "en");

			assert!(subject.contains("Security Alert"));
			assert!(subject.contains("New login from unknown device"));
			assert!(body.contains("New login from unknown device"));
			assert!(body.contains("192.168.1.1"));
			assert!(body.contains("San Francisco"));
		}

		#[test]
		fn renders_org_invitation() {
			let template = EmailTemplate::OrgInvitation {
				org_name: "Acme Corp".to_string(),
				inviter_name: "Alice".to_string(),
				token: "invite-token-xyz".to_string(),
			};

			let (subject, body) = render_email(&template, "en");

			assert!(subject.contains("Acme Corp"));
			assert!(subject.contains("invited"));
			assert!(body.contains("Alice"));
			assert!(body.contains("Acme Corp"));
			assert!(body.contains("invite-token-xyz"));
			assert!(body.contains("/accept"));
		}

		#[test]
		fn renders_account_deletion_warning() {
			let template = EmailTemplate::AccountDeletionWarning { days_remaining: 7 };

			let (subject, body) = render_email(&template, "en");

			assert!(subject.contains("deletion"));
			assert!(body.contains("7"));
			assert!(body.contains("restore"));
			assert!(body.contains("cannot be recovered"));
		}

		#[test]
		fn renders_in_spanish() {
			let template = EmailTemplate::MagicLink {
				email: "user@example.com".to_string(),
				token: "abc123".to_string(),
				expires_minutes: 10,
			};

			let (subject, _body) = render_email(&template, "es");
			assert_eq!(subject, "Iniciar sesión en Loom");
		}

		#[test]
		fn renders_in_arabic() {
			let template = EmailTemplate::MagicLink {
				email: "user@example.com".to_string(),
				token: "abc123".to_string(),
				expires_minutes: 10,
			};

			let (subject, _body) = render_email(&template, "ar");
			assert_eq!(subject, "تسجيل الدخول إلى Loom");
		}
	}
}
