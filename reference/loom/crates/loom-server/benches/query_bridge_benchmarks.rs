// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Performance benchmarks for the ServerQueryManager
//!
//! This benchmark suite measures:
//! - Latency: Query send + response cycle
//! - Throughput: Queries per second
//! - Memory: Allocations per query
//! - Serialization: JSON encoding/decoding
//!
//! Why these benchmarks are important:
//! - Latency benchmarks ensure query roundtrip stays under 200ms SLA
//! - Throughput benchmarks validate scalability for concurrent clients
//! - Serialization benchmarks track JSON overhead
//! - Manager operations ensure storage/retrieval is efficient

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use loom_common_core::server_query::{
	ServerQuery, ServerQueryKind, ServerQueryResponse, ServerQueryResult,
};
use loom_server::ServerQueryManager;
use std::sync::Arc;
use uuid::Uuid;

/// Generate a unique query ID
fn generate_query_id() -> String {
	format!("Q-{}", &Uuid::new_v4().to_string().replace("-", "")[0..32])
}

/// Generate a test query
fn generate_query(kind: ServerQueryKind) -> ServerQuery {
	ServerQuery {
		id: generate_query_id(),
		kind,
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 30,
		metadata: serde_json::json!({}),
	}
}

/// Generate a test response
fn generate_response(query_id: String, result: ServerQueryResult) -> ServerQueryResponse {
	ServerQueryResponse {
		query_id,
		sent_at: chrono::Utc::now().to_rfc3339(),
		result,
		error: None,
	}
}

// ============================================================================
// LATENCY BENCHMARKS: Query send + response cycle
// ============================================================================

/// Benchmark single query roundtrip latency
///
/// Why important:
/// - Validates sub-200ms latency SLA for typical queries
/// - Different payload sizes simulate real-world scenarios
fn bench_single_query_latency(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("latency_single_query");
	group.sample_size(20);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("small_readfile_query", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let query = generate_query(ServerQueryKind::ReadFile {
					path: "/etc/passwd".to_string(),
				});
				let query_id = query.id.clone();

				let manager_clone = manager.clone();
				let response_task = tokio::spawn(async move {
					tokio::time::sleep(std::time::Duration::from_millis(1)).await;
					manager_clone
						.receive_response(generate_response(
							query_id,
							ServerQueryResult::FileContent("content".to_string()),
						))
						.await;
				});

				let result = manager.send_query("session-1", query).await;
				let _ = response_task.await;
				black_box(result)
			})
		})
	});

	group.bench_function("large_readfile_query", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let large_content = "x".repeat(100_000); // 100KB
				let query = generate_query(ServerQueryKind::ReadFile {
					path: format!("/var/log/{}", "a".repeat(100)),
				});
				let query_id = query.id.clone();

				let manager_clone = manager.clone();
				let response_task = tokio::spawn(async move {
					tokio::time::sleep(std::time::Duration::from_millis(2)).await;
					manager_clone
						.receive_response(generate_response(
							query_id,
							ServerQueryResult::FileContent(large_content),
						))
						.await;
				});

				let result = manager.send_query("session-1", query).await;
				let _ = response_task.await;
				black_box(result)
			})
		})
	});

	group.finish();
}

/// Benchmark concurrent query handling
///
/// Why important:
/// - Ensures manager can handle 10 simultaneous queries efficiently
/// - Tests broadcast channel scalability
fn bench_concurrent_queries(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("latency_concurrent");
	group.sample_size(10);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("concurrent_10_queries", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let mut handles = vec![];

				for i in 0..10 {
					let manager_clone = manager.clone();
					let query = generate_query(ServerQueryKind::ReadFile {
						path: format!("/file{i}.txt"),
					});
					let query_id = query.id.clone();

					let response_handle = tokio::spawn(async move {
						tokio::time::sleep(std::time::Duration::from_millis(1)).await;
						manager_clone
							.receive_response(generate_response(
								query_id,
								ServerQueryResult::FileContent(format!("content{i}")),
							))
							.await;
					});

					let query_handle = tokio::spawn({
						let manager_clone = manager.clone();
						async move {
							manager_clone
								.send_query(&format!("session-{i}"), query)
								.await
						}
					});

					handles.push((query_handle, response_handle));
				}

				let mut results = vec![];
				for (query_h, response_h) in handles {
					let _ = response_h.await;
					results.push(query_h.await);
				}
				black_box(results)
			})
		})
	});

	group.finish();
}

// ============================================================================
// THROUGHPUT BENCHMARKS: Queries per second
// ============================================================================

