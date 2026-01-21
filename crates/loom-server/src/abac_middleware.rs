// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! ABAC (Attribute-Based Access Control) route-level middleware for coarse permission checks.
//!
//! This module provides Tower layers for route-level authorization and helpers for
//! fine-grained handler-level authorization.
//!
//! # Architecture
//!
//! Authorization in Loom uses a two-tier approach:
//!
//! 1. **Route-level** (this module's layers): Coarse checks applied to entire routes
//! 2. **Handler-level** (helper functions): Fine-grained checks within handlers
//!
//! # Route-Level Authorization
//!
//! - [`RequireCapability`] - Checks if user can perform an action on a resource type
//! - [`RequireRole`] - Checks if user has specific global roles
//!
//! # Handler-Level Authorization
//!
//! - [`build_subject_attrs`] - Build SubjectAttrs from CurrentUser + membership data
//! - [`org_resource`], [`team_resource`], [`thread_resource`], [`user_resource`] - Resource builders
//! - [`authorize!`] macro - For inline authorization checks in handlers
//! - [`check_authorization`] - Function for authorization checks
//!
//! # Security Properties
//!
//! - All authorization decisions are logged with user_id and action (never tokens)
//! - Unauthenticated requests are rejected with 401 Unauthorized
//! - Unauthorized requests are rejected with 403 Forbidden
//! - Error responses do not leak permission details beyond "Insufficient permissions"
//!
//! # Example
//!
//! ```ignore
//! use loom_server::abac_middleware::{RequireCapability, RequireRole};
//! use loom_server_auth::{Action, ResourceType};
//!
//! // Route-level: require admin role
//! Router::new()
//!     .route("/admin", get(admin_dashboard))
//!     .route_layer(RequireRole::admin());
//!
//! // Route-level: require write capability on organizations
//! Router::new()
//!     .route("/orgs", post(create_org))
//!     .route_layer(RequireCapability::new(Action::Write, ResourceType::Organization));
//! ```

