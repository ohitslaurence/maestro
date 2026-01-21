// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::OrgId;

/// Unique identifier for a strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StrategyId(pub Uuid);

impl StrategyId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for StrategyId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for StrategyId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for StrategyId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Reusable rollout strategy with targeting conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
	pub id: StrategyId,
	/// None = platform-level strategy
	pub org_id: Option<OrgId>,
	pub name: String,
	pub description: Option<String>,
	/// All conditions must match (AND)
	pub conditions: Vec<Condition>,
	/// 0-100, applied after conditions
	pub percentage: Option<u32>,
	pub percentage_key: PercentageKey,
	pub schedule: Option<Schedule>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// A condition that must be met for a strategy to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Condition {
	Attribute {
		/// e.g., "plan", "created_at"
		attribute: String,
		operator: AttributeOperator,
		value: serde_json::Value,
	},
	Geographic {
		field: GeoField,
		operator: GeoOperator,
		/// e.g., ["US", "CA", "GB"]
		values: Vec<String>,
	},
	Environment {
		/// e.g., ["prod", "staging"]
		environments: Vec<String>,
	},
}

/// Operators for attribute conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeOperator {
	Equals,
	NotEquals,
	Contains,
	StartsWith,
	EndsWith,
	GreaterThan,
	LessThan,
	GreaterThanOrEquals,
	LessThanOrEquals,
	In,
	NotIn,
}

impl AttributeOperator {
	/// Evaluates this operator against two JSON values.
	pub fn evaluate(&self, actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
		match self {
			AttributeOperator::Equals => actual == expected,
			AttributeOperator::NotEquals => actual != expected,
			AttributeOperator::Contains => {
				if let (Some(actual_str), Some(expected_str)) = (actual.as_str(), expected.as_str()) {
					actual_str.contains(expected_str)
				} else if let Some(actual_arr) = actual.as_array() {
					actual_arr.contains(expected)
				} else {
					false
				}
			}
			AttributeOperator::StartsWith => {
				if let (Some(actual_str), Some(expected_str)) = (actual.as_str(), expected.as_str()) {
					actual_str.starts_with(expected_str)
				} else {
					false
				}
			}
			AttributeOperator::EndsWith => {
				if let (Some(actual_str), Some(expected_str)) = (actual.as_str(), expected.as_str()) {
					actual_str.ends_with(expected_str)
				} else {
					false
				}
			}
			AttributeOperator::GreaterThan => compare_values(actual, expected, |a, b| a > b),
			AttributeOperator::LessThan => compare_values(actual, expected, |a, b| a < b),
			AttributeOperator::GreaterThanOrEquals => compare_values(actual, expected, |a, b| a >= b),
			AttributeOperator::LessThanOrEquals => compare_values(actual, expected, |a, b| a <= b),
			AttributeOperator::In => {
				if let Some(expected_arr) = expected.as_array() {
					expected_arr.contains(actual)
				} else {
					false
				}
			}
			AttributeOperator::NotIn => {
				if let Some(expected_arr) = expected.as_array() {
					!expected_arr.contains(actual)
				} else {
					true
				}
			}
		}
	}
}

/// Helper to compare numeric values.
fn compare_values<F>(actual: &serde_json::Value, expected: &serde_json::Value, cmp: F) -> bool
where
	F: Fn(f64, f64) -> bool,
{
	match (actual.as_f64(), expected.as_f64()) {
		(Some(a), Some(b)) => cmp(a, b),
		_ => {
			// Try string comparison as fallback for dates, etc.
			match (actual.as_str(), expected.as_str()) {
				(Some(a), Some(b)) => cmp(a.len() as f64, b.len() as f64) || a.cmp(b).is_gt(),
				_ => false,
			}
		}
	}
}

/// Geographic targeting field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoField {
	Country,
	Region,
	City,
}

/// Geographic targeting operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoOperator {
	In,
	NotIn,
}

impl GeoOperator {
	/// Evaluates this operator against a value and list of values.
	pub fn evaluate(&self, actual: &str, values: &[String]) -> bool {
		let is_in = values.iter().any(|v| v.eq_ignore_ascii_case(actual));
		match self {
			GeoOperator::In => is_in,
			GeoOperator::NotIn => !is_in,
		}
	}
}

/// The key used for percentage-based distribution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PercentageKey {
	#[default]
	UserId,
	OrgId,
	SessionId,
	Custom(String),
}