/// Benchmark single query throughput
///
/// Why important:
/// - Baseline for best-case throughput (single query in isolation)
/// - Measures overhead of manager infrastructure
fn bench_throughput_single_query(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("throughput_single");
	group.throughput(Throughput::Elements(1));
	group.sample_size(20);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("query_roundtrip", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let query = generate_query(ServerQueryKind::ReadFile {
					path: "/test/file.txt".to_string(),
				});
				let query_id = query.id.clone();

				let manager_clone = manager.clone();
				tokio::spawn(async move {
					tokio::time::sleep(std::time::Duration::from_millis(1)).await;
					manager_clone
						.receive_response(generate_response(
							query_id,
							ServerQueryResult::FileContent("data".to_string()),
						))
						.await;
				});

				black_box(manager.send_query("session", query).await)
			})
		})
	});

	group.finish();
}

/// Benchmark sustained multi-session load
///
/// Why important:
/// - Validates handling multiple concurrent sessions (10 sessions × 10 queries)
/// - Measures throughput under realistic sustained load
fn bench_throughput_sustained_load(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("throughput_sustained");
	group.throughput(Throughput::Elements(100));
	group.sample_size(10);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("sustained_10_sessions_10_queries", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let mut handles = vec![];

				for session in 0..10 {
					for i in 0..10 {
						let manager_clone = manager.clone();
						let query = generate_query(ServerQueryKind::ReadFile {
							path: format!("/file{i}.txt"),
						});
						let query_id = query.id.clone();

						let response_handle = tokio::spawn(async move {
							tokio::time::sleep(std::time::Duration::from_millis(2)).await;
							manager_clone
								.receive_response(generate_response(
									query_id,
									ServerQueryResult::FileContent("data".to_string()),
								))
								.await;
						});

						let query_handle = tokio::spawn({
							let manager_clone = manager.clone();
							let session_id = format!("session-{session}");
							async move { manager_clone.send_query(&session_id, query).await }
						});

						handles.push((query_handle, response_handle));
					}
				}

				let mut results = vec![];
				for (query_h, response_h) in handles {
					let _ = response_h.await;
					results.push(query_h.await);
				}
				black_box(results)
			})
		})
	});

	group.finish();
}

/// Benchmark burst query load
///
/// Why important:
/// - Tests manager performance under spike loads (100 rapid queries)
/// - Validates broadcast channel handling under pressure
fn bench_throughput_burst_load(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("throughput_burst");
	group.throughput(Throughput::Elements(100));
	group.sample_size(10);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("burst_100_queries_rapid", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let mut handles = vec![];

				// Fire 100 queries immediately
				for i in 0..100 {
					let manager_clone = manager.clone();
					let query = generate_query(ServerQueryKind::ReadFile {
						path: format!("/file{}.txt", i % 10),
					});
					let query_id = query.id.clone();

					let response_handle = tokio::spawn(async move {
						tokio::time::sleep(std::time::Duration::from_millis(1)).await;
						manager_clone
							.receive_response(generate_response(
								query_id,
								ServerQueryResult::FileContent("data".to_string()),
							))
							.await;
					});

					let query_handle = tokio::spawn({
						let manager_clone = manager.clone();
						async move { manager_clone.send_query("session", query).await }
					});

					handles.push((query_handle, response_handle));
				}

				let mut results = vec![];
				for (query_h, response_h) in handles {
					let _ = response_h.await;
					results.push(query_h.await);
				}
				black_box(results)
			})
		})
	});

	group.finish();
}

// ============================================================================
// SERIALIZATION BENCHMARKS: JSON encoding/decoding
// ============================================================================