use axum::{
	body::Body,
	http::{Request, StatusCode},
	response::{IntoResponse, Response},
	Json,
};
use loom_server_auth::{
	abac::{OrgMembershipAttr, TeamMembershipAttr},
	middleware::{AuthContext, CurrentUser},
	Action, GlobalRole, OrgId, ResourceAttrs, ResourceType, SubjectAttrs, TeamId, UserId, Visibility,
};
use pin_project_lite::pin_project;
use serde::Serialize;
use std::{
	future::Future,
	pin::Pin,
	task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::instrument;

use crate::db::{OrgRepository, TeamRepository};
use crate::error::ErrorResponse;

// =============================================================================
// Route-Level Authorization Layers
// =============================================================================

/// Route layer that checks if user has a capability (action + resource type).
///
/// This is a coarse-grained check that verifies the user has the general capability
/// to perform an action on a resource type, without checking specific resource ownership
/// or organization membership.
///
/// # Security
///
/// - Rejects unauthenticated requests with 401
/// - Rejects unauthorized requests with 403
/// - Logs all authorization decisions with user_id and action
///
/// # Example
///
/// ```ignore
/// Router::new()
///     .route("/orgs", post(create_org))
///     .route_layer(RequireCapability::new(Action::Write, ResourceType::Organization))
/// ```
#[derive(Clone)]
pub struct RequireCapability {
	action: Action,
	resource_type: ResourceType,
}

impl RequireCapability {
	/// Create a new capability requirement.
	///
	/// # Arguments
	///
	/// * `action` - The action the user must be able to perform (Read, Write, Delete, Admin)
	/// * `resource_type` - The type of resource the action applies to
	pub fn new(action: Action, resource_type: ResourceType) -> Self {
		Self {
			action,
			resource_type,
		}
	}
}

impl<S> Layer<S> for RequireCapability {
	type Service = RequireCapabilityService<S>;

	fn layer(&self, inner: S) -> Self::Service {
		RequireCapabilityService {
			inner,
			action: self.action,
			resource_type: self.resource_type,
		}
	}
}

/// Service wrapper for [`RequireCapability`] layer.
#[derive(Clone)]
pub struct RequireCapabilityService<S> {
	inner: S,
	action: Action,
	resource_type: ResourceType,
}

impl<S> Service<Request<Body>> for RequireCapabilityService<S>
where
	S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
	S::Future: Send,
{
	type Response = Response;
	type Error = S::Error;
	type Future = RequireCapabilityFuture<S::Future>;

	fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		self.inner.poll_ready(cx)
	}

	fn call(&mut self, req: Request<Body>) -> Self::Future {
		let auth_ctx = req
			.extensions()
			.get::<AuthContext>()
			.cloned()
			.unwrap_or_else(AuthContext::unauthenticated);

		let Some(current_user) = auth_ctx.current_user else {
			tracing::debug!(
				action = ?self.action,
				resource_type = ?self.resource_type,
				"ABAC denied: not authenticated"
			);
			return RequireCapabilityFuture::Rejected {
				resp: Some(unauthorized_response()),
			};
		};

		let subject = build_subject_attrs_sync(&current_user);
		let resource = ResourceAttrs {
			resource_type: self.resource_type,
			owner_user_id: None,
			org_id: None,
			team_id: None,
			visibility: loom_server_auth::Visibility::Private,
			is_shared_with_support: false,
		};

		if !loom_server_auth::is_allowed(&subject, self.action, &resource) {
			tracing::info!(
				user_id = %current_user.user.id,
				action = ?self.action,
				resource_type = ?self.resource_type,
				"ABAC denied: capability check failed"
			);

			return RequireCapabilityFuture::Rejected {
				resp: Some(forbidden_response()),
			};
		}

		tracing::debug!(
			user_id = %current_user.user.id,
			action = ?self.action,
			resource_type = ?self.resource_type,
			"ABAC allowed: capability check passed"
		);

		RequireCapabilityFuture::Inner {
			fut: self.inner.call(req),
		}
	}
}

pin_project! {
	/// Future for [`RequireCapabilityService`].
	#[project = RequireCapabilityFutureProj]
	pub enum RequireCapabilityFuture<F> {
		Inner { #[pin] fut: F },
		Rejected { resp: Option<Response> },
	}
}

impl<F, E> Future for RequireCapabilityFuture<F>
where
	F: Future<Output = Result<Response, E>>,
{
	type Output = Result<Response, E>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match self.project() {
			RequireCapabilityFutureProj::Inner { fut } => fut.poll(cx),
			RequireCapabilityFutureProj::Rejected { resp } => {
				Poll::Ready(Ok(resp.take().expect("polled after completion")))
			}
		}
	}
}

/// Route layer for global role checks.
///
/// Checks if the authenticated user has one of the required global roles
/// (SystemAdmin, Support, Auditor).
///
/// # Role Combinations
///
/// - [`RequireRole::admin()`] - Requires SystemAdmin role
/// - [`RequireRole::support()`] - Requires Support role
/// - [`RequireRole::auditor()`] - Requires Auditor role
/// - [`RequireRole::admin_or_support()`] - Requires either SystemAdmin or Support
/// - [`RequireRole::admin_or_auditor()`] - Requires either SystemAdmin or Auditor
///
/// # Security
///
/// - Rejects unauthenticated requests with 401
/// - Rejects unauthorized requests with 403
/// - Logs all authorization decisions with user_id and required roles
///
/// # Example
///
/// ```ignore
/// Router::new()
///     .route("/admin", get(admin_dashboard))
///     .route_layer(RequireRole::admin())
/// ```
#[derive(Clone)]
pub struct RequireRole {
	require_admin: bool,
	require_support: bool,
	require_auditor: bool,
}

impl RequireRole {
	/// Create a new role requirement with no required roles.
	///
	/// This allows any authenticated user through.
	pub fn new() -> Self {
		Self {
			require_admin: false,
			require_support: false,
			require_auditor: false,
		}
	}

