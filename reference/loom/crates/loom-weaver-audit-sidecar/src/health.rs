// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
	Healthy,
	Degraded,
	Unhealthy,
}

#[derive(Debug, Clone, Serialize)]
pub struct EbpfHealth {
	pub status: HealthStatus,
	pub programs_attached: u32,
	pub programs_expected: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerHealth {
	pub status: HealthStatus,
	pub last_successful_send: Option<String>,
	pub buffered_events: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BufferHealth {
	pub status: HealthStatus,
	pub size_bytes: u64,
	pub utilization_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
	pub ebpf: EbpfHealth,
	pub server: ServerHealth,
	pub buffer: BufferHealth,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
	pub status: HealthStatus,
	pub components: ComponentHealth,
}

#[derive(Clone)]
pub struct HealthState {
	inner: Arc<RwLock<HealthStateInner>>,
}

struct HealthStateInner {
	ebpf_programs_attached: u32,
	ebpf_programs_expected: u32,
	last_successful_send: Option<chrono::DateTime<chrono::Utc>>,
	buffered_events: u64,
	buffer_size_bytes: u64,
	buffer_max_bytes: u64,
}

impl HealthState {
	pub fn new(buffer_max_bytes: u64) -> Self {
		HealthState {
			inner: Arc::new(RwLock::new(HealthStateInner {
				ebpf_programs_attached: 0,
				ebpf_programs_expected: 5,
				last_successful_send: None,
				buffered_events: 0,
				buffer_size_bytes: 0,
				buffer_max_bytes,
			})),
		}
	}

	pub async fn set_ebpf_status(&self, attached: u32, expected: u32) {
		let mut inner = self.inner.write().await;
		inner.ebpf_programs_attached = attached;
		inner.ebpf_programs_expected = expected;
	}

	pub async fn record_successful_send(&self) {
		let mut inner = self.inner.write().await;
		inner.last_successful_send = Some(chrono::Utc::now());
	}

	pub async fn set_buffer_status(&self, events: u64, size_bytes: u64) {
		let mut inner = self.inner.write().await;
		inner.buffered_events = events;
		inner.buffer_size_bytes = size_bytes;
	}

	async fn get_health(&self) -> HealthResponse {
		let inner = self.inner.read().await;

		let ebpf_status = if inner.ebpf_programs_attached == inner.ebpf_programs_expected {
			HealthStatus::Healthy
		} else if inner.ebpf_programs_attached > 0 {
			HealthStatus::Degraded
		} else {
			HealthStatus::Unhealthy
		};

		let server_status = if inner.buffered_events == 0 {
			HealthStatus::Healthy
		} else {
			HealthStatus::Degraded
		};

		let buffer_utilization =
			(inner.buffer_size_bytes as f64 / inner.buffer_max_bytes as f64) * 100.0;
		let buffer_status = if buffer_utilization < 80.0 {
			HealthStatus::Healthy
		} else if buffer_utilization < 95.0 {
			HealthStatus::Degraded
		} else {
			HealthStatus::Unhealthy
		};

		let overall_status = match (&ebpf_status, &server_status, &buffer_status) {
			(HealthStatus::Unhealthy, _, _) => HealthStatus::Unhealthy,
			(_, _, HealthStatus::Unhealthy) => HealthStatus::Unhealthy,
			(HealthStatus::Degraded, _, _) => HealthStatus::Degraded,
			(_, HealthStatus::Degraded, _) => HealthStatus::Degraded,
			(_, _, HealthStatus::Degraded) => HealthStatus::Degraded,
			_ => HealthStatus::Healthy,
		};

		HealthResponse {
			status: overall_status,
			components: ComponentHealth {
				ebpf: EbpfHealth {
					status: ebpf_status,
					programs_attached: inner.ebpf_programs_attached,
					programs_expected: inner.ebpf_programs_expected,
				},
				server: ServerHealth {
					status: server_status,
					last_successful_send: inner.last_successful_send.map(|t| t.to_rfc3339()),
					buffered_events: inner.buffered_events,
				},
				buffer: BufferHealth {
					status: buffer_status,
					size_bytes: inner.buffer_size_bytes,
					utilization_percent: buffer_utilization,
				},
			},
		}
	}
}

async fn health_handler(State(state): State<HealthState>) -> impl IntoResponse {
	let health = state.get_health().await;
	let status_code = match health.status {
		HealthStatus::Healthy | HealthStatus::Degraded => axum::http::StatusCode::OK,
		HealthStatus::Unhealthy => axum::http::StatusCode::SERVICE_UNAVAILABLE,
	};

	(status_code, Json(health))
}

pub fn health_router(state: HealthState) -> Router {
	Router::new()
		.route("/health", get(health_handler))
		.with_state(state)
}