/// Benchmark JSON serialization/deserialization
///
/// Why important:
/// - Validates serialization overhead is negligible
/// - Different payload sizes test scaling behavior
/// - JSON is used for HTTP/WebSocket transport
fn bench_serialization(c: &mut Criterion) {
	let mut group = c.benchmark_group("serialization");
	group.sample_size(30);
	group.measurement_time(std::time::Duration::from_secs(5));

	let small_query = generate_query(ServerQueryKind::ReadFile {
		path: "/etc/passwd".to_string(),
	});

	let large_query = generate_query(ServerQueryKind::Custom {
		name: "custom".to_string(),
		payload: serde_json::json!({
				"data": "x".repeat(100_000)
		}),
	});

	group.bench_function("small_query_serialize", |b| {
		b.iter(|| serde_json::to_string(&black_box(&small_query)).unwrap())
	});

	group.bench_function("large_query_serialize", |b| {
		b.iter(|| serde_json::to_string(&black_box(&large_query)).unwrap())
	});

	let small_json = serde_json::to_string(&small_query).unwrap();
	let large_json = serde_json::to_string(&large_query).unwrap();

	group.bench_function("small_query_deserialize", |b| {
		b.iter(|| serde_json::from_str::<ServerQuery>(black_box(&small_json)).unwrap())
	});

	group.bench_function("large_query_deserialize", |b| {
		b.iter(|| serde_json::from_str::<ServerQuery>(black_box(&large_json)).unwrap())
	});

	// Response serialization/deserialization
	let small_response = generate_response(
		small_query.id.clone(),
		ServerQueryResult::FileContent("small content".to_string()),
	);

	let large_response = generate_response(
		large_query.id.clone(),
		ServerQueryResult::FileContent("x".repeat(100_000)),
	);

	group.bench_function("small_response_serialize", |b| {
		b.iter(|| serde_json::to_string(&black_box(&small_response)).unwrap())
	});

	group.bench_function("large_response_serialize", |b| {
		b.iter(|| serde_json::to_string(&black_box(&large_response)).unwrap())
	});

	let small_resp_json = serde_json::to_string(&small_response).unwrap();
	let large_resp_json = serde_json::to_string(&large_response).unwrap();

	group.bench_function("small_response_deserialize", |b| {
		b.iter(|| serde_json::from_str::<ServerQueryResponse>(black_box(&small_resp_json)).unwrap())
	});

	group.bench_function("large_response_deserialize", |b| {
		b.iter(|| serde_json::from_str::<ServerQueryResponse>(black_box(&large_resp_json)).unwrap())
	});

	group.finish();
}

// ============================================================================
// MANAGER OPERATIONS BENCHMARKS: Store/retrieve operations
// ============================================================================

/// Benchmark manager storage operations
///
/// Why important:
/// - Validates HashMap lookup/insert overhead is minimal
/// - Tests lock contention on Mutex operations
/// - Ensures response retrieval is O(1)
fn bench_manager_operations(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("manager_operations");
	group.sample_size(20);
	group.measurement_time(std::time::Duration::from_secs(5));

	group.bench_function("list_pending_empty", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				black_box(manager.list_pending("session").await)
			})
		})
	});

	group.bench_function("get_response", |b| {
		b.iter(|| {
			rt.block_on(async {
				let manager = Arc::new(ServerQueryManager::new());
				let query = generate_query(ServerQueryKind::ReadFile {
					path: "/test.txt".to_string(),
				});
				let query_id = query.id.clone();

				manager
					.receive_response(generate_response(
						query_id.clone(),
						ServerQueryResult::FileContent("data".to_string()),
					))
					.await;

				black_box(manager.get_response(&query_id).await)
			})
		})
	});

	group.finish();
}

// ============================================================================
// QUERY TYPES BENCHMARKS: Different query kind overheads
// ============================================================================

/// Benchmark different query types
///
/// Why important:
/// - Validates overhead is consistent across query types
/// - Tests serialization overhead for complex payloads
/// - Ensures no type-specific bottlenecks
fn bench_query_types(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("query_types");
	group.sample_size(10);
	group.measurement_time(std::time::Duration::from_secs(5));

	let query_kinds = [
		(
			"readfile",
			ServerQueryKind::ReadFile {
				path: "/etc/passwd".to_string(),
			},
		),
		(
			"execute_command",
			ServerQueryKind::ExecuteCommand {
				command: "ls".to_string(),
				args: vec!["-la".to_string()],
				timeout_secs: 10,
			},
		),
		(
			"user_input",
			ServerQueryKind::RequestUserInput {
				prompt: "Enter value:".to_string(),
				input_type: "text".to_string(),
				options: None,
			},
		),
		(
			"environment",
			ServerQueryKind::GetEnvironment {
				keys: vec!["PATH".to_string(), "HOME".to_string()],
			},
		),
	];

	for (name, kind) in query_kinds.iter() {
		group.bench_with_input(BenchmarkId::from_parameter(name), name, |b, _| {
			b.iter(|| {
				rt.block_on(async {
					let kind_clone = kind.clone();
					let manager = Arc::new(ServerQueryManager::new());
					let query = generate_query(kind_clone);
					let query_id = query.id.clone();

					let manager_clone = manager.clone();
					tokio::spawn(async move {
						tokio::time::sleep(std::time::Duration::from_millis(1)).await;
						manager_clone
							.receive_response(generate_response(
								query_id,
								ServerQueryResult::FileContent("result".to_string()),
							))
							.await;
					});

					black_box(manager.send_query("session", query).await)
				})
			})
		});
	}

	group.finish();
}

criterion_group!(
	benches,
	bench_single_query_latency,
	bench_concurrent_queries,
	bench_throughput_single_query,
	bench_throughput_sustained_load,
	bench_throughput_burst_load,
	bench_serialization,
	bench_manager_operations,
	bench_query_types,
);

criterion_main!(benches);
