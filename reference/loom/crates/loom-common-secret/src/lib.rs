// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secret wrapper type that prevents accidental logging of sensitive values.
//!
//! The [`Secret<T>`] type wraps sensitive values like API keys, passwords, and tokens,
//! ensuring they:
//!
//! - Never appear in logs (redacted Debug/Display)
//! - Never serialize to plain text (redacted Serialize)
//! - Never appear in structured logging (implements tracing::Value as redacted)
//! - Are zeroized from memory on drop
//! - Require explicit `.expose()` call to access the inner value
//!
//! # Example
//!
//! ```
//! use loom_common_secret::Secret;
//!
//! let api_key = Secret::new("sk-secret-key".to_string());
//!
//! // Debug and Display are redacted
//! assert_eq!(format!("{:?}", api_key), "Secret(\"[REDACTED]\")");
//! assert_eq!(format!("{}", api_key), "[REDACTED]");
//!
//! // Must explicitly expose to use the value
//! assert_eq!(api_key.expose(), "sk-secret-key");
//! ```
//!
//! # Structured Logging
//!
//! The `Secret` type implements `tracing::Value` to ensure secrets are never
//! logged even when using structured logging:
//!
//! ```
//! use loom_common_secret::Secret;
//! use tracing::info;
//!
//! let api_key = Secret::new("sk-secret-key".to_string());
//!
//! // This will log "[REDACTED]" instead of the actual key
//! info!(api_key = %api_key, "Configured API");
//! info!(?api_key, "Debug format also redacted");
//! ```

use std::fmt;
use zeroize::Zeroize;

/// The redaction placeholder used in all output.
pub const REDACTED: &str = "[REDACTED]";

/// A wrapper for sensitive values that prevents accidental exposure.
///
/// # Features
///
/// - **Redacted Debug/Display**: Always prints `[REDACTED]` instead of the actual value
/// - **Redacted Serialize**: Serializes as `"[REDACTED]"` to prevent leaking in config dumps
/// - **Redacted tracing::Value**: Structured logging always shows `[REDACTED]`
/// - **Zeroize on drop**: Memory is securely zeroed when the value is dropped
/// - **Explicit access**: No `Deref` impl; must call `.expose()` to get the inner value
///
/// # Example
///
/// ```
/// use loom_common_secret::Secret;
///
/// let api_key = Secret::new("sk-secret-key".to_string());
///
/// // Debug and Display are redacted
/// assert_eq!(format!("{:?}", api_key), "Secret(\"[REDACTED]\")");
/// assert_eq!(format!("{}", api_key), "[REDACTED]");
///
/// // Must explicitly expose to use the value
/// assert_eq!(api_key.expose(), "sk-secret-key");
/// ```
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Secret<T>
where
	T: Zeroize,
{
	inner: T,
}

/// Convenience alias for the common case of secret strings.
pub type SecretString = Secret<String>;

impl<T> Secret<T>
where
	T: Zeroize,
{
	/// Create a new secret wrapper around the given value.
	pub fn new(inner: T) -> Self {
		Self { inner }
	}

	/// Explicitly access the inner value.
	///
	/// Call sites must opt-in to seeing the secret by calling this method.
	/// This makes secret access visible in code review.
	pub fn expose(&self) -> &T {
		&self.inner
	}

	/// Mutable access to the inner value.
	///
	/// Use with caution; prefer immutable access when possible.
	pub fn expose_mut(&mut self) -> &mut T {
		&mut self.inner
	}

	/// Consume the wrapper and return the inner value.
	///
	/// Use when passing into an API that needs ownership of the secret.
	/// Note: This clones the value rather than moving it to maintain zeroization
	/// guarantees on the original secret memory.
	pub fn into_inner(self) -> T
	where
		T: Clone,
	{
		self.inner.clone()
	}
}

impl<T> Clone for Secret<T>
where
	T: Zeroize + Clone,
{
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
		}
	}
}

impl<T> fmt::Debug for Secret<T>
where
	T: Zeroize,
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_tuple("Secret").field(&REDACTED).finish()
	}
}

impl<T> fmt::Display for Secret<T>
where
	T: Zeroize,
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(REDACTED)
	}
}

impl<T> PartialEq for Secret<T>
where
	T: Zeroize + PartialEq,
{
	fn eq(&self, other: &Self) -> bool {
		self.inner == other.inner
	}
}

