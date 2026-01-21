// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Context types for crash events (user, device, browser, OS, request).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runtime information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Runtime {
	/// "node", "browser", "rustc"
	pub name: String,
	pub version: Option<String>,
}

/// User context at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserContext {
	pub id: Option<String>,
	pub email: Option<String>,
	pub username: Option<String>,
	/// IP address (sensitive - not displayed by default)
	pub ip_address: Option<String>,
}

impl Default for UserContext {
	fn default() -> Self {
		Self {
			id: None,
			email: None,
			username: None,
			ip_address: None,
		}
	}
}

/// Device context at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeviceContext {
	pub name: Option<String>,
	pub family: Option<String>,
	pub model: Option<String>,
	pub arch: Option<String>,
}

impl Default for DeviceContext {
	fn default() -> Self {
		Self {
			name: None,
			family: None,
			model: None,
			arch: None,
		}
	}
}

/// Browser context at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BrowserContext {
	/// "Chrome", "Firefox", "Safari"
	pub name: Option<String>,
	pub version: Option<String>,
}

impl Default for BrowserContext {
	fn default() -> Self {
		Self {
			name: None,
			version: None,
		}
	}
}

/// Operating system context at crash time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OsContext {
	/// "Windows", "macOS", "Linux"
	pub name: Option<String>,
	pub version: Option<String>,
}

impl Default for OsContext {
	fn default() -> Self {
		Self {
			name: None,
			version: None,
		}
	}
}

/// HTTP request context for server-side crashes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RequestContext {
	pub url: Option<String>,
	pub method: Option<String>,
	pub headers: HashMap<String, String>,
	pub query_string: Option<String>,
}

impl Default for RequestContext {
	fn default() -> Self {
		Self {
			url: None,
			method: None,
			headers: HashMap::new(),
			query_string: None,
		}
	}
}
