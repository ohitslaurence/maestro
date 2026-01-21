<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Internationalization (i18n) System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2024-12-30

---

## 1. Overview

### Purpose

The Loom i18n system provides internationalization support for server-side strings, primarily for
email templates and API responses. It uses GNU gettext with `.po` files for translator-friendly
workflows and supports both left-to-right (LTR) and right-to-left (RTL) languages.

### Goals

- **Translator-friendly:** Use industry-standard `.po` files compatible with Poedit, Weblate, Crowdin
- **RTL support:** Full right-to-left language support from day one (Arabic, Hebrew, etc.)
- **Type-safe:** Compile-time embedding of translations, runtime locale resolution
- **Consistent:** Unified string naming conventions across server and client
- **Pure Rust:** No C dependencies (uses `gettext` crate, not `gettext-rs`)

### Non-Goals

- Frontend i18n (handled separately by LinguiJS in loom-web)
- Automatic translation
- Locale detection from HTTP headers (explicit user preference only)

### Related Specifications

- [Configuration System](configuration-system.md) - `LOOM_SERVER_DEFAULT_LOCALE` environment variable
- [loom-web Specification](loom-web.md) - Frontend i18n with LinguiJS

---

## 2. Architecture

### Crate Structure

```
crates/loom-i18n/
├── Cargo.toml
├── build.rs                    # Compiles .po → .mo at build time
├── src/
│   ├── lib.rs                  # Public API
│   ├── catalog.rs              # Gettext catalog loading
│   ├── locale.rs               # LocaleInfo, Direction enum
│   └── resolve.rs              # Locale resolution logic
└── locales/
    ├── en/
    │   └── messages.po         # English (source)
    ├── es/
    │   └── messages.po         # Spanish
    └── ar/
        └── messages.po         # Arabic (RTL)
```

### Dependency Graph

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ loom-server │────▶│  loom-auth  │────▶│  loom-i18n  │
└─────────────┘     └─────────────┘     └─────────────┘
                                               │
                                               ▼
                                        ┌─────────────┐
                                        │   gettext   │
                                        │ (pure Rust) │
                                        └─────────────┘
```

---

## 3. Locale Resolution

### Precedence Order

Locale is resolved using the following precedence (highest to lowest):

| Priority | Source | Description |
|----------|--------|-------------|
| 1 | User preference | `users.locale` column in database |
| 2 | Server default | `LOOM_SERVER_DEFAULT_LOCALE` environment variable |
| 3 | Fallback | `en` (English) |

### Resolution Algorithm

```rust
pub fn resolve_locale(
    user_locale: Option<&str>,
    server_default: &str,
) -> &'static str {
    // 1. User preference (if valid)
    if let Some(locale) = user_locale {
        if is_supported(locale) {
            return locale;
        }
    }
    
    // 2. Server default (if valid)
    if is_supported(server_default) {
        return server_default;
    }
    
    // 3. Fallback
    "en"
}
```

### Edge Cases

| Scenario | Resolution |
|----------|------------|
| New user (no account) | Server default |
| Org invitation to new user | Inviter's locale |
| User preference not set | Server default |
| Invalid locale code | Fallback to `en` |

---

## 4. String Naming Convention

All translatable strings use a hierarchical dot-notation key format.

### Prefix Rules

| Prefix | Usage | Example |
|--------|-------|---------|
| `server.` | Backend strings (emails, API responses) | `server.email.magic_link.subject` |
| `client.` | CLI strings (loom-cli output) | `client.error.connection_failed` |

### Hierarchy Structure

```
{prefix}.{domain}.{component}.{element}
```

| Level | Description | Examples |
|-------|-------------|----------|
| `prefix` | `server` or `client` | `server`, `client` |
| `domain` | Feature area | `email`, `api`, `auth`, `org` |
| `component` | Specific component | `magic_link`, `invitation`, `deletion` |
| `element` | String type | `subject`, `body`, `title`, `message` |

### Examples

```
# Email subjects
server.email.magic_link.subject         = "Sign in to Loom"
server.email.invitation.subject         = "You've been invited to join {org_name} on Loom"
server.email.deletion_warning.subject   = "Your Loom account is scheduled for deletion"
server.email.security_alert.subject     = "Loom Security Alert: {event}"

# Email bodies
server.email.magic_link.body            = "Click the link below to sign in..."
server.email.magic_link.expires         = "This link expires in {minutes} minutes."
server.email.invitation.body            = "{inviter_name} has invited you to join..."

# API responses
server.api.auth.check_email             = "Check your email for a login link"
server.api.error.internal               = "An internal error occurred"
server.api.error.not_found              = "Resource not found"

