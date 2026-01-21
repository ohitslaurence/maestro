// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SMTP email client for Loom.
//!
//! This crate provides a simple async SMTP client for sending emails with both
//! HTML and plain text bodies. It integrates with [`loom_common_secret`] to ensure
//! passwords are never logged.
//!
//! # Features
//!
//! - Async email sending using [`lettre`]
//! - TLS support (STARTTLS)
//! - Optional authentication
//! - Multipart emails (HTML + plain text)
//! - Secure password handling via [`SecretString`]
//!
//! # Example
//!
//! ```no_run
//! use loom_server_smtp::{SmtpClient, SmtpConfig};
//! use loom_common_secret::SecretString;
//!
//! # async fn example() -> Result<(), loom_server_smtp::SmtpError> {
//! let config = SmtpConfig {
//!     host: "smtp.example.com".to_string(),
//!     port: 587,
//!     username: Some("user@example.com".to_string()),
//!     password: Some(SecretString::new("password".to_string())),
//!     from_address: "noreply@example.com".to_string(),
//!     from_name: "Loom".to_string(),
//!     use_tls: true,
//! };
//!
//! let client = SmtpClient::new(config)?;
//! client.send_email(
//!     "recipient@example.com",
//!     "Hello",
//!     "<h1>Hello World</h1>",
//!     "Hello World",
//! ).await?;
//! # Ok(())
//! # }
//! ```

use lettre::{
	message::{header::ContentType, Mailbox, MultiPart, SinglePart},
	transport::smtp::authentication::Credentials,
	AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};
use std::env;

/// Errors that can occur during SMTP operations.
///
/// Each variant captures a specific failure mode with a descriptive message.
#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
	/// Failed to connect to the SMTP server.
	#[error("connection failed: {0}")]
	Connection(String),

	/// Authentication with the SMTP server failed.
	#[error("authentication failed: {0}")]
	Auth(String),

	/// Failed to send an email message.
	#[error("send failed: {0}")]
	Send(String),

	/// Invalid configuration (missing required fields, invalid values).
	#[error("invalid configuration: {0}")]
	Config(String),

	/// Invalid email address format.
	#[error("invalid email address: {0}")]
	Address(String),
}

/// Configuration for the SMTP client.
///
/// This struct holds all the settings needed to connect to an SMTP server
/// and send emails. It can be loaded from environment variables using
/// [`SmtpConfig::from_env`] or constructed directly.
///
/// # Security
///
/// The `password` field uses [`SecretString`] to ensure passwords are:
/// - Never logged (Debug/Display are redacted)
/// - Zeroized from memory on drop
/// - Never serialized to plain text
///
/// # Example
///
/// ```
/// use loom_server_smtp::SmtpConfig;
/// use loom_common_secret::SecretString;
///
/// let config = SmtpConfig {
///     host: "smtp.example.com".to_string(),
///     port: 587,
///     username: Some("user".to_string()),
///     password: Some(SecretString::new("secret".to_string())),
///     from_address: "noreply@example.com".to_string(),
///     from_name: "My App".to_string(),
///     use_tls: true,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
	/// SMTP server hostname (e.g., "smtp.gmail.com").
	pub host: String,

	/// SMTP server port. Common values: 25 (unencrypted), 465 (TLS), 587 (STARTTLS).
	pub port: u16,

	/// Optional username for SMTP authentication.
	pub username: Option<String>,

	/// Optional password for SMTP authentication.
	/// Wrapped in [`SecretString`] to prevent accidental logging.
	pub password: Option<SecretString>,

	/// Email address to send from (e.g., "noreply@example.com").
	pub from_address: String,

	/// Display name for the sender (e.g., "Loom Notifications").
	pub from_name: String,

	/// Whether to use STARTTLS for the connection. Defaults to `true`.
	#[serde(default = "default_use_tls")]
	pub use_tls: bool,
}

fn default_use_tls() -> bool {
	true
}

