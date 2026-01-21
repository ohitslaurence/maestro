// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Gettext catalog loading and translation functions.

use std::collections::HashMap;

use gettext::Catalog;
use once_cell::sync::Lazy;

use crate::locale::DEFAULT_LOCALE;

const EN_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en.mo"));
const ES_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/es.mo"));
const AR_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ar.mo"));
const FR_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/fr.mo"));
const RU_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ru.mo"));
const JA_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ja.mo"));
const KO_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ko.mo"));
const PT_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pt.mo"));
const SV_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/sv.mo"));
const NL_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/nl.mo"));
const ZH_CN_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zh-CN.mo"));
const HE_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/he.mo"));
const IT_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/it.mo"));
const EL_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/el.mo"));
const ET_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/et.mo"));
const HI_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/hi.mo"));
const BN_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/bn.mo"));
const ID_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/id.mo"));

static CATALOGS: Lazy<HashMap<&'static str, Catalog>> = Lazy::new(|| {
	let mut map = HashMap::new();

	if let Ok(catalog) = Catalog::parse(EN_MO) {
		map.insert("en", catalog);
	} else {
		tracing::error!("Failed to parse English translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(ES_MO) {
		map.insert("es", catalog);
	} else {
		tracing::warn!("Failed to parse Spanish translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(AR_MO) {
		map.insert("ar", catalog);
	} else {
		tracing::warn!("Failed to parse Arabic translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(FR_MO) {
		map.insert("fr", catalog);
	} else {
		tracing::warn!("Failed to parse French translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(RU_MO) {
		map.insert("ru", catalog);
	} else {
		tracing::warn!("Failed to parse Russian translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(JA_MO) {
		map.insert("ja", catalog);
	} else {
		tracing::warn!("Failed to parse Japanese translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(KO_MO) {
		map.insert("ko", catalog);
	} else {
		tracing::warn!("Failed to parse Korean translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(PT_MO) {
		map.insert("pt", catalog);
	} else {
		tracing::warn!("Failed to parse Portuguese translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(SV_MO) {
		map.insert("sv", catalog);
	} else {
		tracing::warn!("Failed to parse Swedish translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(NL_MO) {
		map.insert("nl", catalog);
	} else {
		tracing::warn!("Failed to parse Dutch translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(ZH_CN_MO) {
		map.insert("zh-CN", catalog);
	} else {
		tracing::warn!("Failed to parse Simplified Chinese translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(HE_MO) {
		map.insert("he", catalog);
	} else {
		tracing::warn!("Failed to parse Hebrew translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(IT_MO) {
		map.insert("it", catalog);
	} else {
		tracing::warn!("Failed to parse Italian translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(EL_MO) {
		map.insert("el", catalog);
	} else {
		tracing::warn!("Failed to parse Greek translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(ET_MO) {
		map.insert("et", catalog);
	} else {
		tracing::warn!("Failed to parse Estonian translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(HI_MO) {
		map.insert("hi", catalog);
	} else {
		tracing::warn!("Failed to parse Hindi translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(BN_MO) {
		map.insert("bn", catalog);
	} else {
		tracing::warn!("Failed to parse Bengali translation catalog");
	}

	if let Ok(catalog) = Catalog::parse(ID_MO) {
		map.insert("id", catalog);
	} else {
		tracing::warn!("Failed to parse Indonesian translation catalog");
	}

	map
});

/// Translate a string for the given locale.
///
/// Falls back to English if the translation is not found, then to the msgid itself.
///
/// # Arguments
///
/// * `locale` - The locale code (e.g., "en", "es", "ar")
/// * `msgid` - The message ID to translate (e.g., "server.email.magic_link.subject")
///
/// # Returns
///
/// The translated string, or the English translation, or the msgid if not found.
///
/// # Example
///
/// ```
/// use loom_common_i18n::t;
///
/// let subject = t("es", "server.email.magic_link.subject");
/// ```
pub fn t(locale: &str, msgid: &str) -> String {
	if let Some(catalog) = CATALOGS.get(locale) {
		let translated = catalog.gettext(msgid);
		if translated != msgid {
			return translated.to_string();
		}
	}

	if locale != DEFAULT_LOCALE {
		if let Some(catalog) = CATALOGS.get(DEFAULT_LOCALE) {
			let translated = catalog.gettext(msgid);
			if translated != msgid {
				return translated.to_string();
			}
		}
	}

	msgid.to_string()
}

/// Translate a string with variable substitution.
///
/// Variables use `{name}` syntax in the translated string.
///
/// # Arguments
///
/// * `locale` - The locale code (e.g., "en", "es", "ar")
/// * `msgid` - The message ID to translate
/// * `args` - Variable substitutions as (name, value) pairs
///
/// # Returns
///
/// The translated string with variables substituted.
///
/// # Example
///
/// ```
/// use loom_common_i18n::t_fmt;
///
/// let body = t_fmt("es", "server.email.invitation.subject", &[
///     ("org_name", "Acme Corp"),
/// ]);
/// ```
pub fn t_fmt(locale: &str, msgid: &str, args: &[(&str, &str)]) -> String {
	let mut result = t(locale, msgid);

	for (name, value) in args {
		let placeholder = format!("{{{name}}}");
		result = result.replace(&placeholder, value);
	}

	result
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_translate_english() {
		let result = t("en", "server.email.magic_link.subject");
		assert_eq!(result, "Sign in to Loom");
	}

	#[test]
	fn test_translate_spanish() {
		let result = t("es", "server.email.magic_link.subject");
		assert_eq!(result, "Iniciar sesión en Loom");
	}

	#[test]
	fn test_translate_arabic() {
		let result = t("ar", "server.email.magic_link.subject");
		assert_eq!(result, "تسجيل الدخول إلى Loom");
	}

	#[test]
	fn test_fallback_to_english() {
		let result = t("es", "server.nonexistent.key");
		let en_result = t("en", "server.nonexistent.key");
		assert_eq!(result, en_result);
	}

	#[test]
	fn test_fallback_to_msgid() {
		let result = t("en", "completely.unknown.key");
		assert_eq!(result, "completely.unknown.key");
	}

	#[test]
	fn test_variable_substitution() {
		let result = t_fmt(
			"en",
			"server.email.magic_link.expires",
			&[("minutes", "10")],
		);
		assert!(result.contains("10"));
		assert!(!result.contains("{minutes}"));
	}

	#[test]
	fn test_multiple_variables() {
		let result = t_fmt(
			"en",
			"server.email.invitation.body",
			&[("inviter_name", "Alice"), ("org_name", "Acme")],
		);
		assert!(result.contains("Alice"));
		assert!(result.contains("Acme"));
	}

	#[test]
	fn test_unknown_locale_falls_back() {
		let result = t("xx", "server.email.magic_link.subject");
		let en_result = t("en", "server.email.magic_link.subject");
		assert_eq!(result, en_result);
	}
}
