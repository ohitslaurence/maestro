// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use axum::{
	middleware,
	routing::{get, post},
	Router,
};
use loom_common_secret::SecretString;
use loom_server_audit::AuditService;
use loom_server_auth::OrgId;
use loom_server_db::{TeamRepository, UserRepository};
use loom_server_provisioning::UserProvisioningService;

use crate::auth::scim_auth_middleware;
use crate::handlers::users::ScimState;
use crate::handlers::{bulk, groups, resource_types, schemas, service_provider, users};

pub fn scim_routes(
	token: Option<SecretString>,
	org_id: OrgId,
	provisioning: Arc<UserProvisioningService>,
	user_repo: Arc<UserRepository>,
	team_repo: Arc<TeamRepository>,
	audit_service: Arc<AuditService>,
) -> Router {
	let state = ScimState {
		org_id,
		provisioning,
		user_repo,
		team_repo,
		audit_service,
	};

	Router::new()
		.route(
			"/ServiceProviderConfig",
			get(service_provider::get_service_provider_config),
		)
		.route("/Schemas", get(schemas::list_schemas))
		.route("/Schemas/{id}", get(schemas::get_schema))
		.route("/ResourceTypes", get(resource_types::list_resource_types))
		.route(
			"/ResourceTypes/{id}",
			get(resource_types::get_resource_type),
		)
		.route("/Users", get(users::list_users).post(users::create_user))
		.route(
			"/Users/{id}",
			get(users::get_user)
				.put(users::replace_user)
				.patch(users::patch_user)
				.delete(users::delete_user),
		)
		.route(
			"/Groups",
			get(groups::list_groups).post(groups::create_group),
		)
		.route(
			"/Groups/{id}",
			get(groups::get_group)
				.put(groups::replace_group)
				.patch(groups::patch_group)
				.delete(groups::delete_group),
		)
		.route("/Bulk", post(bulk::bulk_operations))
		.layer(middleware::from_fn_with_state(token, scim_auth_middleware))
		.with_state(state)
}