	/// Require the SystemAdmin global role.
	pub fn admin() -> Self {
		Self {
			require_admin: true,
			require_support: false,
			require_auditor: false,
		}
	}

	/// Require the Support global role.
	pub fn support() -> Self {
		Self {
			require_admin: false,
			require_support: true,
			require_auditor: false,
		}
	}

	/// Require the Auditor global role.
	pub fn auditor() -> Self {
		Self {
			require_admin: false,
			require_support: false,
			require_auditor: true,
		}
	}

	/// Require either SystemAdmin or Support role.
	pub fn admin_or_support() -> Self {
		Self {
			require_admin: true,
			require_support: true,
			require_auditor: false,
		}
	}

	/// Require either SystemAdmin or Auditor role.
	pub fn admin_or_auditor() -> Self {
		Self {
			require_admin: true,
			require_support: false,
			require_auditor: true,
		}
	}
}

impl Default for RequireRole {
	fn default() -> Self {
		Self::new()
	}
}

impl<S> Layer<S> for RequireRole {
	type Service = RequireRoleService<S>;

	fn layer(&self, inner: S) -> Self::Service {
		RequireRoleService {
			inner,
			require_admin: self.require_admin,
			require_support: self.require_support,
			require_auditor: self.require_auditor,
		}
	}
}

/// Service wrapper for [`RequireRole`] layer.
#[derive(Clone)]
pub struct RequireRoleService<S> {
	inner: S,
	require_admin: bool,
	require_support: bool,
	require_auditor: bool,
}

impl<S> Service<Request<Body>> for RequireRoleService<S>
where
	S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
	S::Future: Send,
{
	type Response = Response;
	type Error = S::Error;
	type Future = RequireRoleFuture<S::Future>;

	fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		self.inner.poll_ready(cx)
	}

	fn call(&mut self, req: Request<Body>) -> Self::Future {
		let auth_ctx = req
			.extensions()
			.get::<AuthContext>()
			.cloned()
			.unwrap_or_else(AuthContext::unauthenticated);

		let Some(current_user) = auth_ctx.current_user else {
			tracing::debug!(
				require_admin = self.require_admin,
				require_support = self.require_support,
				require_auditor = self.require_auditor,
				"Role check denied: not authenticated"
			);
			return RequireRoleFuture::Rejected {
				resp: Some(unauthorized_response()),
			};
		};

		let is_admin = current_user.user.is_system_admin;
		let is_support = current_user.user.is_support;
		let is_auditor = current_user.user.is_auditor;

		let role_satisfied = (self.require_admin && is_admin)
			|| (self.require_support && is_support)
			|| (self.require_auditor && is_auditor)
			|| (!self.require_admin && !self.require_support && !self.require_auditor);

		if !role_satisfied {
			tracing::info!(
				user_id = %current_user.user.id,
				require_admin = self.require_admin,
				require_support = self.require_support,
				require_auditor = self.require_auditor,
				is_admin = is_admin,
				is_support = is_support,
				is_auditor = is_auditor,
				"Role check denied: insufficient privileges"
			);

			return RequireRoleFuture::Rejected {
				resp: Some(forbidden_response()),
			};
		}

		tracing::debug!(
			user_id = %current_user.user.id,
			"Role check passed"
		);

		RequireRoleFuture::Inner {
			fut: self.inner.call(req),
		}
	}
}

pin_project! {
	/// Future for [`RequireRoleService`].
	#[project = RequireRoleFutureProj]
	pub enum RequireRoleFuture<F> {
		Inner { #[pin] fut: F },
		Rejected { resp: Option<Response> },
	}
}

impl<F, E> Future for RequireRoleFuture<F>
where
	F: Future<Output = Result<Response, E>>,
{
	type Output = Result<Response, E>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match self.project() {
			RequireRoleFutureProj::Inner { fut } => fut.poll(cx),
			RequireRoleFutureProj::Rejected { resp } => {
				Poll::Ready(Ok(resp.take().expect("polled after completion")))
			}
		}
	}
}