impl SmtpConfig {
	/// Load SMTP configuration from environment variables.
	///
	/// # Environment Variables
	///
	/// - `LOOM_SERVER_SMTP_HOST` (required): SMTP server hostname
	/// - `LOOM_SERVER_SMTP_PORT` (optional, default: 587): SMTP server port
	/// - `LOOM_SERVER_SMTP_USERNAME` (optional): Authentication username
	/// - `LOOM_SERVER_SMTP_PASSWORD` (optional): Authentication password
	/// - `LOOM_SERVER_SMTP_FROM_ADDRESS` (required): Sender email address
	/// - `LOOM_SERVER_SMTP_FROM_NAME` (optional, default: "Loom"): Sender display name
	/// - `LOOM_SERVER_SMTP_USE_TLS` (optional, default: true): Enable STARTTLS
	///
	/// # Errors
	///
	/// Returns [`SmtpError::Config`] if required variables are missing or invalid.
	///
	/// # Example
	///
	/// ```no_run
	/// use loom_server_smtp::SmtpConfig;
	///
	/// std::env::set_var("LOOM_SERVER_SMTP_HOST", "smtp.example.com");
	/// std::env::set_var("LOOM_SERVER_SMTP_FROM_ADDRESS", "noreply@example.com");
	///
	/// let config = SmtpConfig::from_env().unwrap();
	/// ```
	pub fn from_env() -> Result<Self, SmtpError> {
		let host = env::var("LOOM_SERVER_SMTP_HOST")
			.map_err(|_| SmtpError::Config("LOOM_SERVER_SMTP_HOST is required".into()))?;

		let port = env::var("LOOM_SERVER_SMTP_PORT")
			.unwrap_or_else(|_| "587".into())
			.parse()
			.map_err(|_| SmtpError::Config("LOOM_SERVER_SMTP_PORT must be a valid port number".into()))?;

		let username = env::var("LOOM_SERVER_SMTP_USERNAME").ok();
		let password = env::var("LOOM_SERVER_SMTP_PASSWORD")
			.ok()
			.map(SecretString::new);

		let from_address = env::var("LOOM_SERVER_SMTP_FROM_ADDRESS")
			.map_err(|_| SmtpError::Config("LOOM_SERVER_SMTP_FROM_ADDRESS is required".into()))?;

		let from_name = env::var("LOOM_SERVER_SMTP_FROM_NAME").unwrap_or_else(|_| "Loom".into());

		let use_tls = env::var("LOOM_SERVER_SMTP_USE_TLS")
			.map(|v| v.to_lowercase() != "false" && v != "0")
			.unwrap_or(true);

		Ok(Self {
			host,
			port,
			username,
			password,
			from_address,
			from_name,
			use_tls,
		})
	}
}

/// Async SMTP client for sending emails.
///
/// The client is created with a configuration and can then be used to send
/// multiple emails. It maintains a connection pool internally via [`lettre`].
///
/// # Example
///
/// ```no_run
/// use loom_server_smtp::{SmtpClient, SmtpConfig};
///
/// # async fn example() -> Result<(), loom_server_smtp::SmtpError> {
/// let config = SmtpConfig::from_env()?;
/// let client = SmtpClient::new(config)?;
///
/// // Check server is reachable
/// client.check_health().await?;
///
/// // Send an email
/// client.send_email(
///     "user@example.com",
///     "Welcome!",
///     "<h1>Welcome to Loom</h1>",
///     "Welcome to Loom",
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct SmtpClient {
	transport: AsyncSmtpTransport<Tokio1Executor>,
	from_mailbox: Mailbox,
}