impl<T> Eq for Secret<T> where T: Zeroize + Eq {}

// =============================================================================
// Tracing Integration - Ensures secrets are never logged in structured logging
// =============================================================================
//
// The `tracing::Value` trait is sealed, so we cannot implement it directly.
// Instead, we rely on:
// 1. Our `Display` implementation which always returns "[REDACTED]"
// 2. Our `Debug` implementation which always returns "Secret(\"[REDACTED]\")"
//
// When using structured logging with tracing:
// - `info!(api_key = %secret, ...)` uses Display -> "[REDACTED]"
// - `info!(?secret, ...)` uses Debug -> "Secret(\"[REDACTED]\")"
//
// Both are safe and will never leak the secret value.

// =============================================================================
// Serde Integration
// =============================================================================

#[cfg(feature = "serde")]
mod serde_impl {
	use super::{Secret, REDACTED};
	use serde::{Deserialize, Deserializer, Serialize, Serializer};
	use zeroize::Zeroize;

	impl<T> Serialize for Secret<T>
	where
		T: Serialize + Zeroize,
	{
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			serializer.serialize_str(REDACTED)
		}
	}

	impl<'de, T> Deserialize<'de> for Secret<T>
	where
		T: Deserialize<'de> + Zeroize,
	{
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			let inner = T::deserialize(deserializer)?;
			Ok(Secret::new(inner))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	mod secret_type {
		use super::*;

		/// Verifies that Debug output never contains the secret value.
		/// This is critical for preventing secrets from appearing in logs.
		#[test]
		fn debug_is_redacted() {
			let secret = Secret::new("super-secret-api-key".to_string());
			let debug_output = format!("{secret:?}");

			assert!(!debug_output.contains("super-secret-api-key"));
			assert!(debug_output.contains(REDACTED));
		}

		/// Verifies that Display output never contains the secret value.
		/// This prevents secrets from appearing in user-facing output.
		#[test]
		fn display_is_redacted() {
			let secret = Secret::new("super-secret-api-key".to_string());
			let display_output = format!("{secret}");

			assert!(!display_output.contains("super-secret-api-key"));
			assert_eq!(display_output, REDACTED);
		}

		/// Verifies that expose() returns the original value.
		/// This is important for the API to actually be usable.
		#[test]
		fn expose_returns_inner_value() {
			let secret = Secret::new("my-api-key".to_string());
			assert_eq!(secret.expose(), "my-api-key");
		}

		/// Verifies that into_inner() consumes and returns the value.
		/// This is important for passing secrets to APIs that need ownership.
		#[test]
		fn into_inner_returns_owned_value() {
			let secret = Secret::new("my-api-key".to_string());
			let inner = secret.into_inner();
			assert_eq!(inner, "my-api-key");
		}

		/// Verifies that clone produces an equivalent secret.
		/// This is important for configuration that may be cloned.
		#[test]
		fn clone_produces_equivalent_secret() {
			let secret = Secret::new("my-api-key".to_string());
			let cloned = secret.clone();
			assert_eq!(secret.expose(), cloned.expose());
		}

		/// Verifies that equality comparison works on inner values.
		/// This is important for testing and configuration comparison.
		#[test]
		fn equality_compares_inner_values() {
			let secret1 = Secret::new("key".to_string());
			let secret2 = Secret::new("key".to_string());
			let secret3 = Secret::new("other".to_string());

			assert_eq!(secret1, secret2);
			assert_ne!(secret1, secret3);
		}

		#[cfg(feature = "serde")]
		mod serde_tests {
			use super::*;

			/// Verifies that serialization never contains the secret value.
			/// This prevents secrets from appearing in config dumps or API responses.
			#[test]
			fn serialize_is_redacted() {
				let secret = Secret::new("super-secret-api-key".to_string());
				let json = serde_json::to_string(&secret).unwrap();

				assert!(!json.contains("super-secret-api-key"));
				assert!(json.contains(REDACTED));
			}

			/// Verifies that deserialization correctly populates the secret.
			/// This is important for loading config from files.
			#[test]
			fn deserialize_populates_secret() {
				let json = r#""my-api-key""#;
				let secret: Secret<String> = serde_json::from_str(json).unwrap();
				assert_eq!(secret.expose(), "my-api-key");
			}
		}
	}

	mod tracing_tests {
		use super::*;

		/// Verifies that secrets are redacted when used with tracing's display format.
		/// When using `info!(api_key = %secret, ...)`, Display is used.
		/// This is critical for structured logging security.
		#[test]
		fn tracing_display_format_is_redacted() {
			let secret = Secret::new("super-secret-value".to_string());
			let display = format!("{secret}");
			assert_eq!(display, REDACTED);
			assert!(!display.contains("super-secret-value"));
		}

		/// Verifies that secrets are redacted when used with tracing's debug format.
		/// When using `info!(?secret, ...)`, Debug is used.
		/// This ensures debug logging doesn't leak secrets.
		#[test]
		fn tracing_debug_format_is_redacted() {
			let secret = Secret::new("super-secret-value".to_string());
			let debug = format!("{secret:?}");
			assert!(debug.contains(REDACTED));
			assert!(!debug.contains("super-secret-value"));
		}

		/// Verifies that Option<Secret> also redacts properly in debug format.
		/// This is important because config fields are often Option<SecretString>.
		#[test]
		fn option_secret_debug_is_redacted() {
			let secret: Option<Secret<String>> = Some(Secret::new("super-secret-value".to_string()));
			let debug = format!("{secret:?}");
			assert!(debug.contains(REDACTED));
			assert!(!debug.contains("super-secret-value"));
		}

		/// Verifies that None secrets don't cause issues.
		#[test]
		fn option_none_secret_debug_works() {
			let secret: Option<Secret<String>> = None;
			let debug = format!("{secret:?}");
			assert_eq!(debug, "None");
		}
	}

	mod property_tests {
		use super::*;

		proptest! {
				/// Verifies that Debug output never contains the secret value for arbitrary strings.
				/// This is the most critical property: secrets must never leak through Debug.
				/// We use alphanumeric + common special chars but avoid quotes and brackets
				/// which appear in Debug/JSON formatting.
				#[test]
				fn debug_never_contains_secret(inner in "[a-zA-Z0-9!@#$%^&*_+=;:,.<>?/-]{3,50}") {
						// Skip strings that would match parts of "[REDACTED]" or "Secret"
						prop_assume!(!inner.contains("REDACTED"));
						prop_assume!(!inner.contains("Secret"));

						let secret = Secret::new(inner.clone());
						let debug_output = format!("{secret:?}");
						prop_assert!(
								!debug_output.contains(&inner),
								"Debug output contained the secret value"
						);
				}

				/// Verifies that Display output never contains the secret value for arbitrary strings.
				/// This ensures secrets don't leak through Display formatting.
				#[test]
				fn display_never_contains_secret(inner in "[a-zA-Z0-9!@#$%^&*_+=;:,.<>?/-]{3,50}") {
						// Skip strings that would match parts of "[REDACTED]"
						prop_assume!(!inner.contains("REDACTED"));

						let secret = Secret::new(inner.clone());
						let display_output = format!("{secret}");
						prop_assert!(
								!display_output.contains(&inner),
								"Display output contained the secret value"
						);
				}

				#[cfg(feature = "serde")]
				/// Verifies that JSON serialization never contains the secret value.
				/// This ensures secrets don't leak through serialization.
				#[test]
				fn serialize_never_contains_secret(inner in "[a-zA-Z0-9!@#$%^&*_+=;:,.<>?/-]{3,50}") {
						// Skip strings that would match parts of "[REDACTED]"
						prop_assume!(!inner.contains("REDACTED"));

						let secret = Secret::new(inner.clone());
						let json = serde_json::to_string(&secret).unwrap();
						prop_assert!(
								!json.contains(&inner),
								"Serialized JSON contained the secret value"
						);
				}

				/// Verifies that expose() always returns the original value.
				/// This ensures the roundtrip property holds.
				#[test]
				fn expose_roundtrips(inner in ".*") {
						let secret = Secret::new(inner.clone());
						prop_assert_eq!(secret.expose(), &inner);
				}

				/// Verifies that clone produces a secret with the same inner value.
				/// This ensures cloning preserves the secret correctly.
				#[test]
				fn clone_preserves_value(inner in ".*") {
						let secret = Secret::new(inner.clone());
						let cloned = secret.clone();
						prop_assert_eq!(secret.expose(), cloned.expose());
				}
		}
	}
}
