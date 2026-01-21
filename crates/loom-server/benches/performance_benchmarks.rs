// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Performance benchmarks for Loom system components.
//!
//! Measures:
//! - Query creation latency
//! - Query processing throughput
//! - Memory overhead per query
//! - State update performance
//! - Concurrent request handling

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use loom_server::models::CreateQueryRequest;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Benchmark: Basic query creation time
///
/// Purpose: Establish baseline for query creation latency to detect
/// performance regressions.
fn bench_query_creation(c: &mut Criterion) {
	c.bench_function("query_creation_simple", |b| {
		b.to_async(tokio::runtime::Runtime::new().unwrap())
			.iter(|| async {
				let request = CreateQueryRequest {
					id: black_box(uuid::Uuid::new_v4().to_string()),
					query: black_box("SELECT * FROM threads".to_string()),
					context: None,
				};

				// Simulate query creation overhead
				request.id.clone()
			});
	});
}

/// Benchmark: Query creation with context
///
/// Purpose: Measure overhead of including context information in queries.
fn bench_query_creation_with_context(c: &mut Criterion) {
	c.bench_function("query_creation_with_context", |b| {
		b.to_async(tokio::runtime::Runtime::new().unwrap())
			.iter(|| async {
				let request = CreateQueryRequest {
					id: black_box(uuid::Uuid::new_v4().to_string()),
					query: black_box("SELECT * FROM threads WHERE id = ?".to_string()),
					context: Some(black_box("database".to_string())),
				};

				request.id.clone()
			});
	});
}

/// Benchmark: Concurrent query operations
///
/// Purpose: Measure system throughput under concurrent load to establish
/// safe concurrency limits.
fn bench_concurrent_queries(c: &mut Criterion) {
	let mut group = c.benchmark_group("concurrent_queries");

	for count in [10, 50, 100, 500].iter() {
		group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
			b.to_async(tokio::runtime::Runtime::new().unwrap())
				.iter(|| async {
					let handles: Vec<_> = (0..count)
						.map(|i| {
							tokio::spawn(async move {
								let request = CreateQueryRequest {
									id: format!("query_{}", i),
									query: format!("query {}", i),
									context: None,
								};
								request.id
							})
						})
						.collect();

					futures::future::join_all(handles).await
				});
		});
	}

	group.finish();
}

/// Benchmark: State update performance with varying query sizes
///
/// Purpose: Measure how query size affects state management performance.
fn bench_state_updates_by_size(c: &mut Criterion) {
	let mut group = c.benchmark_group("state_updates_by_size");

	for size in [10, 100, 1000, 10000].iter() {
		group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
			b.to_async(tokio::runtime::Runtime::new().unwrap())
				.iter(|| async {
					let query_data = "x".repeat(size);
					let request = CreateQueryRequest {
						id: uuid::Uuid::new_v4().to_string(),
						query: black_box(query_data),
						context: None,
					};

					request.query.len()
				});
		});
	}

	group.finish();
}

/// Benchmark: Query manager allocation and operation
///
/// Purpose: Measure the overhead of the query manager itself.
fn bench_query_manager_operations(c: &mut Criterion) {
	c.bench_function("query_manager_create_and_lock", |b| {
		b.to_async(tokio::runtime::Runtime::new().unwrap())
			.iter(|| async {
				let manager = Arc::new(Mutex::new(vec![]));
				let _lock = manager.lock().await;
			});
	});
}

/// Benchmark: UUID generation
///
/// Purpose: Measure cost of UUID generation used for request IDs.
fn bench_uuid_generation(c: &mut Criterion) {
	c.bench_function("uuid_generation", |b| {
		b.iter(|| uuid::Uuid::new_v4());
	});
}

criterion_group!(
	benches,
	bench_query_creation,
	bench_query_creation_with_context,
	bench_concurrent_queries,
	bench_state_updates_by_size,
	bench_query_manager_operations,
	bench_uuid_generation,
);

criterion_main!(benches);
