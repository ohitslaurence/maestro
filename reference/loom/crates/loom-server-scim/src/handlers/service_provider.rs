// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::Json;
use chrono::Utc;
use loom_scim::types::{AuthenticationScheme, BulkSupported, FilterSupported, Supported};
use loom_scim::{Meta, ServiceProviderConfig};

pub async fn get_service_provider_config() -> Json<ServiceProviderConfig> {
	Json(ServiceProviderConfig {
		schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig".to_string()],
		documentation_uri: Some("https://docs.loom.dev/scim".to_string()),
		patch: Supported { supported: true },
		bulk: BulkSupported {
			supported: true,
			max_operations: 1000,
			max_payload_size: 1048576,
		},
		filter: FilterSupported {
			supported: true,
			max_results: 1000,
		},
		change_password: Supported { supported: false },
		sort: Supported { supported: false },
		etag: Supported { supported: false },
		authentication_schemes: vec![AuthenticationScheme {
			scheme_type: "oauthbearertoken".to_string(),
			name: "OAuth Bearer Token".to_string(),
			description: "Authentication scheme using OAuth Bearer Token".to_string(),
			spec_uri: Some("https://tools.ietf.org/html/rfc6750".to_string()),
			documentation_uri: None,
			primary: true,
		}],
		meta: Some(Meta {
			resource_type: "ServiceProviderConfig".to_string(),
			created: Utc::now(),
			last_modified: Utc::now(),
			location: Some("/api/scim/ServiceProviderConfig".to_string()),
			version: None,
		}),
	})
}
