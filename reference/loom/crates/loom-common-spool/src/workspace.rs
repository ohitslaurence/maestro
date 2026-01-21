// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::{Path, PathBuf};
use std::sync::Arc;

use jj_lib::backend::{ChangeId, CommitId};
use jj_lib::commit::Commit;
use jj_lib::object_id::ObjectId;
use jj_lib::op_store::OperationId;
use jj_lib::repo::{ReadonlyRepo, Repo, RepoLoader};
use jj_lib::transaction::Transaction;
use jj_lib::working_copy::WorkingCopy;
use jj_lib::workspace::Workspace;

use crate::config::SpoolSettings;
use crate::error::{Result, SpoolError};
use crate::types::{OperationId as SpoolOperationId, StitchId, TreeId};

/// A Spool workspace wrapping jj-lib's Workspace.
///
/// The workspace manages both the repository and working copy state.
pub struct SpoolWorkspace {
	workspace: Workspace,
	repo: Arc<ReadonlyRepo>,
	settings: SpoolSettings,
}

impl SpoolWorkspace {
	/// Initialize a new Spool repository at the given path.
	///
	/// If `colocate_git` is true, creates a Git-backed repository with
	/// a colocated .git directory for Git interoperability.
	pub fn init(path: impl AsRef<Path>, colocate_git: bool) -> Result<Self> {
		let path = path.as_ref();
		let settings = SpoolSettings::default_settings()?;

		let (workspace, repo) = if colocate_git {
			// Initialize with Git backend for Git interop
			Workspace::init_colocated_git(settings.inner(), path).map_err(|e| {
				SpoolError::workspace(format!("failed to init colocated git workspace: {e}"))
			})?
		} else {
			// Initialize with native jj backend (still uses git internally)
			Workspace::init_internal_git(settings.inner(), path)
				.map_err(|e| SpoolError::workspace(format!("failed to init internal git workspace: {e}")))?
		};

		Ok(Self {
			workspace,
			repo,
			settings,
		})
	}

	/// Open an existing Spool repository at the given path.
	///
	/// Searches for a .spool or .jj directory starting from the given path
	/// and walking up parent directories.
	pub fn open(path: impl AsRef<Path>) -> Result<Self> {
		let path = path.as_ref();
		let settings = SpoolSettings::default_settings()?;

		// Find the workspace root by looking for .spool or .jj
		let workspace_root = Self::find_workspace_root(path)?;

		// Load the workspace
		let workspace = Workspace::load(
			settings.inner(),
			&workspace_root,
			&jj_lib::repo::StoreFactories::default(),
			&jj_lib::workspace::default_working_copy_factories(),
		)
		.map_err(|e| SpoolError::workspace(format!("failed to load workspace: {e}")))?;

		let repo = workspace
			.repo_loader()
			.load_at_head()
			.map_err(SpoolError::backend)?;

		Ok(Self {
			workspace,
			repo,
			settings,
		})
	}

	/// Find the workspace root directory containing .jj.
	fn find_workspace_root(start: &Path) -> Result<PathBuf> {
		let start = if start.is_file() {
			start.parent().unwrap_or(start)
		} else {
			start
		};

		let mut current = start.to_path_buf();
		loop {
			if current.join(".jj").is_dir() {
				return Ok(current);
			}
			if !current.pop() {
				return Err(SpoolError::NotASpoolRepo);
			}
		}
	}

	/// Get the workspace root path.
	pub fn workspace_root(&self) -> &Path {
		self.workspace.workspace_root()
	}

	/// Get the repository loader.
	pub fn repo_loader(&self) -> &RepoLoader {
		self.workspace.repo_loader()
	}

	/// Get the current repository state.
	pub fn repo(&self) -> &Arc<ReadonlyRepo> {
		&self.repo
	}

	/// Get the settings.
	pub fn settings(&self) -> &SpoolSettings {
		&self.settings
	}

	/// Reload the repository from disk.
	pub fn reload(&mut self) -> Result<()> {
		self.repo = self
			.workspace
			.repo_loader()
			.load_at_head()
			.map_err(SpoolError::backend)?;
		Ok(())
	}

