// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_server_auth::OrgId;
use serde::{Deserialize, Serialize};

/// The source of a provisioning request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisioningSource {
	/// OAuth provider (GitHub, Google, Okta)
	OAuth,
	/// Magic link email authentication
	MagicLink,
	/// SCIM enterprise provisioning
	Scim,
}

impl std::fmt::Display for ProvisioningSource {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::OAuth => write!(f, "oauth"),
			Self::MagicLink => write!(f, "magic_link"),
			Self::Scim => write!(f, "scim"),
		}
	}
}

/// Request to provision a user.
///
/// This struct captures all the information needed to create or update a user
/// across different provisioning sources (OAuth, magic link, SCIM).
#[derive(Debug, Clone)]
pub struct ProvisioningRequest {
	/// User's email address (required).
	pub email: String,

	/// User's display name.
	pub display_name: String,

	/// Avatar URL (typically from OAuth provider).
	pub avatar_url: Option<String>,

	/// Preferred username (used to generate unique username).
	pub preferred_username: Option<String>,

	/// User's locale preference.
	pub locale: Option<String>,

	/// Source of this provisioning request.
	pub source: ProvisioningSource,

	/// SCIM external ID (for IdP sync).
	/// Only set when source is SCIM.
	pub scim_external_id: Option<String>,

	/// Enterprise organization to add user to.
	/// When set, user is added as a member to this org.
	/// Only used for SCIM provisioning.
	pub enterprise_org_id: Option<OrgId>,
}

impl ProvisioningRequest {
	/// Create a request for OAuth provisioning.
	pub fn oauth(
		email: impl Into<String>,
		display_name: impl Into<String>,
		avatar_url: Option<String>,
		preferred_username: Option<String>,
	) -> Self {
		Self {
			email: email.into(),
			display_name: display_name.into(),
			avatar_url,
			preferred_username,
			locale: None,
			source: ProvisioningSource::OAuth,
			scim_external_id: None,
			enterprise_org_id: None,
		}
	}

	/// Create a request for magic link provisioning.
	pub fn magic_link(email: impl Into<String>) -> Self {
		let email = email.into();
		Self {
			display_name: email.clone(),
			email,
			avatar_url: None,
			preferred_username: None,
			locale: None,
			source: ProvisioningSource::MagicLink,
			scim_external_id: None,
			enterprise_org_id: None,
		}
	}

	/// Create a request for SCIM provisioning.
	pub fn scim(
		email: impl Into<String>,
		display_name: impl Into<String>,
		scim_external_id: Option<String>,
		locale: Option<String>,
		enterprise_org_id: OrgId,
	) -> Self {
		Self {
			email: email.into(),
			display_name: display_name.into(),
			avatar_url: None,
			preferred_username: None,
			locale,
			source: ProvisioningSource::Scim,
			scim_external_id,
			enterprise_org_id: Some(enterprise_org_id),
		}
	}
}
