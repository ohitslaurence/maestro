// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use axum::{
	body::Body,
	extract::State,
	http::{Request, StatusCode},
	middleware::{from_fn_with_state, Next},
	response::{IntoResponse, Response},
	routing::MethodRouter,
	Json, Router,
};
use loom_server_auth::middleware::AuthContext;
use tracing::instrument;

use crate::{api::AppState, auth_middleware::auth_layer, error::ErrorResponse};

#[instrument(
	name = "require_auth_layer",
	skip(_state, request, next),
	fields(authenticated = tracing::field::Empty)
)]
pub async fn require_auth_layer(
	State(_state): State<AppState>,
	request: Request<Body>,
	next: Next,
) -> Response {
	let (parts, body) = request.into_parts();

	let auth_ctx = parts
		.extensions
		.get::<AuthContext>()
		.cloned()
		.unwrap_or_else(AuthContext::unauthenticated);

	if auth_ctx.current_user.is_none() {
		tracing::Span::current().record("authenticated", false);
		return (
			StatusCode::UNAUTHORIZED,
			Json(ErrorResponse {
				error: "unauthorized".to_string(),
				message: "Authentication required".to_string(),
				server_version: None,
				client_version: None,
			}),
		)
			.into_response();
	}

	tracing::Span::current().record("authenticated", true);
	let request = Request::from_parts(parts, body);
	next.run(request).await
}

pub struct AuthedRouter(Router<AppState>);

impl AuthedRouter {
	pub fn new() -> Self {
		Self(Router::new())
	}

	pub fn route(self, path: &str, method_router: MethodRouter<AppState>) -> Self {
		Self(self.0.route(path, method_router))
	}

	pub fn nest(self, path: &str, router: AuthedRouter) -> Self {
		Self(self.0.nest(path, router.0))
	}

	pub fn build(self, state: AppState) -> Router<AppState> {
		self
			.0
			.layer(from_fn_with_state(state.clone(), require_auth_layer))
			.layer(from_fn_with_state(state, auth_layer))
	}
}

impl Default for AuthedRouter {
	fn default() -> Self {
		Self::new()
	}
}

pub struct PublicRouter(Router<AppState>);

impl PublicRouter {
	pub fn new() -> Self {
		Self(Router::new())
	}

	pub fn route(self, path: &str, method_router: MethodRouter<AppState>) -> Self {
		Self(self.0.route(path, method_router))
	}

	pub fn nest(self, path: &str, router: PublicRouter) -> Self {
		Self(self.0.nest(path, router.0))
	}

	pub fn build(self) -> Router<AppState> {
		self.0
	}
}

impl Default for PublicRouter {
	fn default() -> Self {
		Self::new()
	}
}

pub struct OptionalAuthRouter(Router<AppState>);

impl OptionalAuthRouter {
	pub fn new() -> Self {
		Self(Router::new())
	}

	pub fn route(self, path: &str, method_router: MethodRouter<AppState>) -> Self {
		Self(self.0.route(path, method_router))
	}

	pub fn nest(self, path: &str, router: OptionalAuthRouter) -> Self {
		Self(self.0.nest(path, router.0))
	}

	pub fn build(self, state: AppState) -> Router<AppState> {
		self.0.layer(from_fn_with_state(state, auth_layer))
	}
}

impl Default for OptionalAuthRouter {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::routing::get;

	async fn dummy_handler() -> &'static str {
		"ok"
	}

	fn assert_router_type<T>(_: &T) {}

	#[test]
	fn authed_router_build_returns_router() {
		let authed = AuthedRouter::new().route("/test", get(dummy_handler));
		assert_router_type(&authed);
	}

	#[test]
	fn public_router_build_returns_router() {
		let public = PublicRouter::new().route("/test", get(dummy_handler));
		assert_router_type(&public);
	}

	#[test]
	fn optional_auth_router_build_returns_router() {
		let optional = OptionalAuthRouter::new().route("/test", get(dummy_handler));
		assert_router_type(&optional);
	}

	#[test]
	fn router_types_are_distinct() {
		fn takes_authed(_: AuthedRouter) {}
		fn takes_public(_: PublicRouter) {}
		fn takes_optional(_: OptionalAuthRouter) {}

		let authed = AuthedRouter::new();
		let public = PublicRouter::new();
		let optional = OptionalAuthRouter::new();

		takes_authed(authed);
		takes_public(public);
		takes_optional(optional);
	}

	#[test]
	fn authed_router_nesting_preserves_type() {
		let inner = AuthedRouter::new().route("/inner", get(dummy_handler));
		let outer = AuthedRouter::new().nest("/nested", inner);
		assert_router_type(&outer);
	}

	#[test]
	fn public_router_nesting_preserves_type() {
		let inner = PublicRouter::new().route("/inner", get(dummy_handler));
		let outer = PublicRouter::new().nest("/nested", inner);
		assert_router_type(&outer);
	}

	#[test]
	fn optional_auth_router_nesting_preserves_type() {
		let inner = OptionalAuthRouter::new().route("/inner", get(dummy_handler));
		let outer = OptionalAuthRouter::new().nest("/nested", inner);
		assert_router_type(&outer);
	}
}