/// Build SubjectAttrs synchronously from CurrentUser (no database lookups).
///
/// This is used by route-level middleware where we cannot perform async operations.
/// It only includes global roles, not organization or team memberships.
fn build_subject_attrs_sync(
	current_user: &loom_server_auth::middleware::CurrentUser,
) -> SubjectAttrs {
	let mut subject = SubjectAttrs::new(current_user.user.id);

	if current_user.user.is_system_admin {
		subject
			.global_roles
			.push(loom_server_auth::GlobalRole::SystemAdmin);
	}
	if current_user.user.is_support {
		subject
			.global_roles
			.push(loom_server_auth::GlobalRole::Support);
	}
	if current_user.user.is_auditor {
		subject
			.global_roles
			.push(loom_server_auth::GlobalRole::Auditor);
	}

	subject
}

fn unauthorized_response() -> Response {
	(
		StatusCode::UNAUTHORIZED,
		Json(ErrorResponse {
			error: "unauthorized".to_string(),
			message: "Authentication required".to_string(),
			server_version: None,
			client_version: None,
		}),
	)
		.into_response()
}

fn forbidden_response() -> Response {
	(
		StatusCode::FORBIDDEN,
		Json(ErrorResponse {
			error: "forbidden".to_string(),
			message: "Insufficient permissions".to_string(),
			server_version: None,
			client_version: None,
		}),
	)
		.into_response()
}

// =============================================================================
// Handler-level authorization helpers
// =============================================================================

/// Error type for authorization failures that implements IntoResponse as 403 Forbidden.
///
/// This error is returned when fine-grained authorization checks fail in handlers.
/// The error message is intentionally generic to avoid leaking permission details.
#[derive(Debug, Clone, Serialize)]
pub struct AuthorizationError {
	/// Error code, always "forbidden"
	pub error: String,
	/// Human-readable message
	pub message: String,
}

impl AuthorizationError {
	/// Create a forbidden error with a custom message.
	pub fn forbidden(message: impl Into<String>) -> Self {
		Self {
			error: "forbidden".to_string(),
			message: message.into(),
		}
	}
}

impl std::fmt::Display for AuthorizationError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}: {}", self.error, self.message)
	}
}

impl std::error::Error for AuthorizationError {}

impl IntoResponse for AuthorizationError {
	fn into_response(self) -> Response {
		(StatusCode::FORBIDDEN, Json(self)).into_response()
	}
}

/// Build SubjectAttrs from a CurrentUser by fetching org and team memberships.
///
/// This function queries the database to populate the user's org and team
/// memberships for fine-grained ABAC checks.
///
/// # Security
///
/// - Only fetches memberships for the authenticated user
/// - Database errors result in empty memberships (fail-closed)
/// - User ID is logged, never tokens
///
/// # Example
///
/// ```ignore
/// let subject = build_subject_attrs(&current_user, &org_repo, &team_repo).await;
/// authorize!(subject, Action::Write, resource)?;
/// ```
#[instrument(skip(user, org_repo, team_repo), fields(user_id = %user.user.id))]
pub async fn build_subject_attrs(
	user: &CurrentUser,
	org_repo: &OrgRepository,
	team_repo: &TeamRepository,
) -> SubjectAttrs {
	let mut subject = SubjectAttrs::new(user.user.id);

	if user.user.is_system_admin {
		subject.global_roles.push(GlobalRole::SystemAdmin);
	}
	if user.user.is_support {
		subject.global_roles.push(GlobalRole::Support);
	}
	if user.user.is_auditor {
		subject.global_roles.push(GlobalRole::Auditor);
	}

	if let Ok(orgs) = org_repo.list_orgs_for_user(&user.user.id).await {
		for org in orgs {
			if let Ok(Some(membership)) = org_repo.get_membership(&org.id, &user.user.id).await {
				subject.org_memberships.push(OrgMembershipAttr {
					org_id: membership.org_id,
					role: membership.role,
				});
			}
		}
	}

	if let Ok(teams) = team_repo.get_teams_for_user(&user.user.id).await {
		for (team, role) in teams {
			subject.team_memberships.push(TeamMembershipAttr {
				team_id: team.id,
				org_id: team.org_id,
				role,
			});
		}
	}

	tracing::debug!(
		org_count = subject.org_memberships.len(),
		team_count = subject.team_memberships.len(),
		"Built subject attributes"
	);

	subject
}

