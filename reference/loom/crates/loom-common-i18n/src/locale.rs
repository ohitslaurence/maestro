// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Locale metadata and direction support.

/// Text direction for a locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
	/// Left-to-right (e.g., English, Spanish)
	Ltr,
	/// Right-to-left (e.g., Arabic, Hebrew)
	Rtl,
}

impl Direction {
	/// Returns the HTML `dir` attribute value.
	pub fn as_html_dir(&self) -> &'static str {
		match self {
			Direction::Ltr => "ltr",
			Direction::Rtl => "rtl",
		}
	}

	/// Returns the CSS `text-align` value for the start of text.
	pub fn text_align_start(&self) -> &'static str {
		match self {
			Direction::Ltr => "left",
			Direction::Rtl => "right",
		}
	}
}

/// Metadata about a supported locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleInfo {
	/// ISO 639-1 language code (e.g., "en", "es", "ar")
	pub code: &'static str,
	/// English name of the language
	pub name: &'static str,
	/// Native name of the language
	pub native_name: &'static str,
	/// Text direction
	pub direction: Direction,
}

/// Default locale used as fallback.
pub const DEFAULT_LOCALE: &str = "en";

/// All supported locales.
pub const LOCALES: &[LocaleInfo] = &[
	LocaleInfo {
		code: "en",
		name: "English",
		native_name: "English",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "es",
		name: "Spanish",
		native_name: "Español",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "ar",
		name: "Arabic",
		native_name: "العربية",
		direction: Direction::Rtl,
	},
	LocaleInfo {
		code: "fr",
		name: "French",
		native_name: "Français",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "ru",
		name: "Russian",
		native_name: "Русский",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "ja",
		name: "Japanese",
		native_name: "日本語",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "ko",
		name: "Korean",
		native_name: "한국어",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "pt",
		name: "Portuguese",
		native_name: "Português",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "sv",
		name: "Swedish",
		native_name: "Svenska",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "nl",
		name: "Dutch",
		native_name: "Nederlands",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "zh-CN",
		name: "Chinese (Simplified)",
		native_name: "简体中文",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "he",
		name: "Hebrew",
		native_name: "עברית",
		direction: Direction::Rtl,
	},
	LocaleInfo {
		code: "it",
		name: "Italian",
		native_name: "Italiano",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "el",
		name: "Greek",
		native_name: "Ελληνικά",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "et",
		name: "Estonian",
		native_name: "Eesti",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "hi",
		name: "Hindi",
		native_name: "हिन्दी",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "bn",
		name: "Bengali",
		native_name: "বাংলা",
		direction: Direction::Ltr,
	},
	LocaleInfo {
		code: "id",
		name: "Indonesian",
		native_name: "Bahasa Indonesia",
		direction: Direction::Ltr,
	},
];

/// Get metadata for a locale.
///
/// Returns `None` if the locale is not supported.
pub fn locale_info(locale: &str) -> Option<&'static LocaleInfo> {
	LOCALES.iter().find(|l| l.code == locale)
}

/// Check if a locale uses right-to-left text direction.
///
/// Returns `false` for unsupported locales.
pub fn is_rtl(locale: &str) -> bool {
	locale_info(locale).is_some_and(|info| info.direction == Direction::Rtl)
}

/// Check if a locale is supported.
pub fn is_supported(locale: &str) -> bool {
	LOCALES.iter().any(|l| l.code == locale)
}

/// Get all supported locales.
pub fn available_locales() -> &'static [LocaleInfo] {
	LOCALES
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_locale_info_found() {
		let info = locale_info("en").unwrap();
		assert_eq!(info.code, "en");
		assert_eq!(info.name, "English");
		assert_eq!(info.direction, Direction::Ltr);
	}

	#[test]
	fn test_locale_info_not_found() {
		assert!(locale_info("xx").is_none());
	}

	#[test]
	fn test_is_rtl() {
		assert!(!is_rtl("en"));
		assert!(!is_rtl("es"));
		assert!(is_rtl("ar"));
		assert!(is_rtl("he"));
		assert!(!is_rtl("it"));
		assert!(!is_rtl("el"));
		assert!(!is_rtl("unknown"));
	}

	#[test]
	fn test_is_supported() {
		assert!(is_supported("en"));
		assert!(is_supported("es"));
		assert!(is_supported("ar"));
		assert!(is_supported("fr"));
		assert!(is_supported("ru"));
		assert!(is_supported("ja"));
		assert!(is_supported("ko"));
		assert!(is_supported("pt"));
		assert!(is_supported("sv"));
		assert!(is_supported("nl"));
		assert!(is_supported("zh-CN"));
		assert!(is_supported("he"));
		assert!(is_supported("it"));
		assert!(is_supported("el"));
		assert!(is_supported("et"));
		assert!(is_supported("hi"));
		assert!(is_supported("bn"));
		assert!(is_supported("id"));
		assert!(!is_supported("de"));
		assert!(!is_supported(""));
	}

	#[test]
	fn test_available_locales() {
		let locales = available_locales();
		assert_eq!(locales.len(), 18);
		assert!(locales.iter().any(|l| l.code == "en"));
		assert!(locales.iter().any(|l| l.code == "es"));
		assert!(locales.iter().any(|l| l.code == "ar"));
		assert!(locales.iter().any(|l| l.code == "fr"));
		assert!(locales.iter().any(|l| l.code == "ru"));
		assert!(locales.iter().any(|l| l.code == "ja"));
		assert!(locales.iter().any(|l| l.code == "ko"));
		assert!(locales.iter().any(|l| l.code == "pt"));
		assert!(locales.iter().any(|l| l.code == "sv"));
		assert!(locales.iter().any(|l| l.code == "nl"));
		assert!(locales.iter().any(|l| l.code == "zh-CN"));
		assert!(locales.iter().any(|l| l.code == "he"));
		assert!(locales.iter().any(|l| l.code == "it"));
		assert!(locales.iter().any(|l| l.code == "el"));
		assert!(locales.iter().any(|l| l.code == "et"));
		assert!(locales.iter().any(|l| l.code == "hi"));
		assert!(locales.iter().any(|l| l.code == "bn"));
		assert!(locales.iter().any(|l| l.code == "id"));
	}

	#[test]
	fn test_direction_html_dir() {
		assert_eq!(Direction::Ltr.as_html_dir(), "ltr");
		assert_eq!(Direction::Rtl.as_html_dir(), "rtl");
	}

	#[test]
	fn test_direction_text_align() {
		assert_eq!(Direction::Ltr.text_align_start(), "left");
		assert_eq!(Direction::Rtl.text_align_start(), "right");
	}

	#[test]
	fn test_arabic_has_native_name() {
		let info = locale_info("ar").unwrap();
		assert_eq!(info.native_name, "العربية");
	}
}
