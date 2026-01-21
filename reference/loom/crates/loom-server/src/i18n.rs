// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Internationalization helpers for API responses.
//!
//! This module provides utilities to determine the user's locale and
//! generate translated API response messages.

use loom_server_auth::middleware::CurrentUser;

/// Resolve the locale for API responses from a CurrentUser.
///
/// Priority:
/// 1. User's stored locale preference (if authenticated and supported)
/// 2. Server's default locale
///
/// # Arguments
/// * `current_user` - The authenticated user
/// * `default_locale` - Server's default locale
pub fn resolve_user_locale<'a>(current_user: &'a CurrentUser, default_locale: &'a str) -> &'a str {
	loom_common_i18n::resolve_locale(current_user.user.locale.as_deref(), default_locale)
}

// Re-export commonly used i18n functions for convenience
pub use loom_common_i18n::{is_rtl, t, t_fmt};