# CLI strings (future)
client.error.connection_failed          = "Failed to connect to server"
client.prompt.enter_workspace           = "Enter workspace path:"
```

### Variable Placeholders

Use `{variable_name}` syntax for interpolation:

```
server.email.deletion_warning.body = "Your account will be deleted in {days} days."
server.email.invitation.subject = "You've been invited to join {org_name} on Loom"
```

---

## 5. Supported Locales

### Initial Locales

| Code | Language | Native Name | Direction |
|------|----------|-------------|-----------|
| `en` | English | English | LTR |
| `es` | Spanish | Español | LTR |
| `ar` | Arabic | العربية | RTL |

### Locale Metadata

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone)]
pub struct LocaleInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub native_name: &'static str,
    pub direction: Direction,
}

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
];
```

---

## 6. RTL Support

### HTML Email Handling

RTL languages require special HTML attributes for proper rendering:

```rust
pub fn email_html_wrapper(locale: &str, body: &str) -> String {
    let info = locale_info(locale);
    let dir = match info.direction {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    };
    
    format!(r#"<!DOCTYPE html>
<html lang="{locale}" dir="{dir}">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body style="direction: {dir}; text-align: {align};">
    {body}
</body>
</html>"#,
        locale = locale,
        dir = dir,
        align = if info.direction == Direction::Rtl { "right" } else { "left" },
        body = body,
    )
}
```

### CSS Considerations

Use logical properties for RTL-safe styling:

| Physical (avoid) | Logical (prefer) |
|------------------|------------------|
| `margin-left` | `margin-inline-start` |
| `margin-right` | `margin-inline-end` |
| `padding-left` | `padding-inline-start` |
| `text-align: left` | `text-align: start` |

### Mixed Content

URLs and code snippets inside RTL text should be wrapped:

```html
<span dir="ltr">https://loom.example/verify?token=abc123</span>
```

---

## 7. Public API

### Core Functions

```rust
/// Translate a string for the given locale.
/// Falls back to English if translation not found.
pub fn t(locale: &str, msgid: &str) -> String;

/// Translate with variable substitution.
/// Variables use {name} syntax in the msgid.
pub fn t_fmt(locale: &str, msgid: &str, args: &[(&str, &str)]) -> String;

/// Get metadata for a locale.
/// Returns None if locale is not supported.
pub fn locale_info(locale: &str) -> Option<&'static LocaleInfo>;

/// Check if a locale uses right-to-left text direction.
pub fn is_rtl(locale: &str) -> bool;

/// Check if a locale is supported.
pub fn is_supported(locale: &str) -> bool;

/// Get all supported locales.
pub fn available_locales() -> &'static [LocaleInfo];

/// Resolve effective locale from user preference and server default.
pub fn resolve_locale(
    user_locale: Option<&str>,
    server_default: &str,
) -> &'static str;
```

### Usage Example

```rust
use loom_i18n::{t, t_fmt, is_rtl, resolve_locale};

// Simple translation
let subject = t("es", "server.email.magic_link.subject");
// → "Iniciar sesión en Loom"

// Translation with variables
let body = t_fmt("es", "server.email.invitation.body", &[
    ("inviter_name", "Alice"),
    ("org_name", "Acme Corp"),
]);
// → "Alice te ha invitado a unirte a Acme Corp en Loom."

// RTL check for email wrapper
if is_rtl("ar") {
    // Add dir="rtl" to HTML
}

// Resolve user's effective locale
let locale = resolve_locale(user.locale.as_deref(), &config.default_locale);
```

---

## 8. .po File Format

### Source File Structure (English)

```po
# Loom Server Translations
# Copyright (c) 2025 Geoffrey Huntley
# SPDX-License-Identifier: Proprietary

msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"
"Language: en\n"

#: Email: Magic Link - Subject
msgid "server.email.magic_link.subject"
msgstr "Sign in to Loom"

#: Email: Magic Link - Body
msgid "server.email.magic_link.body"
msgstr "Click the link below to sign in to your Loom account."

#: Email: Magic Link - Expiry notice
#, python-brace-format
msgid "server.email.magic_link.expires"
msgstr "This link expires in {minutes} minutes."

#: Email: Org Invitation - Subject
#, python-brace-format
msgid "server.email.invitation.subject"
msgstr "You've been invited to join {org_name} on Loom"
```

### Translation File (Spanish)

```po
msgid "server.email.magic_link.subject"
msgstr "Iniciar sesión en Loom"

msgid "server.email.magic_link.body"
msgstr "Haz clic en el siguiente enlace para iniciar sesión en tu cuenta de Loom."

msgid "server.email.magic_link.expires"
msgstr "Este enlace expira en {minutes} minutos."

msgid "server.email.invitation.subject"
msgstr "Te han invitado a unirte a {org_name} en Loom"
```

