// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core types for the Loom feature flags system.
//!
//! This crate provides shared types for feature flags, strategies, kill switches,
//! and evaluation context. It is used by both the server-side evaluation engine
//! (`loom-server-flags`) and the client SDK (`loom-flags`).
//!
//! # Overview
//!
//! The feature flags system supports:
//! - Multi-variant flags with boolean, string, or JSON values
//! - Per-environment configuration (dev, staging, prod)
//! - Rollout strategies with percentage, attribute, and geographic targeting
//! - Kill switches for emergency feature disablement
//! - Real-time updates via SSE
//!
//! # Example
//!
//! ```
//! use loom_flags_core::{
//!     EvaluationContext, EvaluationResult, EvaluationReason,
//!     Flag, Variant, VariantValue,
//! };
//!
//! // Build evaluation context
//! let ctx = EvaluationContext::new("prod")
//!     .with_user_id("user123")
//!     .with_attribute("plan", serde_json::json!("enterprise"));
//!
//! // Evaluation results include the variant and reason
//! let result = EvaluationResult::new(
//!     "feature.new_flow",
//!     "enabled",
//!     VariantValue::Boolean(true),
//!     EvaluationReason::Default,
//! );
//! ```

pub mod environment;
pub mod error;
pub mod evaluation;
pub mod flag;
pub mod kill_switch;
pub mod sdk_key;
pub mod sse;
pub mod strategy;