impl SmtpClient {
	/// Create a new SMTP client from the given configuration.
	///
	/// This validates the configuration and builds the SMTP transport.
	/// The actual connection is made lazily when sending emails.
	///
	/// # Arguments
	///
	/// * `config` - SMTP configuration including server details and credentials
	///
	/// # Errors
	///
	/// Returns [`SmtpError::Address`] if the from address is invalid.
	/// Returns [`SmtpError::Connection`] if the transport cannot be built.
	#[tracing::instrument(
        name = "smtp_client_new",
        skip(config),
        fields(host = %config.host, port = %config.port, use_tls = %config.use_tls)
    )]
	pub fn new(config: SmtpConfig) -> Result<Self, SmtpError> {
		let from_mailbox: Mailbox = format!("{} <{}>", config.from_name, config.from_address)
			.parse()
			.map_err(|e| SmtpError::Address(format!("{e}")))?;

		let builder = if config.use_tls {
			AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
				.map_err(|e| SmtpError::Connection(format!("{e}")))?
		} else {
			AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
		};

		let mut builder = builder.port(config.port);

		if let (Some(username), Some(password)) = (config.username, config.password) {
			let credentials = Credentials::new(username, password.into_inner());
			builder = builder.credentials(credentials);
		}

		let transport = builder.build();

		tracing::debug!("SMTP client initialized");

		Ok(Self {
			transport,
			from_mailbox,
		})
	}

	/// Check if the SMTP server is reachable and responding.
	///
	/// This performs an actual connection test to the SMTP server,
	/// useful for health checks and startup validation.
	///
	/// # Errors
	///
	/// Returns [`SmtpError::Connection`] if the server is unreachable.
	#[tracing::instrument(name = "smtp_check_health", skip(self))]
	pub async fn check_health(&self) -> Result<(), SmtpError> {
		tracing::debug!("checking SMTP server health");
		self
			.transport
			.test_connection()
			.await
			.map_err(|e| SmtpError::Connection(format!("{e}")))?;
		tracing::debug!("SMTP server is healthy");
		Ok(())
	}

	/// Send an email to a recipient.
	///
	/// Sends a multipart email with both HTML and plain text versions.
	/// The recipient's email client will choose which version to display.
	///
	/// # Arguments
	///
	/// * `to` - Recipient email address
	/// * `subject` - Email subject line
	/// * `body_html` - HTML version of the email body
	/// * `body_text` - Plain text version of the email body
	///
	/// # Errors
	///
	/// Returns [`SmtpError::Address`] if the recipient address is invalid.
	/// Returns [`SmtpError::Send`] if the email fails to send.
	///
	/// # Example
	///
	/// ```no_run
	/// # use loom_server_smtp::{SmtpClient, SmtpConfig};
	/// # async fn example(client: SmtpClient) -> Result<(), loom_server_smtp::SmtpError> {
	/// client.send_email(
	///     "user@example.com",
	///     "Password Reset",
	///     "<p>Click <a href='...'>here</a> to reset your password.</p>",
	///     "Visit this link to reset your password: ...",
	/// ).await?;
	/// # Ok(())
	/// # }
	/// ```
	#[tracing::instrument(
        name = "smtp_send_email",
        skip(self, body_html, body_text),
        fields(to = %to, subject = %subject)
    )]
	pub async fn send_email(
		&self,
		to: &str,
		subject: &str,
		body_html: &str,
		body_text: &str,
	) -> Result<(), SmtpError> {
		let to_mailbox: Mailbox = to.parse().map_err(|e| SmtpError::Address(format!("{e}")))?;

		tracing::debug!("building email message");

		let message = Message::builder()
			.from(self.from_mailbox.clone())
			.to(to_mailbox)
			.subject(subject)
			.multipart(
				MultiPart::alternative()
					.singlepart(
						SinglePart::builder()
							.header(ContentType::TEXT_PLAIN)
							.body(body_text.to_string()),
					)
					.singlepart(
						SinglePart::builder()
							.header(ContentType::TEXT_HTML)
							.body(body_html.to_string()),
					),
			)
			.map_err(|e| SmtpError::Send(format!("failed to build message: {e}")))?;

		tracing::debug!("sending email");

		self
			.transport
			.send(message)
			.await
			.map_err(|e| SmtpError::Send(format!("{e}")))?;

		tracing::info!("email sent successfully");

		Ok(())
	}
}

