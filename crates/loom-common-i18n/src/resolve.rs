// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Locale resolution logic.

use crate::locale::{is_supported, DEFAULT_LOCALE};

/// Resolve the effective locale from user preference and server default.
///
/// Resolution order (highest to lowest priority):
/// 1. User's stored locale preference (if valid)
/// 2. Server default locale (if valid)
/// 3. Fallback to English ("en")
///
/// # Arguments
///
/// * `user_locale` - User's preferred locale from database (may be None or invalid)
/// * `server_default` - Server's default locale from `LOOM_DEFAULT_LOCALE` env var
///
/// # Returns
///
/// A valid locale code that is guaranteed to be supported.
///
/// # Example
///
/// ```
/// use loom_common_i18n::resolve_locale;
///
/// // User preference takes priority
/// assert_eq!(resolve_locale(Some("es"), "en"), "es");
///
/// // Falls back to server default if user has no preference
/// assert_eq!(resolve_locale(None, "es"), "es");
///
/// // Falls back to English if both are invalid
/// assert_eq!(resolve_locale(Some("invalid"), "also_invalid"), "en");
/// ```
pub fn resolve_locale(user_locale: Option<&str>, server_default: &str) -> &'static str {
	if let Some(locale) = user_locale {
		if is_supported(locale) {
			return locale_to_static(locale);
		}
	}

	if is_supported(server_default) {
		return locale_to_static(server_default);
	}

	DEFAULT_LOCALE
}

fn locale_to_static(locale: &str) -> &'static str {
	match locale {
		"en" => "en",
		"es" => "es",
		"ar" => "ar",
		_ => DEFAULT_LOCALE,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_user_preference_takes_priority() {
		assert_eq!(resolve_locale(Some("es"), "en"), "es");
		assert_eq!(resolve_locale(Some("ar"), "en"), "ar");
	}

	#[test]
	fn test_server_default_when_no_user_preference() {
		assert_eq!(resolve_locale(None, "es"), "es");
		assert_eq!(resolve_locale(None, "ar"), "ar");
	}

	#[test]
	fn test_fallback_to_english_when_user_invalid() {
		assert_eq!(resolve_locale(Some("invalid"), "en"), "en");
		assert_eq!(resolve_locale(Some("fr"), "en"), "en");
	}

	#[test]
	fn test_fallback_to_english_when_both_invalid() {
		assert_eq!(resolve_locale(Some("invalid"), "also_invalid"), "en");
		assert_eq!(resolve_locale(None, "invalid"), "en");
	}

	#[test]
	fn test_empty_string_is_invalid() {
		assert_eq!(resolve_locale(Some(""), "en"), "en");
		assert_eq!(resolve_locale(None, ""), "en");
	}
}