impl PercentageKey {
	/// Gets the key value from an evaluation context.
	pub fn get_value<'a>(
		&self,
		user_id: Option<&'a str>,
		org_id: Option<&'a str>,
		session_id: Option<&'a str>,
		attributes: &'a std::collections::HashMap<String, serde_json::Value>,
	) -> Option<&'a str> {
		match self {
			PercentageKey::UserId => user_id,
			PercentageKey::OrgId => org_id,
			PercentageKey::SessionId => session_id,
			PercentageKey::Custom(key) => attributes.get(key).and_then(|v| v.as_str()),
		}
	}
}

/// A schedule for gradual rollout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Schedule {
	pub steps: Vec<ScheduleStep>,
}

impl Schedule {
	/// Evaluates the schedule at a given time to get the current percentage.
	pub fn evaluate(&self, now: DateTime<Utc>) -> u32 {
		let mut current_percentage = 0;
		for step in &self.steps {
			if now >= step.start_at {
				current_percentage = step.percentage;
			}
		}
		current_percentage
	}
}

/// A step in a rollout schedule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleStep {
	pub percentage: u32,
	pub start_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_attribute_operator_equals() {
		let op = AttributeOperator::Equals;
		assert!(op.evaluate(&serde_json::json!("test"), &serde_json::json!("test")));
		assert!(!op.evaluate(&serde_json::json!("test"), &serde_json::json!("other")));
		assert!(op.evaluate(&serde_json::json!(42), &serde_json::json!(42)));
		assert!(!op.evaluate(&serde_json::json!(42), &serde_json::json!(43)));
	}

	#[test]
	fn test_attribute_operator_contains() {
		let op = AttributeOperator::Contains;

		// String contains
		assert!(op.evaluate(
			&serde_json::json!("hello world"),
			&serde_json::json!("world")
		));
		assert!(!op.evaluate(&serde_json::json!("hello world"), &serde_json::json!("foo")));

		// Array contains
		assert!(op.evaluate(&serde_json::json!(["a", "b", "c"]), &serde_json::json!("b")));
		assert!(!op.evaluate(&serde_json::json!(["a", "b", "c"]), &serde_json::json!("d")));
	}

	#[test]
	fn test_attribute_operator_in() {
		let op = AttributeOperator::In;
		assert!(op.evaluate(&serde_json::json!("b"), &serde_json::json!(["a", "b", "c"])));
		assert!(!op.evaluate(&serde_json::json!("d"), &serde_json::json!(["a", "b", "c"])));
	}

	#[test]
	fn test_geo_operator() {
		let countries = vec!["US".to_string(), "CA".to_string(), "GB".to_string()];

		assert!(GeoOperator::In.evaluate("US", &countries));
		assert!(GeoOperator::In.evaluate("us", &countries)); // Case insensitive
		assert!(!GeoOperator::In.evaluate("DE", &countries));

		assert!(!GeoOperator::NotIn.evaluate("US", &countries));
		assert!(GeoOperator::NotIn.evaluate("DE", &countries));
	}

	#[test]
	fn test_schedule_evaluate() {
		use chrono::TimeZone;

		let schedule = Schedule {
			steps: vec![
				ScheduleStep {
					percentage: 10,
					start_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
				},
				ScheduleStep {
					percentage: 50,
					start_at: Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap(),
				},
				ScheduleStep {
					percentage: 100,
					start_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
				},
			],
		};

		// Before any step
		assert_eq!(
			schedule.evaluate(Utc.with_ymd_and_hms(2023, 12, 31, 0, 0, 0).unwrap()),
			0
		);

		// After first step
		assert_eq!(
			schedule.evaluate(Utc.with_ymd_and_hms(2024, 1, 10, 0, 0, 0).unwrap()),
			10
		);

		// After second step
		assert_eq!(
			schedule.evaluate(Utc.with_ymd_and_hms(2024, 1, 20, 0, 0, 0).unwrap()),
			50
		);

		// After third step
		assert_eq!(
			schedule.evaluate(Utc.with_ymd_and_hms(2024, 2, 15, 0, 0, 0).unwrap()),
			100
		);
	}
}