/// Validate an email address format.
///
/// Uses [`lettre`]'s [`Mailbox`] parser to check if an email address is valid.
/// This validates the format, not whether the address actually exists.
///
/// # Arguments
///
/// * `email` - Email address string to validate
///
/// # Returns
///
/// `true` if the email address is syntactically valid, `false` otherwise.
///
/// # Example
///
/// ```
/// use loom_server_smtp::is_valid_email;
///
/// assert!(is_valid_email("user@example.com"));
/// assert!(is_valid_email("User Name <user@example.com>"));
/// assert!(!is_valid_email("not-an-email"));
/// assert!(!is_valid_email(""));
/// ```
pub fn is_valid_email(email: &str) -> bool {
	email.parse::<Mailbox>().is_ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	mod email_validation {
		use super::*;

		#[test]
		fn valid_simple_email() {
			assert!(is_valid_email("user@example.com"));
		}

		#[test]
		fn valid_email_with_name() {
			assert!(is_valid_email("User Name <user@example.com>"));
		}

		#[test]
		fn valid_email_with_subdomain() {
			assert!(is_valid_email("user@mail.example.com"));
		}

		#[test]
		fn valid_email_with_plus() {
			assert!(is_valid_email("user+tag@example.com"));
		}

		#[test]
		fn invalid_empty_string() {
			assert!(!is_valid_email(""));
		}

		#[test]
		fn invalid_no_at_symbol() {
			assert!(!is_valid_email("userexample.com"));
		}

		#[test]
		fn invalid_no_domain() {
			assert!(!is_valid_email("user@"));
		}

		#[test]
		fn invalid_no_local_part() {
			assert!(!is_valid_email("@example.com"));
		}

		#[test]
		fn invalid_multiple_at_symbols() {
			assert!(!is_valid_email("user@@example.com"));
		}
	}

	mod config {
		use super::*;

		#[test]
		fn config_debug_does_not_leak_password() {
			let config = SmtpConfig {
				host: "smtp.example.com".to_string(),
				port: 587,
				username: Some("user".to_string()),
				password: Some(SecretString::new("super-secret-password".to_string())),
				from_address: "test@example.com".to_string(),
				from_name: "Test".to_string(),
				use_tls: true,
			};

			let debug = format!("{config:?}");
			assert!(!debug.contains("super-secret-password"));
			assert!(debug.contains("[REDACTED]"));
		}

		#[test]
		fn default_use_tls_is_true() {
			assert!(default_use_tls());
		}
	}

	mod property_tests {
		use super::*;
		use proptest::prelude::*;

		proptest! {
				#[test]
				fn valid_emails_are_accepted(
						local in "[a-zA-Z][a-zA-Z0-9]{0,30}",
						domain in "[a-zA-Z][a-zA-Z0-9]{0,20}",
						tld in "(com|org|net|io|dev)"
				) {
						let email = format!("{local}@{domain}.{tld}");
						prop_assert!(is_valid_email(&email), "Expected valid: {}", email);
				}

				#[test]
				fn empty_local_part_is_invalid(
						domain in "[a-zA-Z][a-zA-Z0-9-]{1,20}",
						tld in "(com|org|net)"
				) {
						let email = format!("@{domain}.{tld}");
						prop_assert!(!is_valid_email(&email));
				}

				#[test]
				fn empty_domain_is_invalid(local in "[a-zA-Z][a-zA-Z0-9]{1,20}") {
						let email = format!("{local}@");
						prop_assert!(!is_valid_email(&email));
				}

				#[test]
				fn no_at_symbol_is_invalid(s in "[a-zA-Z0-9._%+-]{1,50}") {
						prop_assume!(!s.contains('@'));
						prop_assert!(!is_valid_email(&s));
				}

				#[test]
				fn password_never_in_config_debug(password in "[a-zA-Z0-9!@#$%^&*]{8,32}") {
						prop_assume!(!password.contains("REDACTED"));
						prop_assume!(!password.contains("Secret"));

						let config = SmtpConfig {
								host: "smtp.example.com".to_string(),
								port: 587,
								username: Some("user".to_string()),
								password: Some(SecretString::new(password.clone())),
								from_address: "test@example.com".to_string(),
								from_name: "Test".to_string(),
								use_tls: true,
						};

						let debug = format!("{config:?}");
						prop_assert!(
								!debug.contains(&password),
								"Password leaked in debug output"
						);
				}

				#[test]
				fn port_parsing_rejects_invalid_strings(s in "[a-zA-Z]+") {
						let result: Result<u16, _> = s.parse();
						prop_assert!(result.is_err());
				}

				#[test]
				fn valid_port_numbers_parse(port in 1u16..=65535u16) {
						let s = port.to_string();
						let parsed: u16 = s.parse().unwrap();
						prop_assert_eq!(parsed, port);
				}
		}
	}
}
