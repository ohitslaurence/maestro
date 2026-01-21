// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_server_scm::{MaintenanceJob, MaintenanceJobStatus, MaintenanceTask};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerMaintenanceRequest {
	#[serde(default = "default_task")]
	pub task: MaintenanceTaskApi,
}

fn default_task() -> MaintenanceTaskApi {
	MaintenanceTaskApi::Gc
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MaintenanceTaskApi {
	Gc,
	Prune,
	Repack,
	Fsck,
	All,
}

impl From<MaintenanceTaskApi> for MaintenanceTask {
	fn from(api: MaintenanceTaskApi) -> Self {
		match api {
			MaintenanceTaskApi::Gc => MaintenanceTask::Gc,
			MaintenanceTaskApi::Prune => MaintenanceTask::Prune,
			MaintenanceTaskApi::Repack => MaintenanceTask::Repack,
			MaintenanceTaskApi::Fsck => MaintenanceTask::Fsck,
			MaintenanceTaskApi::All => MaintenanceTask::All,
		}
	}
}

impl From<MaintenanceTask> for MaintenanceTaskApi {
	fn from(task: MaintenanceTask) -> Self {
		match task {
			MaintenanceTask::Gc => MaintenanceTaskApi::Gc,
			MaintenanceTask::Prune => MaintenanceTaskApi::Prune,
			MaintenanceTask::Repack => MaintenanceTaskApi::Repack,
			MaintenanceTask::Fsck => MaintenanceTaskApi::Fsck,
			MaintenanceTask::All => MaintenanceTaskApi::All,
		}
	}
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MaintenanceJobStatusApi {
	Pending,
	Running,
	Success,
	Failed,
}

impl From<MaintenanceJobStatus> for MaintenanceJobStatusApi {
	fn from(status: MaintenanceJobStatus) -> Self {
		match status {
			MaintenanceJobStatus::Pending => MaintenanceJobStatusApi::Pending,
			MaintenanceJobStatus::Running => MaintenanceJobStatusApi::Running,
			MaintenanceJobStatus::Success => MaintenanceJobStatusApi::Success,
			MaintenanceJobStatus::Failed => MaintenanceJobStatusApi::Failed,
		}
	}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MaintenanceJobResponse {
	pub id: String,
	pub repo_id: Option<String>,
	pub task: MaintenanceTaskApi,
	pub status: MaintenanceJobStatusApi,
	pub started_at: Option<String>,
	pub finished_at: Option<String>,
	pub error: Option<String>,
	pub created_at: String,
}

impl From<MaintenanceJob> for MaintenanceJobResponse {
	fn from(job: MaintenanceJob) -> Self {
		Self {
			id: job.id.to_string(),
			repo_id: job.repo_id.map(|id| id.to_string()),
			task: job.task.into(),
			status: job.status.into(),
			started_at: job.started_at.map(|t| t.to_rfc3339()),
			finished_at: job.finished_at.map(|t| t.to_rfc3339()),
			error: job.error,
			created_at: job.created_at.to_rfc3339(),
		}
	}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TriggerMaintenanceResponse {
	pub job_id: String,
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MaintenanceErrorResponse {
	pub error: String,
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListMaintenanceJobsResponse {
	pub jobs: Vec<MaintenanceJobResponse>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListMaintenanceJobsQuery {
	#[serde(default = "default_limit")]
	pub limit: u32,
}

fn default_limit() -> u32 {
	50
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerGlobalSweepRequest {
	#[serde(default = "default_task")]
	pub task: MaintenanceTaskApi,
	#[serde(default = "default_stagger_ms")]
	pub stagger_ms: u64,
}

fn default_stagger_ms() -> u64 {
	1000
}
