// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_scm_mirror::PushMirror;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMirrorRequest {
	pub remote_url: String,
	pub credential_key: Option<String>,
	pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorResponse {
	pub id: Uuid,
	pub repo_id: Uuid,
	pub remote_url: String,
	pub enabled: bool,
	pub last_pushed_at: Option<DateTime<Utc>>,
	pub last_error: Option<String>,
	pub created_at: DateTime<Utc>,
}

impl From<PushMirror> for MirrorResponse {
	fn from(m: PushMirror) -> Self {
		Self {
			id: m.id,
			repo_id: m.repo_id,
			remote_url: m.remote_url,
			enabled: m.enabled,
			last_pushed_at: m.last_pushed_at,
			last_error: m.last_error,
			created_at: m.created_at,
		}
	}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListMirrorsResponse {
	pub mirrors: Vec<MirrorResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncResponse {
	pub message: String,
	pub queued: bool,
}
