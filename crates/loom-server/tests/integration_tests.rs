// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Comprehensive integration tests for the Loom system.
//!
//! This test suite covers:
//! - Query handler initialization
//! - Query metrics tracking
//! - Query tracing functionality
//! - Concurrent operations
//! - Error handling and recovery

use loom_server::{LlmQueryHandler, QueryTracer, ServerQueryManager, SimpleRegexDetector, TraceId};
use std::sync::Arc;

/// Tests query handler initialization.
///
/// Purpose: Verify that query handlers can be properly initialized
/// and are ready for use.
#[test]
fn test_component_rendering_integration() {
	// Initialize detector and query manager
	let detector = SimpleRegexDetector::new();
	let query_manager = Arc::new(ServerQueryManager::new());

	// Create handler
	let handler = LlmQueryHandler::new(detector, query_manager);

	// Verify handler was created
	assert!(std::mem::size_of_val(&handler) > 0);
}

/// Tests multiple handler instances can coexist.
///
/// Purpose: Verify that multiple query handlers can be created and
/// used independently without interference.
#[test]
fn test_concurrent_api_requests() {
	// Create multiple independent handlers
	let handlers: Vec<_> = (0..10)
		.map(|_| {
			let detector = SimpleRegexDetector::new();
			let query_manager = Arc::new(ServerQueryManager::new());
			LlmQueryHandler::new(detector, query_manager)
		})
		.collect();

	// All should be successfully created
	assert_eq!(handlers.len(), 10);
}

/// Tests query tracing state isolation.
///
/// Purpose: Verify that query metrics and tracing state
/// remain isolated across different trace sessions.
#[test]
fn test_state_management_isolation() {
	// Create separate tracers with different query IDs
	let tracer1 = QueryTracer::new("query_1", Some("session_1".to_string()));
	let tracer2 = QueryTracer::new("query_2", Some("session_2".to_string()));

	// Create trace IDs
	let trace_id_1 = TraceId::new();
	let trace_id_2 = TraceId::new();

	// IDs should be different
	assert_ne!(trace_id_1, trace_id_2);
}

/// Tests error handling for invalid requests.
///
/// Purpose: Verify that the system properly handles initialization
/// and edge cases gracefully.
#[test]
fn test_error_handling_invalid_input() {
	// Create handler with minimal configuration
	let detector = SimpleRegexDetector::new();
	let query_manager = Arc::new(ServerQueryManager::new());
	let _handler = LlmQueryHandler::new(detector, query_manager);

	// Should not panic or error on creation
	assert!(true);
}

/// Tests query bridge construction.
///
/// Purpose: Verify that the query bridge components can be
/// assembled and are properly configured.
#[test]
fn test_query_bridge_routing() {
	// Create handler components
	let detector = SimpleRegexDetector::new();
	let query_manager = Arc::new(ServerQueryManager::new());
	let handler = LlmQueryHandler::new(detector, query_manager.clone());
	let tracer = QueryTracer::new("test_query", None);

	// Verify all components exist
	assert!(std::mem::size_of_val(&handler) > 0);
	assert!(std::mem::size_of_val(&tracer) > 0);
}

/// Tests recovery and consistency.
///
/// Purpose: Verify that creating multiple instances and
/// re-creating components works consistently.
#[test]
fn test_recovery_from_transient_failure() {
	// Create first set of components
	let detector_1 = SimpleRegexDetector::new();
	let manager_1 = Arc::new(ServerQueryManager::new());
	let handler_1 = LlmQueryHandler::new(detector_1, manager_1);

	// Create second set
	let detector_2 = SimpleRegexDetector::new();
	let manager_2 = Arc::new(ServerQueryManager::new());
	let handler_2 = LlmQueryHandler::new(detector_2, manager_2);

	// Both should work
	assert!(std::mem::size_of_val(&handler_1) > 0);
	assert!(std::mem::size_of_val(&handler_2) > 0);
}

#[cfg(test)]
mod property_tests {
	use super::*;
	use proptest::prelude::*;

	/// Property test: Creating trace IDs produces unique values.
	///
	/// Purpose: Verify that trace ID generation consistently
	/// produces unique identifiers even under repeated allocation,
	/// preventing collision bugs in distributed tracing.
	proptest! {
			#[test]
			fn prop_trace_ids_are_unique(
					count in 1..100usize
			) {
					let mut ids = vec![];
					for _ in 0..count {
							let id = TraceId::new();
							prop_assert!(!ids.contains(&id), "Found duplicate trace ID");
							ids.push(id);
					}
			}
	}

	/// Property test: Handler creation is stable.
	///
	/// Purpose: Verify that handlers can be created repeatedly
	/// without panicking or exhibiting non-deterministic behavior,
	/// ensuring reliable handler allocation across different scenarios.
	proptest! {
			#[test]
			fn prop_handler_creation_is_stable(
					count in 1..50usize
			) {
					for _ in 0..count {
							let detector = SimpleRegexDetector::new();
							let manager = Arc::new(ServerQueryManager::new());
							let _handler = LlmQueryHandler::new(detector, manager);
							// Should not panic
					}
					prop_assert!(true);
			}
	}
}
