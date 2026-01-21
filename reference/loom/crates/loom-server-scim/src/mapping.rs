// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::Utc;
use loom_scim::types::SCHEMA_CORE_USER;
use loom_scim::{Meta, Name, ScimEmail, ScimUser};
use loom_server_auth::UserId;

pub struct LoomUser {
	pub id: UserId,
	pub email: String,
	pub display_name: Option<String>,
	pub avatar_url: Option<String>,
	pub locale: Option<String>,
	pub scim_external_id: Option<String>,
	pub active: bool,
	pub created_at: chrono::DateTime<Utc>,
	pub updated_at: chrono::DateTime<Utc>,
}

impl From<LoomUser> for ScimUser {
	fn from(user: LoomUser) -> Self {
		let primary_email = ScimEmail {
			value: user.email.clone(),
			email_type: Some("work".to_string()),
			primary: true,
		};

		ScimUser {
			schemas: vec![SCHEMA_CORE_USER.to_string()],
			id: Some(user.id.to_string()),
			external_id: user.scim_external_id,
			user_name: user.email.clone(),
			name: user.display_name.as_ref().map(|dn| Name {
				formatted: Some(dn.clone()),
				family_name: None,
				given_name: None,
				middle_name: None,
				honorific_prefix: None,
				honorific_suffix: None,
			}),
			display_name: user.display_name,
			nick_name: None,
			profile_url: None,
			title: None,
			user_type: None,
			preferred_language: None,
			locale: user.locale,
			timezone: None,
			active: user.active,
			emails: vec![primary_email],
			phone_numbers: vec![],
			meta: Some(Meta {
				resource_type: "User".to_string(),
				created: user.created_at,
				last_modified: user.updated_at,
				location: None,
				version: None,
			}),
		}
	}
}

pub fn scim_user_to_display_name(user: &ScimUser) -> Option<String> {
	user
		.display_name
		.clone()
		.or_else(|| user.name.as_ref().and_then(|n| n.formatted.clone()))
		.or_else(|| {
			let name = user.name.as_ref()?;
			let given = name.given_name.as_ref()?;
			let family = name.family_name.as_ref()?;
			Some(format!("{} {}", given, family))
		})
}

pub fn scim_user_to_email(user: &ScimUser) -> Option<String> {
	user
		.emails
		.iter()
		.find(|e| e.primary)
		.or_else(|| user.emails.first())
		.map(|e| e.value.clone())
		.or_else(|| Some(user.user_name.clone()))
}