### Translation File (Arabic - RTL)

```po
msgid "server.email.magic_link.subject"
msgstr "تسجيل الدخول إلى Loom"

msgid "server.email.magic_link.body"
msgstr "انقر على الرابط أدناه لتسجيل الدخول إلى حسابك في Loom."

msgid "server.email.magic_link.expires"
msgstr "تنتهي صلاحية هذا الرابط خلال {minutes} دقيقة."

msgid "server.email.invitation.subject"
msgstr "لقد تمت دعوتك للانضمام إلى {org_name} على Loom"
```

---

## 9. Build Process

### Compilation Flow

```
.po files (source, human-editable)
        │
        ▼ msgfmt (build.rs)
        │
.mo files (binary, embedded)
        │
        ▼ include_bytes!
        │
Rust binary (translations embedded)
```

### build.rs

```rust
use std::process::Command;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=locales/");
    
    let locales = ["en", "es", "ar"];
    let out_dir = std::env::var("OUT_DIR").unwrap();
    
    for locale in locales {
        let po_path = format!("locales/{}/messages.po", locale);
        let mo_path = format!("{}/{}.mo", out_dir, locale);
        
        if Path::new(&po_path).exists() {
            Command::new("msgfmt")
                .args(["-o", &mo_path, &po_path])
                .status()
                .expect("Failed to run msgfmt");
        }
    }
}
```

### Embedding

```rust
const EN_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/en.mo"));
const ES_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/es.mo"));
const AR_MO: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ar.mo"));
```

---

## 10. Database Schema

### Users Table Migration

```sql
-- Add locale preference column
ALTER TABLE users ADD COLUMN locale VARCHAR(5) DEFAULT NULL;

-- Add index for locale queries
CREATE INDEX idx_users_locale ON users(locale);
```

### Repository Update

```rust
impl UserRepository {
    pub async fn update_locale(&self, user_id: &str, locale: &str) -> Result<()>;
    pub async fn get_locale(&self, user_id: &str) -> Result<Option<String>>;
}
```

---

## 11. Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LOOM_SERVER_DEFAULT_LOCALE` | No | `en` | Default locale for users without preference |

### Integration with ServerConfig

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
    // ... existing fields ...
    
    /// Default locale for emails and API responses.
    /// Used when user has no preference set.
    pub default_locale: String,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        // ... existing code ...
        
        let default_locale = std::env::var("LOOM_SERVER_DEFAULT_LOCALE")
            .unwrap_or_else(|_| "en".to_string());
        
        // Validate locale is supported
        if !loom_i18n::is_supported(&default_locale) {
            return Err(ConfigError::InvalidLocale(default_locale));
        }
        
        // ...
    }
}
```

---

## 12. Migration Guide

### Existing Email Strings

Before (hardcoded in `loom-auth/src/email.rs`):

```rust
EmailTemplate::MagicLink { .. } => {
    let subject = "Your Loom login link".to_string();
    let body = format!(
        "Click this link to log in to Loom:\n\n{}\n\nThis link expires in {} minutes.",
        url, expires_minutes
    );
    (subject, body)
}
```

After (using loom-i18n):

```rust
EmailTemplate::MagicLink { .. } => {
    let subject = t(locale, "server.email.magic_link.subject");
    let body = t_fmt(locale, "server.email.magic_link.body_full", &[
        ("url", url),
        ("minutes", &expires_minutes.to_string()),
    ]);
    (subject, body)
}
```

### Extracted Strings

| Old Location | String Key |
|--------------|------------|
| `loom-auth/email.rs` MagicLink subject | `server.email.magic_link.subject` |
| `loom-auth/email.rs` MagicLink body | `server.email.magic_link.body` |
| `loom-auth/email.rs` SecurityNotification | `server.email.security_alert.subject` |
| `loom-auth/email.rs` OrgInvitation | `server.email.invitation.subject` |
| `loom-auth/email.rs` AccountDeletionWarning | `server.email.deletion_warning.subject` |
| `loom-server/routes/auth.rs` magic link email | `server.email.magic_link.*` |
| `loom-server/routes/users.rs` deletion email | `server.email.deletion_scheduled.*` |

---

## 13. Testing

### Unit Tests

```rust
#[test]
fn test_translation_lookup() {
    assert_eq!(
        t("en", "server.email.magic_link.subject"),
        "Sign in to Loom"
    );
    assert_eq!(
        t("es", "server.email.magic_link.subject"),
        "Iniciar sesión en Loom"
    );
}

