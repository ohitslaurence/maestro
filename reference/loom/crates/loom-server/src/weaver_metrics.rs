// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Prometheus metrics for weaver provisioning.

use prometheus::{Counter, Gauge};

/// Prometheus metrics for weaver provisioning operations.
pub struct WeaverMetrics {
	/// Total number of weavers created.
	pub weavers_created_total: Counter,
	/// Total number of weavers deleted.
	pub weavers_deleted_total: Counter,
	/// Total number of weavers that failed to start.
	pub weavers_failed_total: Counter,
	/// Total number of cleanup runs completed.
	pub weavers_cleanup_total: Counter,
	/// Total number of weavers deleted by cleanup.
	pub weavers_cleanup_deleted_total: Counter,
	/// Current number of active weavers.
	pub weavers_active: Gauge,
}

impl Default for WeaverMetrics {
	fn default() -> Self {
		Self::new()
	}
}

impl WeaverMetrics {
	/// Create new weaver metrics and register them with the default registry.
	pub fn new() -> Self {
		let weavers_created_total = Counter::new(
			"loom_weavers_created_total",
			"Total number of weavers provisioned",
		)
		.expect("Failed to create weavers_created_total counter");

		let weavers_deleted_total = Counter::new(
			"loom_weavers_deleted_total",
			"Total number of weavers deleted (manual + cleanup)",
		)
		.expect("Failed to create weavers_deleted_total counter");

		let weavers_failed_total = Counter::new(
			"loom_weavers_failed_total",
			"Total number of weavers that entered failed state",
		)
		.expect("Failed to create weavers_failed_total counter");

		let weavers_cleanup_total = Counter::new(
			"loom_weavers_cleanup_total",
			"Total number of cleanup runs completed",
		)
		.expect("Failed to create weavers_cleanup_total counter");

		let weavers_cleanup_deleted_total = Counter::new(
			"loom_weavers_cleanup_deleted_total",
			"Total number of weavers deleted by cleanup",
		)
		.expect("Failed to create weavers_cleanup_deleted_total counter");

		let weavers_active = Gauge::new("loom_weavers_active", "Current number of running weavers")
			.expect("Failed to create weavers_active gauge");

		let registry = prometheus::default_registry();

		registry
			.register(Box::new(weavers_created_total.clone()))
			.ok();
		registry
			.register(Box::new(weavers_deleted_total.clone()))
			.ok();
		registry
			.register(Box::new(weavers_failed_total.clone()))
			.ok();
		registry
			.register(Box::new(weavers_cleanup_total.clone()))
			.ok();
		registry
			.register(Box::new(weavers_cleanup_deleted_total.clone()))
			.ok();
		registry.register(Box::new(weavers_active.clone())).ok();

		Self {
			weavers_created_total,
			weavers_deleted_total,
			weavers_failed_total,
			weavers_cleanup_total,
			weavers_cleanup_deleted_total,
			weavers_active,
		}
	}

	/// Increment the created counter.
	pub fn inc_created(&self) {
		self.weavers_created_total.inc();
	}

	/// Increment the deleted counter.
	pub fn inc_deleted(&self) {
		self.weavers_deleted_total.inc();
	}

	/// Increment the failed counter.
	pub fn inc_failed(&self) {
		self.weavers_failed_total.inc();
	}

	/// Increment the cleanup counter.
	pub fn inc_cleanup(&self) {
		self.weavers_cleanup_total.inc();
	}

	/// Increment the cleanup deleted counter by the given amount.
	pub fn inc_cleanup_deleted(&self, count: u64) {
		self.weavers_cleanup_deleted_total.inc_by(count as f64);
	}

	/// Set the active weavers gauge.
	pub fn set_active(&self, count: u64) {
		self.weavers_active.set(count as f64);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_weaver_metrics_creation() {
		let metrics = WeaverMetrics::new();
		assert_eq!(metrics.weavers_created_total.get(), 0.0);
		assert_eq!(metrics.weavers_active.get(), 0.0);
	}

	#[test]
	fn test_inc_created() {
		let metrics = WeaverMetrics::new();
		metrics.inc_created();
		assert_eq!(metrics.weavers_created_total.get(), 1.0);
	}

	#[test]
	fn test_set_active() {
		let metrics = WeaverMetrics::new();
		metrics.set_active(5);
		assert_eq!(metrics.weavers_active.get(), 5.0);
	}
}