/// Build ResourceAttrs for an organization resource.
///
/// # Arguments
///
/// * `org_id` - The organization ID
/// * `visibility` - The visibility level of the organization
pub fn org_resource(org_id: OrgId, visibility: Visibility) -> ResourceAttrs {
	ResourceAttrs {
		resource_type: ResourceType::Organization,
		owner_user_id: None,
		org_id: Some(org_id),
		team_id: None,
		visibility,
		is_shared_with_support: false,
	}
}

/// Build ResourceAttrs for a team resource.
///
/// Teams inherit their organization's context for authorization.
///
/// # Arguments
///
/// * `team_id` - The team ID
/// * `org_id` - The organization ID the team belongs to
pub fn team_resource(team_id: TeamId, org_id: OrgId) -> ResourceAttrs {
	ResourceAttrs {
		resource_type: ResourceType::Team,
		owner_user_id: None,
		org_id: Some(org_id),
		team_id: Some(team_id),
		visibility: Visibility::Private,
		is_shared_with_support: false,
	}
}

/// Build ResourceAttrs for a thread resource.
///
/// Threads have an owner and optional organization context.
///
/// # Arguments
///
/// * `owner_id` - The user ID who owns the thread
/// * `visibility` - The visibility level of the thread
/// * `org_id` - Optional organization ID if the thread belongs to an org
pub fn thread_resource(
	owner_id: UserId,
	visibility: Visibility,
	org_id: Option<OrgId>,
) -> ResourceAttrs {
	ResourceAttrs {
		resource_type: ResourceType::Thread,
		owner_user_id: Some(owner_id),
		org_id,
		team_id: None,
		visibility,
		is_shared_with_support: false,
	}
}

/// Build ResourceAttrs for a user resource.
///
/// User resources are private and owned by the user themselves.
///
/// # Arguments
///
/// * `user_id` - The user ID
pub fn user_resource(user_id: UserId) -> ResourceAttrs {
	ResourceAttrs {
		resource_type: ResourceType::User,
		owner_user_id: Some(user_id),
		org_id: None,
		team_id: None,
		visibility: Visibility::Private,
		is_shared_with_support: false,
	}
}

/// Check if the subject is allowed to perform the action on the resource.
///
/// Returns `Ok(())` if allowed, `Err(AuthorizationError)` if denied.
///
/// # Security
///
/// - Logs denied access attempts with user_id and action
/// - Never logs tokens or sensitive resource data
///
/// # Example
///
/// ```ignore
/// check_authorization(&subject, Action::Write, &resource)?;
/// ```
#[instrument(
	skip(subject, resource),
	fields(
		user_id = %subject.user_id,
		action = ?action,
		resource_type = ?resource.resource_type,
	)
)]
pub fn check_authorization(
	subject: &SubjectAttrs,
	action: Action,
	resource: &ResourceAttrs,
) -> Result<(), AuthorizationError> {
	if loom_server_auth::is_allowed(subject, action, resource) {
		tracing::debug!("Authorization check passed");
		Ok(())
	} else {
		tracing::info!("ABAC denied: fine-grained authorization check failed");
		Err(AuthorizationError::forbidden("Insufficient permissions"))
	}
}