	/// Start a new transaction for making changes to the repository.
	pub fn start_transaction(&self) -> Transaction {
		self.repo.start_transaction()
	}

	/// Get the working copy state.
	pub fn working_copy(&self) -> &dyn WorkingCopy {
		self.workspace.working_copy()
	}

	/// Check if the working copy is colocated with Git.
	pub fn is_colocated(&self) -> bool {
		self.workspace_root().join(".git").is_dir()
	}

	/// Get the current working copy commit (the "shuttle" in spool terms).
	pub fn working_copy_commit(&self) -> Result<Commit> {
		// Get the working copy commit ID from the operation view
		let wc_commit_id = self
			.repo
			.view()
			.get_wc_commit_id(self.workspace.workspace_name())
			.ok_or_else(|| SpoolError::Workspace("no working copy commit".to_string()))?;

		self
			.repo
			.store()
			.get_commit(wc_commit_id)
			.map_err(SpoolError::backend)
	}

	/// Get the workspace name.
	pub fn workspace_name(&self) -> &jj_lib::ref_name::WorkspaceName {
		self.workspace.workspace_name()
	}

	/// Get access to the inner workspace for advanced operations.
	pub fn inner(&self) -> &Workspace {
		&self.workspace
	}

	/// Get mutable access to the inner workspace.
	pub fn inner_mut(&mut self) -> &mut Workspace {
		&mut self.workspace
	}
}

/// Convert a jj-lib ChangeId to a SpoolStitchId.
pub fn change_id_to_stitch_id(id: &ChangeId) -> StitchId {
	let bytes = id.as_bytes();
	let mut arr = [0u8; 16];
	let len = bytes.len().min(16);
	arr[..len].copy_from_slice(&bytes[..len]);
	StitchId(arr)
}

/// Convert a SpoolStitchId to a jj-lib ChangeId.
pub fn stitch_id_to_change_id(id: &StitchId) -> ChangeId {
	ChangeId::new(id.0.to_vec())
}

/// Convert a jj-lib CommitId to a TreeId.
pub fn commit_id_to_tree_id(id: &CommitId) -> TreeId {
	let bytes = id.as_bytes();
	let mut arr = [0u8; 20];
	let len = bytes.len().min(20);
	arr[..len].copy_from_slice(&bytes[..len]);
	TreeId(arr)
}

/// Convert a jj-lib OperationId to a SpoolOperationId.
pub fn operation_id_to_spool(id: &OperationId) -> SpoolOperationId {
	let bytes = id.as_bytes();
	let mut arr = [0u8; 16];
	let len = bytes.len().min(16);
	arr[..len].copy_from_slice(&bytes[..len]);
	SpoolOperationId(arr)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	#[test]
	fn test_init_and_open() {
		let temp = TempDir::new().unwrap();
		let path = temp.path();

		// Initialize a new workspace
		let ws = SpoolWorkspace::init(path, false).expect("init should succeed");
		assert!(path.join(".jj").is_dir());

		drop(ws);

		// Reopen the workspace
		let ws = SpoolWorkspace::open(path).expect("open should succeed");
		assert_eq!(ws.workspace_root(), path);
	}

	#[test]
	fn test_init_colocated() {
		let temp = TempDir::new().unwrap();
		let path = temp.path();

		let ws = SpoolWorkspace::init(path, true).expect("init should succeed");
		assert!(ws.is_colocated());
		assert!(path.join(".git").is_dir());
	}

	#[test]
	fn test_not_a_spool_repo() {
		let temp = TempDir::new().unwrap();
		let result = SpoolWorkspace::open(temp.path());
		assert!(matches!(result, Err(SpoolError::NotASpoolRepo)));
	}

	#[test]
	fn test_change_id_conversion() {
		let original = StitchId([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
		let change_id = stitch_id_to_change_id(&original);
		let converted = change_id_to_stitch_id(&change_id);
		assert_eq!(original, converted);
	}
}
