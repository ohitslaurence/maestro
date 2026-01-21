// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Internationalization (i18n) support for Loom.
//!
//! This crate provides translation support for server-side strings using GNU gettext.
//! It supports both left-to-right (LTR) and right-to-left (RTL) languages.
//!
//! # String Naming Convention
//!
//! All translatable strings use a hierarchical dot-notation key format:
//!
//! - `server.` prefix for backend strings (emails, API responses)
//! - `client.` prefix for CLI strings
//!
//! Example: `server.email.magic_link.subject`
//!
//! # Example
//!
//! ```
//! use loom_common_i18n::{t, t_fmt, is_rtl, resolve_locale};
//!
//! // Simple translation
//! let subject = t("es", "server.email.magic_link.subject");
//!
//! // Translation with variables
//! let body = t_fmt("es", "server.email.invitation.subject", &[
//!     ("org_name", "Acme Corp"),
//! ]);
//!
//! // Check for RTL language
//! if is_rtl("ar") {
//!     // Add dir="rtl" to HTML
//! }
//!
//! // Resolve user's effective locale
//! let locale = resolve_locale(Some("es"), "en");
//! ```

mod catalog;
mod locale;
mod resolve;

pub use catalog::{t, t_fmt};
pub use locale::{available_locales, is_rtl, is_supported, locale_info, Direction, LocaleInfo};
pub use resolve::resolve_locale;

pub use locale::{DEFAULT_LOCALE, LOCALES};
