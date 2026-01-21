// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secret redaction for audit events.
//!
//! This module provides automatic redaction of secrets from audit event `details`
//! fields using the `loom-redact` crate's gitleaks patterns.

use serde_json::Value;

const MAX_DEPTH: usize = 128;

/// Redacts secrets from a JSON value in-place.
///
/// This recursively walks the JSON structure and applies secret detection
/// to all string values AND object keys, replacing detected secrets with `[REDACTED:<rule-id>]`.
///
/// Depth is limited to prevent stack overflow from maliciously nested JSON.
pub fn redact_json_value(value: &mut Value) {
	redact_json_value_with_depth(value, 0);
}

fn redact_json_value_with_depth(value: &mut Value, depth: usize) {
	if depth > MAX_DEPTH {
		return;
	}

	match value {
		Value::String(s) => {
			let redacted = loom_redact::redact(s);
			if let std::borrow::Cow::Owned(new_s) = redacted {
				*s = new_s;
			}
		}
		Value::Array(arr) => {
			for item in arr {
				redact_json_value_with_depth(item, depth + 1);
			}
		}
		Value::Object(obj) => {
			let keys_to_check: Vec<String> = obj.keys().cloned().collect();
			let mut keys_to_rename: Vec<(String, String)> = Vec::new();

			for key in &keys_to_check {
				let redacted_key = loom_redact::redact(key);
				if let std::borrow::Cow::Owned(new_key) = redacted_key {
					keys_to_rename.push((key.clone(), new_key));
				}
			}

			for (old_key, new_key) in keys_to_rename {
				if let Some(val) = obj.remove(&old_key) {
					obj.insert(new_key, val);
				}
			}

			for (_, v) in obj.iter_mut() {
				redact_json_value_with_depth(v, depth + 1);
			}
		}
		_ => {}
	}
}

/// Redacts secrets from an audit event's details field.
///
/// Returns a new Value with all detected secrets replaced.
pub fn redact_details(details: &Value) -> Value {
	let mut cloned = details.clone();
	redact_json_value(&mut cloned);
	cloned
}

/// Redacts secrets from a string value.
///
/// Returns the redacted string (owned if changes were made, borrowed if not).
pub fn redact_string(s: &str) -> std::borrow::Cow<'_, str> {
	loom_redact::redact(s)
}

/// Redacts secrets from an optional string field in-place.
pub fn redact_optional_string(s: &mut Option<String>) {
	if let Some(ref mut val) = s {
		let redacted = loom_redact::redact(val);
		if let std::borrow::Cow::Owned(new_val) = redacted {
			*val = new_val;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn github_pat() -> String {
		format!("ghp_{}", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8")
	}

	fn aws_key() -> String {
		format!("AKIA{}", "Z7VRSQ5TJN2XMPLQ")
	}

	#[test]
	fn test_redact_simple_string() {
		let mut value = json!("This is a test");
		redact_json_value(&mut value);
		assert_eq!(value, json!("This is a test"));
	}

	#[test]
	fn test_redact_aws_key_in_string() {
		let mut value = json!(format!("AWS_ACCESS_KEY_ID={}", aws_key()));
		redact_json_value(&mut value);
		let s = value.as_str().unwrap();
		assert!(s.contains("[REDACTED:"), "Expected redaction: {s}");
		assert!(!s.contains(&aws_key()));
	}

	#[test]
	fn test_redact_github_pat_in_string() {
		let mut value = json!(format!("export GITHUB_TOKEN={}", github_pat()));
		redact_json_value(&mut value);
		let s = value.as_str().unwrap();
		assert!(s.contains("[REDACTED:"), "Expected redaction: {s}");
		assert!(!s.contains(&github_pat()));
	}

	#[test]
	fn test_redact_nested_object() {
		let mut value = json!({
			"user": "test",
			"credentials": {
				"github_token": format!("export GITHUB_TOKEN={}", github_pat())
			}
		});
		redact_json_value(&mut value);
		let token = &value["credentials"]["github_token"];
		assert!(
			token.as_str().unwrap().contains("[REDACTED:"),
			"Expected redaction in: {}",
			token
		);
	}

	#[test]
	fn test_redact_array_values() {
		let mut value = json!(["normal", format!("token={}", github_pat()), "also normal"]);
		redact_json_value(&mut value);
		let arr = value.as_array().unwrap();
		assert_eq!(arr[0], "normal");
		assert!(
			arr[1].as_str().unwrap().contains("[REDACTED:"),
			"Expected redaction in: {}",
			arr[1]
		);
		assert_eq!(arr[2], "also normal");
	}

	#[test]
	fn test_redact_preserves_numbers_and_bools() {
		let mut value = json!({
			"count": 42,
			"enabled": true,
			"rate": 3.14
		});
		let original = value.clone();
		redact_json_value(&mut value);
		assert_eq!(value, original);
	}

	#[test]
	fn test_redact_details_returns_new_value() {
		let original = json!({
			"secret": format!("GITHUB_TOKEN={}", github_pat())
		});
		let redacted = redact_details(&original);
		assert!(
			redacted["secret"].as_str().unwrap().contains("[REDACTED:"),
			"Expected redaction in: {}",
			redacted["secret"]
		);
		assert!(original["secret"].as_str().unwrap().contains(&github_pat()));
	}

	#[test]
	fn test_redact_secret_in_object_key() {
		let pat = github_pat();
		let mut value = json!({});
		value
			.as_object_mut()
			.unwrap()
			.insert(pat.clone(), json!("some value"));

		redact_json_value(&mut value);

		let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
		assert_eq!(keys.len(), 1);
		assert!(
			keys[0].contains("[REDACTED:"),
			"Expected key to be redacted: {}",
			keys[0]
		);
		assert!(!keys[0].contains(&pat));
	}

	#[test]
	fn test_depth_limit_prevents_stack_overflow() {
		fn deeply_nested(depth: usize) -> Value {
			if depth == 0 {
				json!("leaf")
			} else {
				json!({ "nested": deeply_nested(depth - 1) })
			}
		}

		let mut value = deeply_nested(200);
		redact_json_value(&mut value);
	}
}
