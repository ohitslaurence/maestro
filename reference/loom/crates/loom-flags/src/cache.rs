// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Local in-memory cache for feature flag states.
//!
//! The cache stores the current state of all flags and kill switches,
//! enabling fast local evaluation and offline mode support.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use loom_flags_core::{FlagState, KillSwitchState};
use tokio::sync::RwLock;

/// In-memory cache for feature flag states.
///
/// The cache is thread-safe and supports concurrent reads with exclusive writes.
/// It stores both flag states and kill switch states, along with metadata about
/// when the cache was last updated.
#[derive(Debug)]
pub struct FlagCache {
	inner: Arc<RwLock<CacheInner>>,
}

#[derive(Debug, Default)]
struct CacheInner {
	/// Cached flag states keyed by flag key.
	flags: HashMap<String, FlagState>,
	/// Cached kill switch states keyed by kill switch key.
	kill_switches: HashMap<String, KillSwitchState>,
	/// When the cache was last updated.
	last_updated: Option<DateTime<Utc>>,
	/// Whether the cache has been initialized.
	initialized: bool,
}

impl FlagCache {
	/// Creates a new empty cache.
	pub fn new() -> Self {
		Self {
			inner: Arc::new(RwLock::new(CacheInner::default())),
		}
	}

	/// Returns true if the cache has been initialized with data.
	pub async fn is_initialized(&self) -> bool {
		self.inner.read().await.initialized
	}

	/// Returns the timestamp of the last cache update.
	pub async fn last_updated(&self) -> Option<DateTime<Utc>> {
		self.inner.read().await.last_updated
	}

	/// Initializes the cache with a full set of flags and kill switches.
	///
	/// This is typically called when receiving an `init` event from SSE.
	pub async fn initialize(&self, flags: Vec<FlagState>, kill_switches: Vec<KillSwitchState>) {
		let mut inner = self.inner.write().await;

		inner.flags.clear();
		for flag in flags {
			inner.flags.insert(flag.key.clone(), flag);
		}

		inner.kill_switches.clear();
		for ks in kill_switches {
			inner.kill_switches.insert(ks.key.clone(), ks);
		}

		inner.last_updated = Some(Utc::now());
		inner.initialized = true;
	}

	/// Gets a flag state by key.
	pub async fn get_flag(&self, key: &str) -> Option<FlagState> {
		self.inner.read().await.flags.get(key).cloned()
	}

	/// Gets all cached flag states.
	pub async fn get_all_flags(&self) -> Vec<FlagState> {
		self.inner.read().await.flags.values().cloned().collect()
	}

	/// Updates a single flag state.
	pub async fn update_flag(&self, flag: FlagState) {
		let mut inner = self.inner.write().await;
		inner.flags.insert(flag.key.clone(), flag);
		inner.last_updated = Some(Utc::now());
	}

	/// Marks a flag as archived.
	pub async fn archive_flag(&self, key: &str) {
		let mut inner = self.inner.write().await;
		if let Some(flag) = inner.flags.get_mut(key) {
			flag.archived = true;
		}
		inner.last_updated = Some(Utc::now());
	}

	/// Marks a flag as restored (unarchived) with updated enabled status.
	pub async fn restore_flag(&self, key: &str, enabled: bool) {
		let mut inner = self.inner.write().await;
		if let Some(flag) = inner.flags.get_mut(key) {
			flag.archived = false;
			flag.enabled = enabled;
		}
		inner.last_updated = Some(Utc::now());
	}

	/// Updates the enabled status of a flag.
	pub async fn update_flag_enabled(&self, key: &str, enabled: bool) {
		let mut inner = self.inner.write().await;
		if let Some(flag) = inner.flags.get_mut(key) {
			flag.enabled = enabled;
		}
		inner.last_updated = Some(Utc::now());
	}

	/// Gets a kill switch state by key.
	pub async fn get_kill_switch(&self, key: &str) -> Option<KillSwitchState> {
		self.inner.read().await.kill_switches.get(key).cloned()
	}

	/// Gets all cached kill switch states.
	pub async fn get_all_kill_switches(&self) -> Vec<KillSwitchState> {
		self
			.inner
			.read()
			.await
			.kill_switches
			.values()
			.cloned()
			.collect()
	}

	/// Activates a kill switch.
	pub async fn activate_kill_switch(&self, key: &str, reason: &str) {
		let mut inner = self.inner.write().await;
		if let Some(ks) = inner.kill_switches.get_mut(key) {
			ks.is_active = true;
			ks.activation_reason = Some(reason.to_string());
		}
		inner.last_updated = Some(Utc::now());
	}

	/// Deactivates a kill switch.
	pub async fn deactivate_kill_switch(&self, key: &str) {
		let mut inner = self.inner.write().await;
		if let Some(ks) = inner.kill_switches.get_mut(key) {
			ks.is_active = false;
			ks.activation_reason = None;
		}
		inner.last_updated = Some(Utc::now());
	}

