// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Helper for building event and person properties.

use serde_json::{Map, Value};

/// A builder for constructing event or person properties.
///
/// # Example
///
/// ```
/// use loom_analytics::Properties;
///
/// let props = Properties::new()
///     .insert("button_name", "checkout")
///     .insert("page", "/cart")
///     .insert("price", 99.99)
///     .insert("is_premium", true);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Properties {
	inner: Map<String, Value>,
}

impl Properties {
	/// Creates a new empty Properties builder.
	pub fn new() -> Self {
		Self { inner: Map::new() }
	}

	/// Inserts a key-value pair into the properties.
	///
	/// The value can be any type that implements `Into<serde_json::Value>`,
	/// including strings, numbers, booleans, arrays, and nested objects.
	pub fn insert<K, V>(mut self, key: K, value: V) -> Self
	where
		K: Into<String>,
		V: Into<Value>,
	{
		self.inner.insert(key.into(), value.into());
		self
	}

	/// Merges another Properties into this one.
	///
	/// If both contain the same key, the value from `other` takes precedence.
	pub fn merge(mut self, other: Properties) -> Self {
		for (k, v) in other.inner {
			self.inner.insert(k, v);
		}
		self
	}

	/// Returns true if the properties are empty.
	pub fn is_empty(&self) -> bool {
		self.inner.is_empty()
	}

	/// Returns the number of properties.
	pub fn len(&self) -> usize {
		self.inner.len()
	}

	/// Gets a value by key.
	pub fn get(&self, key: &str) -> Option<&Value> {
		self.inner.get(key)
	}

	/// Converts the properties into a `serde_json::Value`.
	pub fn into_value(self) -> Value {
		Value::Object(self.inner)
	}
}

impl From<Properties> for Value {
	fn from(props: Properties) -> Self {
		props.into_value()
	}
}

impl From<Value> for Properties {
	fn from(value: Value) -> Self {
		match value {
			Value::Object(map) => Self { inner: map },
			_ => Self::new(),
		}
	}
}

impl From<Map<String, Value>> for Properties {
	fn from(map: Map<String, Value>) -> Self {
		Self { inner: map }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_properties_new_is_empty() {
		let props = Properties::new();
		assert!(props.is_empty());
		assert_eq!(props.len(), 0);
	}

	#[test]
	fn test_properties_insert_string() {
		let props = Properties::new().insert("name", "Alice");
		assert_eq!(props.get("name"), Some(&Value::String("Alice".to_string())));
	}

	#[test]
	fn test_properties_insert_number() {
		let props = Properties::new().insert("count", 42);
		assert_eq!(props.get("count"), Some(&Value::Number(42.into())));
	}

	#[test]
	fn test_properties_insert_bool() {
		let props = Properties::new().insert("active", true);
		assert_eq!(props.get("active"), Some(&Value::Bool(true)));
	}

	#[test]
	fn test_properties_insert_float() {
		let props = Properties::new().insert("price", 99.99);
		let val = props.get("price").unwrap();
		assert!(val.is_f64());
	}

	#[test]
	fn test_properties_insert_multiple() {
		let props = Properties::new()
			.insert("name", "Bob")
			.insert("age", 30)
			.insert("active", true);

		assert_eq!(props.len(), 3);
		assert_eq!(props.get("name"), Some(&Value::String("Bob".to_string())));
		assert_eq!(props.get("age"), Some(&Value::Number(30.into())));
		assert_eq!(props.get("active"), Some(&Value::Bool(true)));
	}

	#[test]
	fn test_properties_merge() {
		let props1 = Properties::new().insert("a", 1).insert("b", 2);

		let props2 = Properties::new().insert("b", 20).insert("c", 3);

		let merged = props1.merge(props2);

		assert_eq!(merged.len(), 3);
		assert_eq!(merged.get("a"), Some(&Value::Number(1.into())));
		assert_eq!(merged.get("b"), Some(&Value::Number(20.into()))); // props2 wins
		assert_eq!(merged.get("c"), Some(&Value::Number(3.into())));
	}

	#[test]
	fn test_properties_into_value() {
		let props = Properties::new().insert("key", "value");
		let val = props.into_value();

		assert!(val.is_object());
		assert_eq!(val["key"], "value");
	}

	#[test]
	fn test_properties_from_value() {
		let val = serde_json::json!({"name": "test", "count": 5});
		let props = Properties::from(val);

		assert_eq!(props.len(), 2);
		assert_eq!(props.get("name"), Some(&Value::String("test".to_string())));
	}

	#[test]
	fn test_properties_from_non_object_value() {
		let val = Value::String("not an object".to_string());
		let props = Properties::from(val);

		assert!(props.is_empty());
	}

	proptest! {
		#[test]
		fn properties_len_matches_insertions(keys in proptest::collection::vec("[a-z]{1,10}", 0..20)) {
			let unique_keys: std::collections::HashSet<_> = keys.iter().cloned().collect();
			let mut props = Properties::new();
			for key in &keys {
				props = props.insert(key.clone(), "value");
			}
			prop_assert_eq!(props.len(), unique_keys.len());
		}

		#[test]
		fn properties_get_returns_inserted_value(key in "[a-z]{1,20}", value in "[a-zA-Z0-9]{1,50}") {
			let props = Properties::new().insert(key.clone(), value.clone());
			prop_assert_eq!(props.get(&key), Some(&Value::String(value)));
		}

		#[test]
		fn properties_into_value_roundtrip(key in "[a-z]{1,20}", value in "[a-zA-Z0-9]{1,50}") {
			let props = Properties::new().insert(key.clone(), value.clone());
			let json_val = props.into_value();
			let props_back = Properties::from(json_val);
			prop_assert_eq!(props_back.get(&key), Some(&Value::String(value)));
		}
	}
}
