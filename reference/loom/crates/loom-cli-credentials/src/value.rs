// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Credential value types.

use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};

/// On-disk credential representation (JSON serializable).
///
/// This is provider-agnostic and can store either API keys or OAuth tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PersistedCredentialValue {
	/// OAuth credentials with refresh token.
	#[serde(rename = "oauth")]
	OAuth {
		refresh: String,
		access: String,
		expires: u64,
	},

	/// Static API key.
	#[serde(rename = "api")]
	ApiKey { key: String },
}

/// Runtime credential representation with secret protection.
///
/// Wraps sensitive values in `SecretString` to prevent accidental logging.
#[derive(Debug, Clone)]
pub enum CredentialValue {
	/// OAuth credentials with refresh token.
	OAuth {
		refresh: SecretString,
		access: SecretString,
		/// Expiration timestamp in milliseconds since epoch.
		expires: u64,
	},

	/// Static API key.
	ApiKey { key: SecretString },
}

impl From<PersistedCredentialValue> for CredentialValue {
	fn from(persisted: PersistedCredentialValue) -> Self {
		match persisted {
			PersistedCredentialValue::OAuth {
				refresh,
				access,
				expires,
			} => CredentialValue::OAuth {
				refresh: SecretString::new(refresh),
				access: SecretString::new(access),
				expires,
			},
			PersistedCredentialValue::ApiKey { key } => CredentialValue::ApiKey {
				key: SecretString::new(key),
			},
		}
	}
}

impl From<&CredentialValue> for PersistedCredentialValue {
	fn from(cred: &CredentialValue) -> Self {
		match cred {
			CredentialValue::OAuth {
				refresh,
				access,
				expires,
			} => PersistedCredentialValue::OAuth {
				refresh: refresh.expose().clone(),
				access: access.expose().clone(),
				expires: *expires,
			},
			CredentialValue::ApiKey { key } => PersistedCredentialValue::ApiKey {
				key: key.expose().clone(),
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_oauth_serialization() {
		let oauth = PersistedCredentialValue::OAuth {
			refresh: "rt_test".to_string(),
			access: "at_test".to_string(),
			expires: 12345,
		};

		let json = serde_json::to_string(&oauth).unwrap();
		assert!(json.contains("\"type\":\"oauth\""));
		assert!(json.contains("\"refresh\":\"rt_test\""));
	}

	#[test]
	fn test_api_key_serialization() {
		let api = PersistedCredentialValue::ApiKey {
			key: "sk-test".to_string(),
		};

		let json = serde_json::to_string(&api).unwrap();
		assert!(json.contains("\"type\":\"api\""));
		assert!(json.contains("\"key\":\"sk-test\""));
	}

	#[test]
	fn test_credential_value_conversion() {
		let persisted = PersistedCredentialValue::OAuth {
			refresh: "rt_test".to_string(),
			access: "at_test".to_string(),
			expires: 12345,
		};

		let runtime: CredentialValue = persisted.into();
		if let CredentialValue::OAuth {
			refresh,
			access,
			expires,
		} = runtime
		{
			assert_eq!(refresh.expose(), "rt_test");
			assert_eq!(access.expose(), "at_test");
			assert_eq!(expires, 12345);
		} else {
			panic!("Expected OAuth");
		}
	}
}