pub use environment::Environment;
pub use error::{FlagsError, Result};
pub use evaluation::{
	BulkEvaluationResult, EvaluationContext, EvaluationReason, EvaluationResult, ExposureLog,
	ExposureLogId, FlagStats, GeoContext,
};
pub use flag::{
	EnvironmentId, Flag, FlagConfig, FlagConfigId, FlagId, FlagPrerequisite, OrgId, UserId, Variant,
	VariantValue,
};
pub use kill_switch::{KillSwitch, KillSwitchId};
pub use sdk_key::{SdkKey, SdkKeyId, SdkKeyType};
pub use sse::{
	ConnectionInfo, FlagArchivedData, FlagRestoredData, FlagState, FlagStreamEvent, FlagUpdatedData,
	HeartbeatData, InitData, KillSwitchActivatedData, KillSwitchDeactivatedData, KillSwitchState,
};
pub use strategy::{
	AttributeOperator, Condition, GeoField, GeoOperator, PercentageKey, Schedule, ScheduleStep,
	Strategy, StrategyId,
};

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	// Property-based tests for flag key validation
	proptest! {
		#[test]
		fn flag_key_starts_with_lowercase(s in "[a-z][a-z0-9_]{2,99}") {
			// Valid keys starting with lowercase should pass
			assert!(Flag::validate_key(&s));
		}

		#[test]
		fn flag_key_rejects_uppercase(s in "[A-Z][a-z0-9_]{2,99}") {
			// Keys starting with uppercase should fail
			assert!(!Flag::validate_key(&s));
		}

		#[test]
		fn flag_key_rejects_too_short(s in "[a-z][a-z0-9_]{0,1}") {
			// Keys with 1-2 chars should fail
			assert!(!Flag::validate_key(&s));
		}

		#[test]
		fn flag_key_with_dots_valid(domain in "[a-z][a-z0-9_]{1,10}", feature in "[a-z][a-z0-9_]{1,10}") {
			let key = format!("{}.{}", domain, feature);
			assert!(Flag::validate_key(&key));
		}

		#[test]
		fn environment_name_valid(s in "[a-z][a-z0-9_]{1,49}") {
			assert!(Environment::validate_name(&s));
		}

		#[test]
		fn environment_name_rejects_dashes(s in "[a-z][a-z0-9-]{1,49}") {
			// Names with dashes should fail
			if s.contains('-') {
				assert!(!Environment::validate_name(&s));
			}
		}

		#[test]
		fn kill_switch_key_valid(s in "[a-z][a-z0-9_]{2,99}") {
			assert!(KillSwitch::validate_key(&s));
		}

		#[test]
		fn color_hex_valid(r in "[0-9a-fA-F]{2}", g in "[0-9a-fA-F]{2}", b in "[0-9a-fA-F]{2}") {
			let color = format!("#{}{}{}", r, g, b);
			assert!(Environment::validate_color(&color));
		}
	}

	// Property-based tests for attribute operators
	proptest! {
		#[test]
		fn equals_is_symmetric(a: i64, b: i64) {
			let val_a = serde_json::json!(a);
			let val_b = serde_json::json!(b);
			let result = AttributeOperator::Equals.evaluate(&val_a, &val_b);
			assert_eq!(result, a == b);
		}

		#[test]
		fn not_equals_is_negation_of_equals(a: i64, b: i64) {
			let val_a = serde_json::json!(a);
			let val_b = serde_json::json!(b);
			let eq = AttributeOperator::Equals.evaluate(&val_a, &val_b);
			let neq = AttributeOperator::NotEquals.evaluate(&val_a, &val_b);
			assert_eq!(eq, !neq);
		}

		#[test]
		fn in_contains_element(values in prop::collection::vec(1i64..100, 1..10), idx in 0usize..10) {
			if !values.is_empty() {
				let idx = idx % values.len();
				let needle = serde_json::json!(values[idx]);
				let haystack = serde_json::json!(values);
				assert!(AttributeOperator::In.evaluate(&needle, &haystack));
			}
		}

		#[test]
		fn not_in_is_negation_of_in(needle: i64, haystack in prop::collection::vec(1i64..100, 0..5)) {
			let needle_val = serde_json::json!(needle);
			let haystack_val = serde_json::json!(haystack);
			let is_in = AttributeOperator::In.evaluate(&needle_val, &haystack_val);
			let not_in = AttributeOperator::NotIn.evaluate(&needle_val, &haystack_val);
			assert_eq!(is_in, !not_in);
		}
	}

	// Property-based tests for schedule evaluation
	proptest! {
		#[test]
		fn schedule_percentage_is_monotonic(
			p1 in 0u32..=50,
			p2 in 50u32..=100,
		) {
			use chrono::{TimeZone, Utc};

			let schedule = Schedule {
				steps: vec![
					ScheduleStep {
						percentage: p1,
						start_at: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
					},
					ScheduleStep {
						percentage: p2,
						start_at: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
					},
				],
			};

			let early = schedule.evaluate(Utc.with_ymd_and_hms(2024, 1, 15, 0, 0, 0).unwrap());
			let late = schedule.evaluate(Utc.with_ymd_and_hms(2024, 2, 15, 0, 0, 0).unwrap());

			// Later schedule step should have >= percentage (assuming monotonic rollout)
			assert!(late >= early || p2 < p1);
		}
	}

	// Property-based tests for geo operators
	proptest! {
		#[test]
		fn geo_in_contains_value(
			values in prop::collection::vec("[A-Z]{2}", 1..5),
			idx in 0usize..5,
		) {
			if !values.is_empty() {
				let idx = idx % values.len();
				let needle = &values[idx];
				assert!(GeoOperator::In.evaluate(needle, &values));
			}
		}

		#[test]
		fn geo_not_in_is_negation(
			needle in "[A-Z]{2}",
			values in prop::collection::vec("[A-Z]{2}", 0..5),
		) {
			let is_in = GeoOperator::In.evaluate(&needle, &values);
			let not_in = GeoOperator::NotIn.evaluate(&needle, &values);
			assert_eq!(is_in, !not_in);
		}
	}

	// Property-based tests for SDK key parsing
	proptest! {
		#[test]
		fn sdk_key_roundtrip(
			key_type in prop_oneof![Just(SdkKeyType::ClientSide), Just(SdkKeyType::ServerSide)],
			env_name in "[a-z]{2,10}",
		) {
			let generated = SdkKey::generate_key(key_type, &env_name);
			let parsed = SdkKey::parse_key(&generated);
			assert!(parsed.is_some());
			let (parsed_type, parsed_env, _) = parsed.unwrap();
			assert_eq!(parsed_type, key_type);
			assert_eq!(parsed_env, env_name);
		}
	}

	// Property-based tests for kill switch behavior
	proptest! {
		#[test]
		fn kill_switch_affects_linked_flag_when_active(
			linked_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 1..5),
			idx in 0usize..5,
		) {
			use chrono::Utc;

			if !linked_keys.is_empty() {
				let idx = idx % linked_keys.len();
				let kill_switch = KillSwitch {
					id: KillSwitchId::new(),
					org_id: None,
					key: "test_kill_switch".to_string(),
					name: "Test Kill Switch".to_string(),
					description: None,
					linked_flag_keys: linked_keys.clone(),
					is_active: true,
					activated_at: Some(Utc::now()),
					activated_by: Some(UserId::new()),
					activation_reason: Some("Testing".to_string()),
					created_at: Utc::now(),
					updated_at: Utc::now(),
				};

				// Active kill switch should affect linked flags
				assert!(kill_switch.affects_flag(&linked_keys[idx]));
			}
		}

		#[test]
		fn inactive_kill_switch_does_not_affect_flags(
			linked_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 1..5),
			idx in 0usize..5,
		) {
			use chrono::Utc;

			if !linked_keys.is_empty() {
				let idx = idx % linked_keys.len();
				let kill_switch = KillSwitch {
					id: KillSwitchId::new(),
					org_id: None,
					key: "test_kill_switch".to_string(),
					name: "Test Kill Switch".to_string(),
					description: None,
					linked_flag_keys: linked_keys.clone(),
					is_active: false,
					activated_at: None,
					activated_by: None,
					activation_reason: None,
					created_at: Utc::now(),
					updated_at: Utc::now(),
				};

				// Inactive kill switch should not affect any flags
				assert!(!kill_switch.affects_flag(&linked_keys[idx]));
			}
		}

		#[test]
		fn kill_switch_does_not_affect_unlinked_flags(
			linked_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 1..5),
			unlinked_key in "[a-z][a-z0-9_.]{2,20}",
		) {
			use chrono::Utc;

			// Only test if unlinked key is not in linked keys
			if !linked_keys.contains(&unlinked_key) {
				let kill_switch = KillSwitch {
					id: KillSwitchId::new(),
					org_id: None,
					key: "test_kill_switch".to_string(),
					name: "Test Kill Switch".to_string(),
					description: None,
					linked_flag_keys: linked_keys,
					is_active: true,
					activated_at: Some(Utc::now()),
					activated_by: Some(UserId::new()),
					activation_reason: Some("Testing".to_string()),
					created_at: Utc::now(),
					updated_at: Utc::now(),
				};

				// Active kill switch should not affect unlinked flags
				assert!(!kill_switch.affects_flag(&unlinked_key));
			}
		}

		#[test]
		fn activate_sets_correct_state(reason in "[a-zA-Z0-9 ]{1,100}") {
			use chrono::Utc;

			let mut kill_switch = KillSwitch {
				id: KillSwitchId::new(),
				org_id: None,
				key: "test_kill_switch".to_string(),
				name: "Test Kill Switch".to_string(),
				description: None,
				linked_flag_keys: vec!["test.flag".to_string()],
				is_active: false,
				activated_at: None,
				activated_by: None,
				activation_reason: None,
				created_at: Utc::now(),
				updated_at: Utc::now(),
			};

			let user_id = UserId::new();
			let old_updated = kill_switch.updated_at;
			kill_switch.activate(user_id, reason.clone());

			assert!(kill_switch.is_active);
			assert!(kill_switch.activated_at.is_some());
			assert_eq!(kill_switch.activated_by, Some(user_id));
			assert_eq!(kill_switch.activation_reason, Some(reason));
			assert!(kill_switch.updated_at >= old_updated);
		}

		#[test]
		fn deactivate_clears_activation_state(_seed: u64) {
			use chrono::Utc;

			let mut kill_switch = KillSwitch {
				id: KillSwitchId::new(),
				org_id: None,
				key: "test_kill_switch".to_string(),
				name: "Test Kill Switch".to_string(),
				description: None,
				linked_flag_keys: vec!["test.flag".to_string()],
				is_active: true,
				activated_at: Some(Utc::now()),
				activated_by: Some(UserId::new()),
				activation_reason: Some("Initial activation".to_string()),
				created_at: Utc::now(),
				updated_at: Utc::now(),
			};

			let old_updated = kill_switch.updated_at;
			kill_switch.deactivate();

			assert!(!kill_switch.is_active);
			assert!(kill_switch.activated_at.is_none());
			assert!(kill_switch.activated_by.is_none());
			assert!(kill_switch.activation_reason.is_none());
			assert!(kill_switch.updated_at >= old_updated);
		}
	}

	// Property-based tests for SSE event serialization roundtrips
	proptest! {
		#[test]
		fn flag_updated_event_roundtrip(
			flag_key in "[a-z][a-z0-9_.]{2,30}",
			environment in "[a-z][a-z0-9_]{2,20}",
			enabled in proptest::bool::ANY,
			default_variant in "[a-z][a-z0-9_]{1,20}",
		) {
			let event = FlagStreamEvent::flag_updated(
				flag_key.clone(),
				environment.clone(),
				enabled,
				default_variant.clone(),
				VariantValue::Boolean(enabled),
			);

			let json = serde_json::to_string(&event).unwrap();
			let parsed: FlagStreamEvent = serde_json::from_str(&json).unwrap();

			if let FlagStreamEvent::FlagUpdated(data) = parsed {
				assert_eq!(data.flag_key, flag_key);
				assert_eq!(data.environment, environment);
				assert_eq!(data.enabled, enabled);
				assert_eq!(data.default_variant, default_variant);
			} else {
				panic!("Expected FlagUpdated event");
			}
		}

		#[test]
		fn flag_archived_event_roundtrip(flag_key in "[a-z][a-z0-9_.]{2,30}") {
			let event = FlagStreamEvent::flag_archived(flag_key.clone());
			let json = serde_json::to_string(&event).unwrap();
			let parsed: FlagStreamEvent = serde_json::from_str(&json).unwrap();

			if let FlagStreamEvent::FlagArchived(data) = parsed {
				assert_eq!(data.flag_key, flag_key);
			} else {
				panic!("Expected FlagArchived event");
			}
		}

		#[test]
		fn kill_switch_activated_event_roundtrip(
			kill_switch_key in "[a-z][a-z0-9_]{2,30}",
			linked_flag_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 0..5),
			reason in "[a-zA-Z0-9 ]{1,100}",
		) {
			let event = FlagStreamEvent::kill_switch_activated(
				kill_switch_key.clone(),
				linked_flag_keys.clone(),
				reason.clone(),
			);

			let json = serde_json::to_string(&event).unwrap();
			let parsed: FlagStreamEvent = serde_json::from_str(&json).unwrap();

			if let FlagStreamEvent::KillSwitchActivated(data) = parsed {
				assert_eq!(data.kill_switch_key, kill_switch_key);
				assert_eq!(data.linked_flag_keys, linked_flag_keys);
				assert_eq!(data.reason, reason);
			} else {
				panic!("Expected KillSwitchActivated event");
			}
		}

		#[test]
		fn event_type_matches_serialized_tag(enabled in proptest::bool::ANY) {
			// Test that event_type() matches the "event" field in serialized JSON
			let events = vec![
				FlagStreamEvent::init(vec![], vec![]),
				FlagStreamEvent::flag_updated(
					"test".to_string(),
					"prod".to_string(),
					enabled,
					"default".to_string(),
					VariantValue::Boolean(enabled),
				),
				FlagStreamEvent::flag_archived("test".to_string()),
				FlagStreamEvent::flag_restored("test".to_string(), "prod".to_string(), enabled),
				FlagStreamEvent::kill_switch_activated("ks".to_string(), vec![], "reason".to_string()),
				FlagStreamEvent::kill_switch_deactivated("ks".to_string(), vec![]),
				FlagStreamEvent::heartbeat(),
			];

			for event in events {
				let event_type = event.event_type();
				let json = serde_json::to_string(&event).unwrap();
				assert!(json.contains(&format!(r#""event":"{}""#, event_type)));
			}
		}
	}

	// Property-based tests for FlagState construction
	proptest! {
		#[test]
		fn flag_state_from_flag_enabled_matches_config(
			flag_key in "[a-z][a-z0-9_.]{2,30}",
			enabled in proptest::bool::ANY,
		) {
			let flag = Flag {
				id: FlagId::new(),
				org_id: Some(OrgId::new()),
				key: flag_key.clone(),
				name: "Test Flag".to_string(),
				description: None,
				tags: vec![],
				maintainer_user_id: None,
				variants: vec![
					Variant {
						name: "on".to_string(),
						value: VariantValue::Boolean(true),
						weight: 50,
					},
					Variant {
						name: "off".to_string(),
						value: VariantValue::Boolean(false),
						weight: 50,
					},
				],
				default_variant: "on".to_string(),
				prerequisites: vec![],
				exposure_tracking_enabled: false,
				created_at: chrono::Utc::now(),
				updated_at: chrono::Utc::now(),
				archived_at: None,
			};

			let config = FlagConfig {
				id: flag::FlagConfigId::new(),
				flag_id: flag.id,
				environment_id: EnvironmentId::new(),
				enabled,
				strategy_id: None,
				created_at: chrono::Utc::now(),
				updated_at: chrono::Utc::now(),
			};

			let state = FlagState::from_flag_and_config(&flag, Some(&config));
			assert_eq!(state.key, flag_key);
			assert_eq!(state.enabled, enabled);
			assert!(!state.archived);
		}

		#[test]
		fn flag_state_disabled_when_no_config(flag_key in "[a-z][a-z0-9_.]{2,30}") {
			let flag = Flag {
				id: FlagId::new(),
				org_id: Some(OrgId::new()),
				key: flag_key.clone(),
				name: "Test Flag".to_string(),
				description: None,
				tags: vec![],
				maintainer_user_id: None,
				variants: vec![
					Variant {
						name: "on".to_string(),
						value: VariantValue::Boolean(true),
						weight: 100,
					},
				],
				default_variant: "on".to_string(),
				prerequisites: vec![],
				exposure_tracking_enabled: false,
				created_at: chrono::Utc::now(),
				updated_at: chrono::Utc::now(),
				archived_at: None,
			};

			let state = FlagState::from_flag_and_config(&flag, None);
			assert_eq!(state.key, flag_key);
			assert!(!state.enabled); // Should be disabled when no config
		}
	}

	// Property-based tests for context hash determinism
	proptest! {
		#[test]
		fn context_hash_is_deterministic(
			user_id in "[a-zA-Z0-9]{1,20}",
			org_id in "[a-zA-Z0-9]{1,20}",
			session_id in "[a-zA-Z0-9]{1,20}",
			flag_key in "[a-z][a-z0-9_.]{2,30}",
		) {
			let ctx = EvaluationContext::new("prod")
				.with_user_id(&user_id)
				.with_org_id(&org_id)
				.with_session_id(&session_id);

			let hash1 = ctx.compute_hash(&flag_key);
			let hash2 = ctx.compute_hash(&flag_key);

			prop_assert_eq!(hash1, hash2);
		}

		#[test]
		fn context_hash_is_64_hex_chars(
			user_id in "[a-zA-Z0-9]{0,20}",
			flag_key in "[a-z][a-z0-9_.]{2,30}",
		) {
			let ctx = EvaluationContext::new("prod")
				.with_user_id(&user_id);

			let hash = ctx.compute_hash(&flag_key);

			// SHA-256 hex = 64 chars
			prop_assert_eq!(hash.len(), 64);
			prop_assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn different_flag_keys_yield_different_hashes(
			flag_key1 in "[a-z][a-z0-9_.]{2,30}",
			flag_key2 in "[a-z][a-z0-9_.]{2,30}",
		) {
			// Only test if keys are different
			if flag_key1 != flag_key2 {
				let ctx = EvaluationContext::new("prod")
					.with_user_id("testuser");

				let hash1 = ctx.compute_hash(&flag_key1);
				let hash2 = ctx.compute_hash(&flag_key2);

				prop_assert_ne!(hash1, hash2);
			}
		}

		#[test]
		fn different_user_ids_yield_different_hashes(
			user1 in "[a-zA-Z0-9]{1,20}",
			user2 in "[a-zA-Z0-9]{1,20}",
		) {
			// Only test if users are different
			if user1 != user2 {
				let ctx1 = EvaluationContext::new("prod")
					.with_user_id(&user1);
				let ctx2 = EvaluationContext::new("prod")
					.with_user_id(&user2);

				let hash1 = ctx1.compute_hash("test.flag");
				let hash2 = ctx2.compute_hash("test.flag");

				prop_assert_ne!(hash1, hash2);
			}
		}

		#[test]
		fn different_environments_yield_different_hashes(
			env1 in "[a-z][a-z0-9_]{2,20}",
			env2 in "[a-z][a-z0-9_]{2,20}",
		) {
			// Only test if environments are different
			if env1 != env2 {
				let ctx1 = EvaluationContext::new(&env1)
					.with_user_id("testuser");
				let ctx2 = EvaluationContext::new(&env2)
					.with_user_id("testuser");

				let hash1 = ctx1.compute_hash("test.flag");
				let hash2 = ctx2.compute_hash("test.flag");

				prop_assert_ne!(hash1, hash2);
			}
		}
	}

	// Property-based tests for ExposureLog
	proptest! {
		#[test]
		fn exposure_log_creates_unique_ids(
			flag_key in "[a-z][a-z0-9_.]{2,30}",
			variant in "[a-z][a-z0-9_]{1,20}",
		) {
			let flag_id = FlagId::new();
			let env_id = EnvironmentId::new();

			let log1 = ExposureLog::new(
				flag_id,
				&flag_key,
				env_id,
				Some("user1".to_string()),
				None,
				&variant,
				EvaluationReason::Default,
				"hash123",
			);

			let log2 = ExposureLog::new(
				flag_id,
				&flag_key,
				env_id,
				Some("user1".to_string()),
				None,
				&variant,
				EvaluationReason::Default,
				"hash123",
			);

			// Each log should have a unique ID
			prop_assert_ne!(log1.id.0, log2.id.0);
		}

		#[test]
		fn exposure_log_preserves_fields(
			flag_key in "[a-z][a-z0-9_.]{2,30}",
			variant in "[a-z][a-z0-9_]{1,20}",
			user_id in "[a-zA-Z0-9]{1,20}",
			org_id in "[a-zA-Z0-9]{1,20}",
			context_hash in "[a-f0-9]{64}",
		) {
			let flag_id = FlagId::new();
			let env_id = EnvironmentId::new();

			let log = ExposureLog::new(
				flag_id,
				&flag_key,
				env_id,
				Some(user_id.clone()),
				Some(org_id.clone()),
				&variant,
				EvaluationReason::Default,
				&context_hash,
			);

			prop_assert_eq!(log.flag_id, flag_id);
			prop_assert_eq!(log.flag_key, flag_key);
			prop_assert_eq!(log.environment_id, env_id);
			prop_assert_eq!(log.user_id, Some(user_id));
			prop_assert_eq!(log.org_id, Some(org_id));
			prop_assert_eq!(log.variant, variant);
			prop_assert_eq!(log.context_hash, context_hash);
		}
	}
}