	/// Checks if any active kill switch affects the given flag.
	pub async fn is_flag_killed(&self, flag_key: &str) -> Option<String> {
		let inner = self.inner.read().await;
		for ks in inner.kill_switches.values() {
			if ks.is_active && ks.linked_flag_keys.contains(&flag_key.to_string()) {
				return Some(ks.key.clone());
			}
		}
		None
	}

	/// Returns the number of cached flags.
	pub async fn flag_count(&self) -> usize {
		self.inner.read().await.flags.len()
	}

	/// Returns the number of cached kill switches.
	pub async fn kill_switch_count(&self) -> usize {
		self.inner.read().await.kill_switches.len()
	}

	/// Clears all cached data.
	pub async fn clear(&self) {
		let mut inner = self.inner.write().await;
		inner.flags.clear();
		inner.kill_switches.clear();
		inner.last_updated = None;
		inner.initialized = false;
	}
}

impl Default for FlagCache {
	fn default() -> Self {
		Self::new()
	}
}

impl Clone for FlagCache {
	fn clone(&self) -> Self {
		Self {
			inner: Arc::clone(&self.inner),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_flags_core::{FlagId, KillSwitchId, VariantValue};

	fn create_test_flag(key: &str, enabled: bool) -> FlagState {
		FlagState {
			key: key.to_string(),
			id: FlagId::new(),
			enabled,
			default_variant: "on".to_string(),
			default_value: VariantValue::Boolean(true),
			archived: false,
		}
	}

	fn create_test_kill_switch(key: &str, linked_keys: Vec<String>, active: bool) -> KillSwitchState {
		KillSwitchState {
			key: key.to_string(),
			id: KillSwitchId::new(),
			is_active: active,
			linked_flag_keys: linked_keys,
			activation_reason: if active {
				Some("Testing".to_string())
			} else {
				None
			},
		}
	}

	#[tokio::test]
	async fn test_initialize_cache() {
		let cache = FlagCache::new();
		assert!(!cache.is_initialized().await);

		let flags = vec![
			create_test_flag("feature.test", true),
			create_test_flag("feature.other", false),
		];
		let kill_switches = vec![create_test_kill_switch(
			"emergency",
			vec!["feature.test".to_string()],
			false,
		)];

		cache.initialize(flags, kill_switches).await;

		assert!(cache.is_initialized().await);
		assert_eq!(cache.flag_count().await, 2);
		assert_eq!(cache.kill_switch_count().await, 1);
		assert!(cache.last_updated().await.is_some());
	}

	#[tokio::test]
	async fn test_get_flag() {
		let cache = FlagCache::new();
		cache
			.initialize(vec![create_test_flag("feature.test", true)], vec![])
			.await;

		let flag = cache.get_flag("feature.test").await;
		assert!(flag.is_some());
		assert_eq!(flag.unwrap().key, "feature.test");

		let missing = cache.get_flag("nonexistent").await;
		assert!(missing.is_none());
	}

	#[tokio::test]
	async fn test_update_flag() {
		let cache = FlagCache::new();
		cache
			.initialize(vec![create_test_flag("feature.test", false)], vec![])
			.await;

		let updated = create_test_flag("feature.test", true);
		cache.update_flag(updated).await;

		let flag = cache.get_flag("feature.test").await.unwrap();
		assert!(flag.enabled);
	}

	#[tokio::test]
	async fn test_archive_and_restore_flag() {
		let cache = FlagCache::new();
		cache
			.initialize(vec![create_test_flag("feature.test", true)], vec![])
			.await;

		cache.archive_flag("feature.test").await;
		let flag = cache.get_flag("feature.test").await.unwrap();
		assert!(flag.archived);

		cache.restore_flag("feature.test", true).await;
		let flag = cache.get_flag("feature.test").await.unwrap();
		assert!(!flag.archived);
		assert!(flag.enabled);
	}

	#[tokio::test]
	async fn test_kill_switch_affects_flag() {
		let cache = FlagCache::new();
		let ks = create_test_kill_switch("emergency", vec!["feature.dangerous".to_string()], true);
		cache.initialize(vec![], vec![ks]).await;

		// Flag is killed by active kill switch
		let killed = cache.is_flag_killed("feature.dangerous").await;
		assert_eq!(killed, Some("emergency".to_string()));

		// Unrelated flag is not killed
		let not_killed = cache.is_flag_killed("feature.safe").await;
		assert!(not_killed.is_none());
	}

	#[tokio::test]
	async fn test_deactivate_kill_switch() {
		let cache = FlagCache::new();
		let ks = create_test_kill_switch("emergency", vec!["feature.test".to_string()], true);
		cache.initialize(vec![], vec![ks]).await;

		assert!(cache.is_flag_killed("feature.test").await.is_some());

		cache.deactivate_kill_switch("emergency").await;

		assert!(cache.is_flag_killed("feature.test").await.is_none());
	}

	#[tokio::test]
	async fn test_clear_cache() {
		let cache = FlagCache::new();
		cache
			.initialize(
				vec![create_test_flag("feature.test", true)],
				vec![create_test_kill_switch("ks", vec![], false)],
			)
			.await;

		assert!(cache.is_initialized().await);

		cache.clear().await;

		assert!(!cache.is_initialized().await);
		assert_eq!(cache.flag_count().await, 0);
		assert_eq!(cache.kill_switch_count().await, 0);
	}

	#[tokio::test]
	async fn test_clone_shares_state() {
		let cache = FlagCache::new();
		let cache_clone = cache.clone();

		cache
			.initialize(vec![create_test_flag("feature.test", true)], vec![])
			.await;

		// Clone should see the same data
		assert!(cache_clone.is_initialized().await);
		assert_eq!(cache_clone.flag_count().await, 1);
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use loom_flags_core::{FlagId, KillSwitchId, VariantValue};
	use proptest::prelude::*;

	fn arb_flag_state() -> impl Strategy<Value = FlagState> {
		(
			"[a-z][a-z0-9_.]{2,30}",
			proptest::bool::ANY,
			proptest::bool::ANY,
		)
			.prop_map(|(key, enabled, archived)| FlagState {
				key,
				id: FlagId::new(),
				enabled,
				default_variant: "default".to_string(),
				default_value: VariantValue::Boolean(enabled),
				archived,
			})
	}

	fn arb_kill_switch_state() -> impl Strategy<Value = KillSwitchState> {
		(
			"[a-z][a-z0-9_]{2,20}",
			prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 0..5),
			proptest::bool::ANY,
		)
			.prop_map(|(key, linked, active)| KillSwitchState {
				key,
				id: KillSwitchId::new(),
				is_active: active,
				linked_flag_keys: linked,
				activation_reason: if active {
					Some("Testing".to_string())
				} else {
					None
				},
			})
	}

	proptest! {
		#[test]
		fn cache_preserves_all_flags(flags in prop::collection::vec(arb_flag_state(), 1..20)) {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let cache = FlagCache::new();
				let flag_count = flags.len();

				cache.initialize(flags.clone(), vec![]).await;

				prop_assert!(cache.is_initialized().await);
				prop_assert_eq!(cache.flag_count().await, flag_count);

				for flag in &flags {
					let cached = cache.get_flag(&flag.key).await;
					prop_assert!(cached.is_some());
					let cached = cached.unwrap();
					prop_assert_eq!(&cached.key, &flag.key);
					prop_assert_eq!(cached.enabled, flag.enabled);
				}

				Ok(())
			})?;
		}

		#[test]
		fn cache_preserves_all_kill_switches(
			kill_switches in prop::collection::vec(arb_kill_switch_state(), 1..10)
		) {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let cache = FlagCache::new();
				let ks_count = kill_switches.len();

				cache.initialize(vec![], kill_switches.clone()).await;

				prop_assert_eq!(cache.kill_switch_count().await, ks_count);

				for ks in &kill_switches {
					let cached = cache.get_kill_switch(&ks.key).await;
					prop_assert!(cached.is_some());
					let cached = cached.unwrap();
					prop_assert_eq!(&cached.key, &ks.key);
					prop_assert_eq!(cached.is_active, ks.is_active);
				}

				Ok(())
			})?;
		}

		#[test]
		fn active_kill_switch_affects_linked_flags(
			flag_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 1..10),
			ks_key in "[a-z][a-z0-9_]{2,20}",
		) {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let cache = FlagCache::new();

				let ks = KillSwitchState {
					key: ks_key,
					id: KillSwitchId::new(),
					is_active: true,
					linked_flag_keys: flag_keys.clone(),
					activation_reason: Some("Testing".to_string()),
				};

				cache.initialize(vec![], vec![ks]).await;

				for flag_key in &flag_keys {
					let killed = cache.is_flag_killed(flag_key).await;
					prop_assert!(killed.is_some());
				}

				Ok(())
			})?;
		}

		#[test]
		fn inactive_kill_switch_does_not_affect_flags(
			flag_keys in prop::collection::vec("[a-z][a-z0-9_.]{2,20}", 1..10),
			ks_key in "[a-z][a-z0-9_]{2,20}",
		) {
			let rt = tokio::runtime::Runtime::new().unwrap();
			rt.block_on(async {
				let cache = FlagCache::new();

				let ks = KillSwitchState {
					key: ks_key,
					id: KillSwitchId::new(),
					is_active: false,
					linked_flag_keys: flag_keys.clone(),
					activation_reason: None,
				};

				cache.initialize(vec![], vec![ks]).await;

				for flag_key in &flag_keys {
					let killed = cache.is_flag_killed(flag_key).await;
					prop_assert!(killed.is_none());
				}

				Ok(())
			})?;
		}
	}
}
