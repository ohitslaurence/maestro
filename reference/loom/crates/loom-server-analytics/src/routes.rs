// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Route configuration for analytics API.
//!
//! This module provides route builders for the analytics API endpoints.
//! Routes are split into two categories:
//!
//! 1. **SDK routes** (`analytics_sdk_routes`) - Used by SDKs with API key auth
//!    - Event capture (Write key)
//!    - Identity operations (Write key)
//!    - Query operations (ReadWrite key)
//!
//! 2. **Management routes** - Used by the web UI with user auth
//!    - API key CRUD operations (handled in loom-server)
//!
//! # Integration
//!
//! These routes are designed to be mounted in loom-server:
//!
//! ```ignore
//! // In loom-server, create state and mount routes
//! let analytics_state = Arc::new(AnalyticsState::new(repo));
//! let app = Router::new()
//!     .nest("/api/analytics", analytics_sdk_routes(analytics_state));
//! ```

pub use crate::handlers::api_keys::{
	api_key_to_response, api_key_type_from_api, api_key_type_to_api, create_api_key_impl,
	generate_api_key, list_api_keys_impl, revoke_api_key_impl, UserAuthContext,
};
pub use crate::handlers::capture::{batch_capture_impl, capture_event_impl, AnalyticsState};
pub use crate::handlers::events::{count_events_impl, export_events_impl, list_events_impl};
pub use crate::handlers::identify::{alias_impl, identify_impl, set_properties_impl};
pub use crate::handlers::persons::{
	get_person_by_distinct_id_impl, get_person_impl, list_persons_impl, person_to_response,
};