#[cfg(test)]
mod proptest_tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn equals_is_symmetric(val in any::<i64>()) {
			// Equals should be symmetric for same values
			let json_val = serde_json::json!(val);
			prop_assert!(AttributeOperator::Equals.evaluate(&json_val, &json_val));
		}

		#[test]
		fn not_equals_is_inverse_of_equals(a in -1000i64..1000, b in -1000i64..1000) {
			let json_a = serde_json::json!(a);
			let json_b = serde_json::json!(b);
			let eq = AttributeOperator::Equals.evaluate(&json_a, &json_b);
			let ne = AttributeOperator::NotEquals.evaluate(&json_a, &json_b);
			prop_assert_ne!(eq, ne);
		}

		#[test]
		fn in_with_single_element_is_equals(val in any::<String>()) {
			let json_val = serde_json::json!(val);
			let json_arr = serde_json::json!([val]);
			prop_assert!(AttributeOperator::In.evaluate(&json_val, &json_arr));
		}

		#[test]
		fn not_in_is_inverse_of_in(val in "[a-z]{1,10}", list in proptest::collection::vec("[a-z]{1,10}", 1..5)) {
			let json_val = serde_json::json!(val);
			let json_arr = serde_json::json!(list);
			let is_in = AttributeOperator::In.evaluate(&json_val, &json_arr);
			let not_in = AttributeOperator::NotIn.evaluate(&json_val, &json_arr);
			prop_assert_ne!(is_in, not_in);
		}

		#[test]
		fn greater_than_is_transitive(a in 0i64..100, b in 0i64..100, c in 0i64..100) {
			let json_a = serde_json::json!(a);
			let json_b = serde_json::json!(b);
			let json_c = serde_json::json!(c);

			// If a > b and b > c, then a > c
			let a_gt_b = AttributeOperator::GreaterThan.evaluate(&json_a, &json_b);
			let b_gt_c = AttributeOperator::GreaterThan.evaluate(&json_b, &json_c);
			let a_gt_c = AttributeOperator::GreaterThan.evaluate(&json_a, &json_c);

			if a_gt_b && b_gt_c {
				prop_assert!(a_gt_c);
			}
		}

		#[test]
		fn less_than_opposite_of_greater_than_or_equals(a in 0i64..100, b in 0i64..100) {
			let json_a = serde_json::json!(a);
			let json_b = serde_json::json!(b);

			let lt = AttributeOperator::LessThan.evaluate(&json_a, &json_b);
			let gte = AttributeOperator::GreaterThanOrEquals.evaluate(&json_a, &json_b);

			// a < b XOR a >= b (exactly one must be true, unless numeric comparison fails)
			if a != b {
				prop_assert_ne!(lt, gte);
			}
		}

		#[test]
		fn contains_substring(haystack in "[a-z]{5,20}", start in 0usize..5) {
			let needle = &haystack[start..start.min(haystack.len()-1).max(start) + 1];
			if !needle.is_empty() {
				let json_haystack = serde_json::json!(haystack);
				let json_needle = serde_json::json!(needle);
				prop_assert!(AttributeOperator::Contains.evaluate(&json_haystack, &json_needle));
			}
		}

		#[test]
		fn starts_with_prefix(s in "[a-z]{5,20}", len in 1usize..5) {
			let prefix = &s[..len.min(s.len())];
			let json_s = serde_json::json!(s);
			let json_prefix = serde_json::json!(prefix);
			prop_assert!(AttributeOperator::StartsWith.evaluate(&json_s, &json_prefix));
		}

		#[test]
		fn ends_with_suffix(s in "[a-z]{5,20}", len in 1usize..5) {
			let suffix_start = s.len().saturating_sub(len);
			let suffix = &s[suffix_start..];
			let json_s = serde_json::json!(s);
			let json_suffix = serde_json::json!(suffix);
			prop_assert!(AttributeOperator::EndsWith.evaluate(&json_s, &json_suffix));
		}

		#[test]
		fn geo_in_is_inverse_of_not_in(country in "[A-Z]{2}", countries in proptest::collection::vec("[A-Z]{2}", 1..5)) {
			let is_in = GeoOperator::In.evaluate(&country, &countries);
			let not_in = GeoOperator::NotIn.evaluate(&country, &countries);
			prop_assert_ne!(is_in, not_in);
		}

		#[test]
		fn geo_in_is_case_insensitive(country in "[a-z]{2}") {
			let countries = vec![country.to_uppercase()];
			prop_assert!(GeoOperator::In.evaluate(&country, &countries));
		}

		#[test]
		fn schedule_percentage_monotonically_increases(
			pct1 in 0u32..50,
			pct2 in 50u32..100
		) {
			use chrono::TimeZone;

			let schedule = Schedule {
				steps: vec![
					ScheduleStep {
						percentage: pct1,
						start_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
					},
					ScheduleStep {
						percentage: pct2,
						start_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
					},
				],
			};

			// Before first step
			let before = schedule.evaluate(Utc.with_ymd_and_hms(2019, 1, 1, 0, 0, 0).unwrap());
			// After first step
			let after_first = schedule.evaluate(Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap());
			// After second step
			let after_second = schedule.evaluate(Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap());

			prop_assert_eq!(before, 0);
			prop_assert_eq!(after_first, pct1);
			prop_assert_eq!(after_second, pct2);
			prop_assert!(pct1 <= pct2);
		}
	}
}