#[test]
fn test_variable_substitution() {
    let result = t_fmt("en", "server.email.invitation.subject", &[
        ("org_name", "Acme"),
    ]);
    assert_eq!(result, "You've been invited to join Acme on Loom");
}

#[test]
fn test_rtl_detection() {
    assert!(!is_rtl("en"));
    assert!(!is_rtl("es"));
    assert!(is_rtl("ar"));
}

#[test]
fn test_locale_resolution() {
    assert_eq!(resolve_locale(Some("es"), "en"), "es");
    assert_eq!(resolve_locale(None, "es"), "es");
    assert_eq!(resolve_locale(Some("invalid"), "en"), "en");
    assert_eq!(resolve_locale(None, "invalid"), "en");
}

#[test]
fn test_fallback_to_english() {
    // Unknown key returns English
    let result = t("es", "server.nonexistent.key");
    assert_eq!(result, t("en", "server.nonexistent.key"));
}
```

### Property Tests

```rust
proptest! {
    #[test]
    fn all_english_keys_have_translations(key in "[a-z._]+") {
        // Every English string should exist
        let en_result = t("en", &key);
        // Should not panic
    }
    
    #[test]
    fn variable_substitution_is_safe(
        key in "server\\.[a-z._]+",
        value in "[a-zA-Z0-9 ]{0,100}"
    ) {
        // Should handle any variable value safely
        let _ = t_fmt("en", &key, &[("var", &value)]);
    }
}
```

---

## 14. Future Considerations

### Pluralization

Gettext supports plural forms for different languages:

```po
msgid "server.email.deletion_warning.days_singular"
msgid_plural "server.email.deletion_warning.days_plural"
msgstr[0] "Your account will be deleted in {count} day."
msgstr[1] "Your account will be deleted in {count} days."
```

### Additional Locales

Priority order for future locales:

1. French (`fr`) - LTR
2. German (`de`) - LTR
3. Portuguese (`pt`) - LTR
4. Hebrew (`he`) - RTL
5. Chinese Simplified (`zh-CN`) - LTR
6. Japanese (`ja`) - LTR

### Translation Management

Consider integrating with:

- **Weblate** - Open source, self-hostable
- **Crowdin** - Commercial, good free tier
- **Transifex** - Commercial, enterprise focus

---

## Appendix A: Complete String Catalog

### Email Strings

| Key | English |
|-----|---------|
| `server.email.magic_link.subject` | Sign in to Loom |
| `server.email.magic_link.body` | Click the link below to sign in to your Loom account. |
| `server.email.magic_link.expires` | This link expires in {minutes} minutes. |
| `server.email.magic_link.ignore` | If you didn't request this email, you can safely ignore it. |
| `server.email.magic_link.copy_link` | Or copy and paste this link: |
| `server.email.invitation.subject` | You've been invited to join {org_name} on Loom |
| `server.email.invitation.body` | {inviter_name} has invited you to join the "{org_name}" organization on Loom. |
| `server.email.invitation.action` | Click the link below to accept the invitation: |
| `server.email.invitation.no_expiry` | This invitation does not expire, but can be revoked by the organization admin. |
| `server.email.security_alert.subject` | Loom Security Alert: {event} |
| `server.email.security_alert.body` | A security event occurred on your Loom account. |
| `server.email.security_alert.event` | Event: {event} |
| `server.email.security_alert.details` | Details: |
| `server.email.security_alert.action` | If this wasn't you, please review your account security settings immediately. |
| `server.email.deletion_warning.subject` | Your Loom account is scheduled for deletion |
| `server.email.deletion_warning.body` | Your Loom account is scheduled for permanent deletion in {days} days. |
| `server.email.deletion_warning.restore` | If you did not request this deletion, or if you've changed your mind, you can restore your account by logging in before the deletion date. |
| `server.email.deletion_warning.permanent` | After permanent deletion, your data cannot be recovered. |
| `server.email.deletion_scheduled.subject` | Account Deletion Scheduled |
| `server.email.deletion_scheduled.body` | Your account has been scheduled for deletion. |
| `server.email.deletion_scheduled.grace` | You have {days} days to restore your account by logging in and canceling the deletion. |
| `server.email.deletion_scheduled.permanent` | After the grace period, your account and all associated data will be permanently deleted. |

### API Response Strings

| Key | English |
|-----|---------|
| `server.api.auth.check_email` | Check your email for a login link |
| `server.api.auth.logged_out` | Successfully logged out |
| `server.api.error.internal` | An internal error occurred |
| `server.api.error.not_found` | Resource not found |
| `server.api.error.unauthorized` | Authentication required |
| `server.api.error.forbidden` | Access denied |
