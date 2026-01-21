// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use chrono::Utc;
use loom_server_auth::{OrgRole, User, UserId};
use loom_server_db::{OrgRepository, UserRepository};

use crate::error::ProvisioningError;
use crate::request::{ProvisioningRequest, ProvisioningSource};

/// Result type for provisioning operations.
pub type Result<T> = std::result::Result<T, ProvisioningError>;

/// Service for provisioning users with all required resources.
///
/// This service provides a single code path for user creation across all
/// authentication methods (OAuth, magic link, SCIM). When a user is
/// provisioned, they automatically get a personal organization.
#[derive(Clone)]
pub struct UserProvisioningService {
	user_repo: Arc<UserRepository>,
	org_repo: Arc<OrgRepository>,
	signups_disabled: bool,
}

impl UserProvisioningService {
	/// Create a new provisioning service.
	pub fn new(
		user_repo: Arc<UserRepository>,
		org_repo: Arc<OrgRepository>,
		signups_disabled: bool,
	) -> Self {
		Self {
			user_repo,
			org_repo,
			signups_disabled,
		}
	}

	/// Provision a user based on the request.
	///
	/// This is the main entry point for all user provisioning:
	/// - If user exists, updates their profile and returns them
	/// - If user is new, creates them with a personal organization
	/// - For SCIM, also handles enterprise org membership
	///
	/// Returns `SignupsDisabled` error if signups are disabled and user doesn't exist.
	#[tracing::instrument(skip(self), fields(email = %request.email, source = %request.source))]
	pub async fn provision_user(&self, request: ProvisioningRequest) -> Result<User> {
		let existing_user = self.user_repo.get_user_by_email(&request.email).await?;

		if self.signups_disabled && existing_user.is_none() {
			tracing::warn!(email = %request.email, source = %request.source, "Signup rejected: signups are disabled");
			return Err(ProvisioningError::SignupsDisabled);
		}

		let user = if let Some(mut user) = existing_user {
			self.update_existing_user(&mut user, &request).await?;
			user
		} else {
			self.create_new_user(&request).await?
		};

		// Ensure personal org exists
		if let Err(e) = self.org_repo.ensure_personal_org(&user.id).await {
			tracing::error!(error = %e, user_id = %user.id, "Failed to ensure personal org");
		}

		// Handle enterprise org membership for SCIM
		if let Some(enterprise_org_id) = &request.enterprise_org_id {
			self
				.ensure_enterprise_membership(&user.id, enterprise_org_id, &request)
				.await?;
		}

		Ok(user)
	}

	/// Update an existing user with new information.
	async fn update_existing_user(
		&self,
		user: &mut User,
		request: &ProvisioningRequest,
	) -> Result<()> {
		// Generate username if missing
		if user.username.is_none() {
			let username_base = request
				.preferred_username
				.as_deref()
				.unwrap_or(&request.display_name);
			let username = self
				.user_repo
				.generate_unique_username(username_base)
				.await?;
			self.user_repo.update_username(&user.id, &username).await?;
			user.username = Some(username);
			tracing::debug!(user_id = %user.id, "set username for existing user");
		}

		// Update SCIM fields if this is SCIM provisioning
		if request.source == ProvisioningSource::Scim {
			self
				.update_scim_fields(&user.id, request.scim_external_id.as_deref())
				.await?;
		}

		tracing::debug!(user_id = %user.id, source = %request.source, "updated existing user");
		Ok(())
	}

	/// Create a new user.
	async fn create_new_user(&self, request: &ProvisioningRequest) -> Result<User> {
		// Check if this will be the first user (auto-promote to system admin)
		let user_count = self.user_repo.count_users().await?;
		let is_first_user = user_count == 0;

		let now = Utc::now();
		let username_base = request
			.preferred_username
			.as_deref()
			.unwrap_or(&request.display_name);
		let username = self
			.user_repo
			.generate_unique_username(username_base)
			.await?;

		let user = User {
			id: UserId::generate(),
			display_name: request.display_name.clone(),
			username: Some(username),
			primary_email: Some(request.email.clone()),
			avatar_url: request.avatar_url.clone(),
			email_visible: true,
			is_system_admin: is_first_user,
			is_support: false,
			is_auditor: false,
			created_at: now,
			updated_at: now,
			deleted_at: None,
			locale: request.locale.clone(),
		};

		self.user_repo.create_user(&user).await?;

		// Set SCIM fields if this is SCIM provisioning
		if request.source == ProvisioningSource::Scim {
			self
				.update_scim_fields(&user.id, request.scim_external_id.as_deref())
				.await?;
		}

		if is_first_user {
			tracing::info!(user_id = %user.id, email = %request.email, "first user created as system admin");
		} else {
			tracing::info!(user_id = %user.id, email = %request.email, source = %request.source, "created new user");
		}

		Ok(user)
	}

	/// Update SCIM-specific fields on a user.
	async fn update_scim_fields(
		&self,
		user_id: &UserId,
		scim_external_id: Option<&str>,
	) -> Result<()> {
		self
			.user_repo
			.update_scim_fields(user_id, scim_external_id, true)
			.await?;
		tracing::debug!(user_id = %user_id, "updated SCIM fields");
		Ok(())
	}

	/// Ensure user has membership in enterprise org.
	async fn ensure_enterprise_membership(
		&self,
		user_id: &UserId,
		org_id: &loom_server_auth::OrgId,
		request: &ProvisioningRequest,
	) -> Result<()> {
		// Check if membership already exists
		if let Ok(Some(_)) = self.org_repo.get_membership(org_id, user_id).await {
			tracing::debug!(user_id = %user_id, org_id = %org_id, "enterprise membership already exists");
			return Ok(());
		}

		// Create membership with provenance tracking
		let provisioned_by = request.source.to_string();
		self
			.org_repo
			.add_member_with_provenance(org_id, user_id, OrgRole::Member, Some(&provisioned_by))
			.await?;

		tracing::info!(
			user_id = %user_id,
			org_id = %org_id,
			provisioned_by = %provisioned_by,
			"added user to enterprise org"
		);
		Ok(())
	}

	/// Ensure a user has a personal organization.
	///
	/// Call this for users that may have been created before personal orgs
	/// were automatically provisioned.
	#[tracing::instrument(skip(self), fields(user_id = %user_id))]
	pub async fn ensure_personal_org(&self, user_id: &UserId) -> Result<()> {
		self.org_repo.ensure_personal_org(user_id).await?;
		Ok(())
	}

	/// Remove a user from an organization (deprovision).
	///
	/// Used by SCIM to remove a user's membership when they are deprovisioned.
	#[tracing::instrument(skip(self), fields(user_id = %user_id, org_id = %org_id))]
	pub async fn deprovision_from_org(
		&self,
		user_id: &UserId,
		org_id: &loom_server_auth::OrgId,
	) -> Result<bool> {
		let removed = self.org_repo.remove_member(org_id, user_id).await?;
		if removed {
			tracing::info!(user_id = %user_id, org_id = %org_id, "deprovisioned user from org");
		}
		Ok(removed)
	}
}
