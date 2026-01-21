// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Email service for Loom.
//!
//! This crate provides a unified [`EmailService`] that consolidates email dispatch
//! patterns across handlers. It handles locale resolution and template rendering
//! internally, providing a simple interface for sending various email types.

use loom_common_i18n::{is_rtl, resolve_locale, t, t_fmt};
use loom_server_auth::email::EmailTemplate;
use loom_server_smtp::SmtpClient;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
	#[error("SMTP error: {0}")]
	Smtp(#[from] loom_server_smtp::SmtpError),
}

pub type Result<T> = std::result::Result<T, EmailError>;

/// Email request variants for different email types.
#[derive(Debug, Clone)]
pub enum EmailRequest {
	/// Magic link login email.
	MagicLink {
		/// The verification URL for the magic link.
		verification_url: String,
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
	/// Account deletion scheduled confirmation email.
	DeletionScheduled {
		/// Number of days in the grace period.
		grace_days: i64,
	},
	/// Account deletion warning email (sent before deletion).
	DeletionWarning {
		/// Days remaining before permanent deletion.
		days_remaining: i64,
	},
	/// Security notification email.
	SecurityNotification {
		/// Type of security event.
		event: String,
		/// Details about the event.
		details: String,
	},
}

/// Email service that handles locale resolution and template rendering.
pub struct EmailService {
	smtp_client: Arc<SmtpClient>,
	default_locale: String,
}

impl EmailService {
	/// Create a new EmailService.
	pub fn new(smtp_client: Arc<SmtpClient>, default_locale: String) -> Self {
		Self {
			smtp_client,
			default_locale,
		}
	}

	/// Send an email to the specified recipient.
	///
	/// Locale resolution: user_locale → default_locale → "en"
	#[tracing::instrument(
		name = "email_service_send",
		skip(self, request),
		fields(to = %to, request_type = ?std::mem::discriminant(&request))
	)]
	pub async fn send(
		&self,
		to: &str,
		request: EmailRequest,
		user_locale: Option<&str>,
	) -> Result<()> {
		let locale = resolve_locale(user_locale, &self.default_locale);
		let (subject, html_body, text_body) = self.render_email(&request, locale);

		self
			.smtp_client
			.send_email(to, &subject, &html_body, &text_body)
			.await?;

		tracing::info!("Email sent successfully");
		Ok(())
	}

	fn render_email(&self, request: &EmailRequest, locale: &str) -> (String, String, String) {
		match request {
			EmailRequest::MagicLink { verification_url } => {
				self.render_magic_link(verification_url, locale)
			}
			EmailRequest::OrgInvitation {
				org_name,
				inviter_name,
				token,
			} => {
				let template = EmailTemplate::OrgInvitation {
					org_name: org_name.clone(),
					inviter_name: inviter_name.clone(),
					token: token.clone(),
				};
				let (subject, body) = loom_server_auth::email::render_email(&template, locale);
				(subject, body.clone(), body)
			}
			EmailRequest::DeletionScheduled { grace_days } => {
				self.render_deletion_scheduled(*grace_days, locale)
			}
			EmailRequest::DeletionWarning { days_remaining } => {
				let template = EmailTemplate::AccountDeletionWarning {
					days_remaining: *days_remaining,
				};
				let (subject, body) = loom_server_auth::email::render_email(&template, locale);
				(subject, body.clone(), body)
			}
			EmailRequest::SecurityNotification { event, details } => {
				let template = EmailTemplate::SecurityNotification {
					event: event.clone(),
					details: details.clone(),
				};
				let (subject, body) = loom_server_auth::email::render_email(&template, locale);
				(subject, body.clone(), body)
			}
		}
	}

	fn render_magic_link(&self, verification_url: &str, locale: &str) -> (String, String, String) {
		let subject = t(locale, "server.email.magic_link.subject");
		let body = t(locale, "server.email.magic_link.body");
		let expires = t_fmt(
			locale,
			"server.email.magic_link.expires",
			&[("minutes", "10")],
		);
		let ignore = t(locale, "server.email.magic_link.ignore");
		let copy_link = t(locale, "server.email.magic_link.copy_link");

		let dir = if is_rtl(locale) { "rtl" } else { "ltr" };
		let align = if is_rtl(locale) { "right" } else { "left" };

		let html = format!(
			r#"<!DOCTYPE html>
<html lang="{locale}" dir="{dir}">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{subject}</title>
</head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; direction: {dir}; text-align: {align};">
    <h1 style="color: #1a1a1a; font-size: 24px; margin-bottom: 20px;">{subject}</h1>
    <p style="margin-bottom: 20px;">{body} {expires}</p>
    <a href="{verification_url}" style="display: inline-block; background-color: #0066cc; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; font-weight: 500;">{subject}</a>
    <p style="margin-top: 20px; color: #666; font-size: 14px;">{ignore}</p>
    <p style="margin-top: 20px; color: #666; font-size: 12px;">{copy_link} <span dir="ltr">{verification_url}</span></p>
</body>
</html>"#,
		);

		let text = format!("{subject}\n\n{body} {expires}\n\n{verification_url}\n\n{ignore}");

		(subject, html, text)
	}

	fn render_deletion_scheduled(&self, grace_days: i64, locale: &str) -> (String, String, String) {
		let days = grace_days.to_string();

		let subject = t(locale, "server.email.deletion_scheduled.subject");
		let body = t(locale, "server.email.deletion_scheduled.body");
		let grace = t_fmt(
			locale,
			"server.email.deletion_scheduled.grace",
			&[("days", &days)],
		);
		let permanent = t(locale, "server.email.deletion_scheduled.permanent");

		let body_text = format!("{body}\n\n{grace}\n\n{permanent}");

		let dir = if is_rtl(locale) { "rtl" } else { "ltr" };
		let body_html =
			format!("<div dir=\"{dir}\"><p>{body}</p><p>{grace}</p><p>{permanent}</p></div>");

		(subject, body_html, body_text)
	}
}