/// Macro for inline authorization checks in handlers.
///
/// This is syntactic sugar for [`check_authorization`] that reads more naturally
/// in handler code.
///
/// # Example
///
/// ```ignore
/// async fn update_thread(
///     RequireAuth(user): RequireAuth,
///     State(state): State<AppState>,
///     Path(thread_id): Path<ThreadId>,
/// ) -> Result<impl IntoResponse, AuthorizationError> {
///     let subject = build_subject_attrs(&user, &state.org_repo, &state.team_repo).await;
///     let resource = thread_resource(thread.owner_id, thread.visibility, thread.org_id);
///     authorize!(subject, Action::Write, resource)?;
///     // ... rest of handler
/// }
/// ```
///
/// Returns `Ok(())` if allowed, `Err(AuthorizationError)` if denied.
#[macro_export]
macro_rules! authorize {
	($subject:expr, $action:expr, $resource:expr) => {
		$crate::abac_middleware::check_authorization($subject, $action, $resource)
	};
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::{http::Request, routing::get, Router};
	use chrono::Utc;
	use loom_server_auth::{middleware::CurrentUser, User, UserId};
	use proptest::prelude::*;
	use tower::ServiceExt;

	fn test_user(is_admin: bool, is_support: bool, is_auditor: bool) -> User {
		User {
			id: UserId::generate(),
			display_name: "Test".to_string(),
			username: None,
			primary_email: Some("test@example.com".to_string()),
			avatar_url: None,
			email_visible: true,
			is_system_admin: is_admin,
			is_support,
			is_auditor,
			created_at: Utc::now(),
			updated_at: Utc::now(),
			deleted_at: None,
			locale: None,
		}
	}

	async fn dummy_handler() -> &'static str {
		"ok"
	}

	#[tokio::test]
	async fn require_role_admin_allows_admin() {
		let app = Router::new()
			.route("/", get(dummy_handler))
			.layer(RequireRole::admin());

		let user = test_user(true, false, false);
		let current_user = CurrentUser::from_access_token(user);
		let auth_ctx = AuthContext::authenticated(current_user);

		let mut req = Request::get("/").body(Body::empty()).unwrap();
		req.extensions_mut().insert(auth_ctx);

		let resp = app.oneshot(req).await.unwrap();
		assert_eq!(resp.status(), StatusCode::OK);
	}

	#[tokio::test]
	async fn require_role_admin_denies_non_admin() {
		let app = Router::new()
			.route("/", get(dummy_handler))
			.layer(RequireRole::admin());

		let user = test_user(false, false, false);
		let current_user = CurrentUser::from_access_token(user);
		let auth_ctx = AuthContext::authenticated(current_user);

		let mut req = Request::get("/").body(Body::empty()).unwrap();
		req.extensions_mut().insert(auth_ctx);

		let resp = app.oneshot(req).await.unwrap();
		assert_eq!(resp.status(), StatusCode::FORBIDDEN);
	}

	#[tokio::test]
	async fn require_role_denies_unauthenticated() {
		let app = Router::new()
			.route("/", get(dummy_handler))
			.layer(RequireRole::admin());

		let req = Request::get("/").body(Body::empty()).unwrap();

		let resp = app.oneshot(req).await.unwrap();
		assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
	}

	#[tokio::test]
	async fn require_capability_denies_unauthenticated() {
		let app = Router::new()
			.route("/", get(dummy_handler))
			.layer(RequireCapability::new(Action::Read, ResourceType::Thread));

		let req = Request::get("/").body(Body::empty()).unwrap();

		let resp = app.oneshot(req).await.unwrap();
		assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
	}

	#[tokio::test]
	async fn require_capability_allows_admin() {
		let app = Router::new()
			.route("/", get(dummy_handler))
			.layer(RequireCapability::new(
				Action::Write,
				ResourceType::Organization,
			));

		let user = test_user(true, false, false);
		let current_user = CurrentUser::from_access_token(user);
		let auth_ctx = AuthContext::authenticated(current_user);

		let mut req = Request::get("/").body(Body::empty()).unwrap();
		req.extensions_mut().insert(auth_ctx);

		let resp = app.oneshot(req).await.unwrap();
		assert_eq!(resp.status(), StatusCode::OK);
	}

	#[test]
	fn authorization_error_display() {
		let err = AuthorizationError::forbidden("Not allowed");
		assert_eq!(err.to_string(), "forbidden: Not allowed");
	}

	mod property_tests {
		use super::*;

		proptest! {
			/// Verifies that authorization decisions are consistent for the same inputs.
			/// Given the same subject, action, and resource, the decision should always be the same.
			#[test]
			fn authorization_is_deterministic(
				is_admin in any::<bool>(),
				is_support in any::<bool>(),
				is_auditor in any::<bool>(),
			) {
				let user_id = UserId::generate();
				let mut subject = SubjectAttrs::new(user_id);
				if is_admin {
					subject.global_roles.push(GlobalRole::SystemAdmin);
				}
				if is_support {
					subject.global_roles.push(GlobalRole::Support);
				}
				if is_auditor {
					subject.global_roles.push(GlobalRole::Auditor);
				}

				let resource = ResourceAttrs {
					resource_type: ResourceType::Thread,
					owner_user_id: Some(user_id),
					org_id: None,
					team_id: None,
					visibility: Visibility::Private,
					is_shared_with_support: false,
				};

				let result1 = check_authorization(&subject, Action::Read, &resource);
				let result2 = check_authorization(&subject, Action::Read, &resource);

				prop_assert_eq!(result1.is_ok(), result2.is_ok());
			}

			/// Verifies that SystemAdmin can perform any action on any resource.
			/// This is a fundamental property of the ABAC system.
			#[test]
			fn admin_can_do_anything(
				action in prop_oneof![
					Just(Action::Read),
					Just(Action::Write),
					Just(Action::Delete),
					Just(Action::ManageOrg),
				],
				resource_type in prop_oneof![
					Just(ResourceType::Thread),
					Just(ResourceType::Organization),
					Just(ResourceType::Team),
					Just(ResourceType::User),
				],
			) {
				let user_id = UserId::generate();
				let mut subject = SubjectAttrs::new(user_id);
				subject.global_roles.push(GlobalRole::SystemAdmin);

				let resource = ResourceAttrs {
					resource_type,
					owner_user_id: None,
					org_id: None,
					team_id: None,
					visibility: Visibility::Private,
					is_shared_with_support: false,
				};

				let result = check_authorization(&subject, action, &resource);
				prop_assert!(result.is_ok(), "Admin should be allowed for {:?} on {:?}", action, resource_type);
			}

			/// Verifies that users can always read their own resources.
			/// This is a fundamental property: resource owners have read access.
			#[test]
			fn owner_can_read_own_resource(
				resource_type in prop_oneof![
					Just(ResourceType::Thread),
					Just(ResourceType::User),
				],
			) {
				let user_id = UserId::generate();
				let subject = SubjectAttrs::new(user_id);

				let resource = ResourceAttrs {
					resource_type,
					owner_user_id: Some(user_id),
					org_id: None,
					team_id: None,
					visibility: Visibility::Private,
					is_shared_with_support: false,
				};

				let result = check_authorization(&subject, Action::Read, &resource);
				prop_assert!(result.is_ok(), "Owner should be able to read own resource");
			}

			/// Verifies that role requirements are OR'd together (any matching role passes).
			#[test]
			fn role_requirements_are_ored(
				is_admin in any::<bool>(),
				is_support in any::<bool>(),
			) {
				let role = RequireRole::admin_or_support();
				let should_pass = is_admin || is_support;

				let user = test_user(is_admin, is_support, false);
				let subject = build_subject_attrs_sync(&CurrentUser::from_access_token(user));

				let is_satisfied = (role.require_admin && subject.global_roles.contains(&GlobalRole::SystemAdmin))
					|| (role.require_support && subject.global_roles.contains(&GlobalRole::Support));

				prop_assert_eq!(is_satisfied, should_pass);
			}
		}
	}
}
