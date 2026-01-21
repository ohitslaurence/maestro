// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::OnceLock;

static DETECTED_LOCALE: OnceLock<String> = OnceLock::new();

/// Get the user's locale, detected from the operating system.
/// Falls back to "en" if detection fails or the locale is unsupported.
pub fn get_locale() -> &'static str {
	DETECTED_LOCALE.get_or_init(|| detect_locale().unwrap_or_else(|| "en".to_string()))
}

fn detect_locale() -> Option<String> {
	let system_locale = sys_locale::get_locale()?;

	let lang_code = system_locale.split(['_', '-']).next()?.to_lowercase();

	if loom_common_i18n::is_supported(&lang_code) {
		Some(lang_code)
	} else {
		Some("en".to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_get_locale_returns_valid() {
		let locale = get_locale();
		assert!(loom_common_i18n::is_supported(locale));
	}

	#[test]
	fn test_detect_locale_extracts_language_code() {
		assert!(detect_locale().is_some());
	}
}
