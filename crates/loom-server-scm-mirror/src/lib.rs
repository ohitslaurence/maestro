// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

pub mod cleanup;
pub mod error;
pub mod pull;
pub mod push;
pub mod schema;
pub mod store;
pub mod types;

pub use cleanup::{
	cleanup_mirror_with_check, delete_mirror, find_stale_mirrors, run_cleanup_job, touch_mirror,
	CleanupDecision, CleanupResult, ExternalMirrorStore,
};
pub use error::{MirrorError, Result};
pub use pull::{
	check_repo_exists, get_clone_url, pull_mirror, pull_mirror_with_recovery, PullResult,
};
pub use push::push_mirror;
pub use store::{
	MirrorRepository, PushMirrorStore, SqliteExternalMirrorStore, SqlitePushMirrorStore,
};
pub use types::{
	CreateExternalMirror, CreatePushMirror, ExternalMirror, MirrorBranchRule, Platform, PushMirror,
};
