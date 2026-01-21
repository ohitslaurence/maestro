// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::{Path, PathBuf};

use gix::object::Kind;
use gix::ObjectId;
use tracing::instrument;

use crate::error::{Result, ScmError};
use crate::git_types::{CommitInfo, TreeEntry, TreeEntryKind};

pub struct GitRepository {
	path: PathBuf,
}

impl GitRepository {
	#[instrument(skip_all, fields(path = %path.display()))]
	pub fn init_bare(path: &Path) -> Result<Self> {
		gix::init_bare(path).map_err(|e| ScmError::GitError(e.to_string()))?;
		Ok(Self {
			path: path.to_path_buf(),
		})
	}

	#[instrument(skip_all, fields(path = %path.display()))]
	pub fn open(path: &Path) -> Result<Self> {
		let repo = gix::open(path).map_err(|e| ScmError::GitError(e.to_string()))?;
		Ok(Self {
			path: repo.path().to_path_buf(),
		})
	}

	pub fn path(&self) -> &Path {
		&self.path
	}

	fn repo(&self) -> Result<gix::Repository> {
		gix::open(&self.path).map_err(|e| ScmError::GitError(e.to_string()))
	}

	#[instrument(skip(self))]
	pub fn default_branch(&self) -> Result<String> {
		let repo = self.repo()?;
		let head = repo
			.head_ref()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		match head {
			Some(r) => {
				let name = r.name().shorten().to_string();
				Ok(name)
			}
			None => Ok("cannon".to_string()),
		}
	}

	#[instrument(skip(self), fields(branch = %branch))]
	pub fn set_default_branch(&self, branch: &str) -> Result<()> {
		let head_path = self.path.join("HEAD");
		let content = format!("ref: refs/heads/{}\n", branch);
		std::fs::write(&head_path, content)?;
		Ok(())
	}

	#[instrument(skip(self))]
	pub fn list_branches(&self) -> Result<Vec<String>> {
		let repo = self.repo()?;
		let refs = repo
			.references()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let branches = refs
			.prefixed("refs/heads/")
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let mut result = Vec::new();
		for r in branches {
			let r = r.map_err(|e| ScmError::GitError(e.to_string()))?;
			let name = r.name().shorten().to_string();
			result.push(name);
		}
		Ok(result)
	}

	#[instrument(skip(self))]
	pub fn list_tags(&self) -> Result<Vec<String>> {
		let repo = self.repo()?;
		let refs = repo
			.references()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let tags = refs
			.prefixed("refs/tags/")
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let mut result = Vec::new();
		for r in tags {
			let r = r.map_err(|e| ScmError::GitError(e.to_string()))?;
			let name = r.name().shorten().to_string();
			result.push(name);
		}
		Ok(result)
	}

	#[instrument(skip(self), fields(sha = %sha))]
	pub fn get_commit(&self, sha: &str) -> Result<CommitInfo> {
		let repo = self.repo()?;
		let oid = ObjectId::from_hex(sha.as_bytes()).map_err(|e| ScmError::GitError(e.to_string()))?;
		let object = repo
			.find_object(oid)
			.map_err(|_| ScmError::ObjectNotFound(sha.to_string()))?;
		let commit = object.into_commit();
		self.commit_to_info(&commit)
	}

	#[instrument(skip(self), fields(refname = %refname, limit = ?limit))]
	pub fn list_commits(&self, refname: &str, limit: Option<usize>) -> Result<Vec<CommitInfo>> {
		let repo = self.repo()?;
		let oid = self.resolve_to_oid(&repo, refname)?;
		let walk = repo
			.rev_walk([oid])
			.all()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let mut commits = Vec::new();
		for info in walk {
			if let Some(max) = limit {
				if commits.len() >= max {
					break;
				}
			}
			let info = info.map_err(|e| ScmError::GitError(e.to_string()))?;
			let object = info
				.object()
				.map_err(|e| ScmError::GitError(e.to_string()))?;
			commits.push(self.object_to_commit_info(&object)?);
		}
		Ok(commits)
	}

	#[instrument(skip(self), fields(refname = %refname, path = %path))]
	pub fn list_tree(&self, refname: &str, path: &str) -> Result<Vec<TreeEntry>> {
		let repo = self.repo()?;
		let oid = self.resolve_to_oid(&repo, refname)?;
		let object = repo
			.find_object(oid)
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let commit = object.into_commit();
		let tree_id = commit
			.tree_id()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let tree_obj = repo
			.find_object(tree_id)
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let tree = tree_obj.into_tree();

		let target_tree = if path.is_empty() || path == "." {
			tree
		} else {
			let entry = tree
				.lookup_entry_by_path(path)
				.map_err(|e| ScmError::GitError(e.to_string()))?
				.ok_or_else(|| ScmError::ObjectNotFound(path.to_string()))?;
			let obj = entry
				.object()
				.map_err(|e| ScmError::GitError(e.to_string()))?;
			obj.into_tree()
		};

		let mut entries = Vec::new();
		for entry_result in target_tree.iter() {
			let entry = entry_result.map_err(|e| ScmError::GitError(e.to_string()))?;
			let name = entry.filename().to_string();
			let entry_path = if path.is_empty() || path == "." {
				name.clone()
			} else {
				format!("{}/{}", path, name)
			};
			let kind = match entry.mode().kind() {
				gix::object::tree::EntryKind::Tree => TreeEntryKind::Directory,
				gix::object::tree::EntryKind::Blob | gix::object::tree::EntryKind::BlobExecutable => {
					TreeEntryKind::File
				}
				gix::object::tree::EntryKind::Link => TreeEntryKind::Symlink,
				gix::object::tree::EntryKind::Commit => TreeEntryKind::Submodule,
			};
			entries.push(TreeEntry {
				name,
				path: entry_path,
				kind,
				sha: entry.id().to_string(),
			});
		}
		Ok(entries)
	}

	#[instrument(skip(self), fields(refname = %refname, path = %path))]
	pub fn get_blob(&self, refname: &str, path: &str) -> Result<Vec<u8>> {
		let repo = self.repo()?;
		let oid = self.resolve_to_oid(&repo, refname)?;
		let object = repo
			.find_object(oid)
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let commit = object.into_commit();
		let tree_id = commit
			.tree_id()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let tree_obj = repo
			.find_object(tree_id)
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let tree = tree_obj.into_tree();
		let entry = tree
			.lookup_entry_by_path(path)
			.map_err(|e| ScmError::GitError(e.to_string()))?
			.ok_or_else(|| ScmError::ObjectNotFound(path.to_string()))?;
		let blob_obj = entry
			.object()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		if blob_obj.kind != Kind::Blob {
			return Err(ScmError::GitError(format!(
				"expected blob, got {:?}",
				blob_obj.kind
			)));
		}
		Ok(blob_obj.data.to_vec())
	}

	#[instrument(skip(self), fields(refname = %refname))]
	pub fn ref_exists(&self, refname: &str) -> Result<bool> {
		let repo = self.repo()?;
		match self.resolve_to_oid(&repo, refname) {
			Ok(_) => Ok(true),
			Err(ScmError::RefNotFound(_)) => Ok(false),
			Err(e) => Err(e),
		}
	}

	// NOTE: gc, prune, fsck use git subprocess because gitoxide doesn't yet support these
	// maintenance operations. See: https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
	// Track progress at: https://github.com/GitoxideLabs/gitoxide/issues/307

	#[instrument(skip(self))]
	pub fn gc(&self) -> Result<()> {
		let output = std::process::Command::new("git")
			.arg("gc")
			.current_dir(&self.path)
			.output()?;
		if !output.status.success() {
			return Err(ScmError::GitError(
				String::from_utf8_lossy(&output.stderr).to_string(),
			));
		}
		Ok(())
	}

	#[instrument(skip(self))]
	pub fn prune(&self) -> Result<()> {
		let output = std::process::Command::new("git")
			.arg("prune")
			.current_dir(&self.path)
			.output()?;
		if !output.status.success() {
			return Err(ScmError::GitError(
				String::from_utf8_lossy(&output.stderr).to_string(),
			));
		}
		Ok(())
	}

	#[instrument(skip(self))]
	pub fn fsck(&self) -> Result<Vec<String>> {
		let output = std::process::Command::new("git")
			.arg("fsck")
			.current_dir(&self.path)
			.output()?;
		let stderr = String::from_utf8_lossy(&output.stderr);
		let stdout = String::from_utf8_lossy(&output.stdout);
		let mut errors = Vec::new();
		for line in stderr.lines().chain(stdout.lines()) {
			if !line.is_empty() {
				errors.push(line.to_string());
			}
		}
		Ok(errors)
	}

	#[instrument(skip(self))]
	pub fn delete(self) -> Result<()> {
		std::fs::remove_dir_all(&self.path)?;
		Ok(())
	}

	#[instrument(skip(self, content), fields(branch = %branch, filename = %filename))]
	pub fn create_initial_commit(
		&self,
		branch: &str,
		filename: &str,
		content: &[u8],
		commit_message: &str,
		author_name: &str,
		author_email: &str,
	) -> Result<String> {
		let repo = self.repo()?;

		// Create blob from content
		let blob_id = repo
			.write_blob(content)
			.map_err(|e| ScmError::GitError(format!("Failed to write blob: {}", e)))?;

		// Create tree with single file entry
		let tree_entry = gix::objs::tree::Entry {
			mode: gix::objs::tree::EntryKind::Blob.into(),
			filename: filename.into(),
			oid: blob_id.detach(),
		};
		let tree = gix::objs::Tree {
			entries: vec![tree_entry],
		};
		let tree_id = repo
			.write_object(&tree)
			.map_err(|e| ScmError::GitError(format!("Failed to write tree: {}", e)))?;

		// Create commit
		let time = gix::date::Time::now_local_or_utc();
		let signature = gix::actor::SignatureRef {
			name: author_name.into(),
			email: author_email.into(),
			time,
		};
		let commit = gix::objs::Commit {
			tree: tree_id.detach(),
			parents: smallvec::smallvec![],
			author: signature.to_owned(),
			committer: signature.to_owned(),
			encoding: None,
			message: commit_message.into(),
			extra_headers: vec![],
		};
		let commit_id = repo
			.write_object(&commit)
			.map_err(|e| ScmError::GitError(format!("Failed to write commit: {}", e)))?;

		// Update branch ref to point to commit
		let ref_name = format!("refs/heads/{}", branch);
		let ref_path = self.path.join(&ref_name);
		if let Some(parent) = ref_path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(&ref_path, format!("{}\n", commit_id))?;

		Ok(commit_id.to_string())
	}

	fn resolve_to_oid(&self, repo: &gix::Repository, refname: &str) -> Result<ObjectId> {
		if let Ok(oid) = ObjectId::from_hex(refname.as_bytes()) {
			return Ok(oid);
		}
		let full_ref = if refname.starts_with("refs/") {
			refname.to_string()
		} else {
			format!("refs/heads/{}", refname)
		};
		let mut reference = repo
			.find_reference(&full_ref)
			.map_err(|_| ScmError::RefNotFound(refname.to_string()))?;
		let peeled = reference
			.peel_to_id_in_place()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		Ok(peeled.detach())
	}

	fn commit_to_info(&self, commit: &gix::Commit) -> Result<CommitInfo> {
		let decoded = commit
			.decode()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let author_time = decoded.author.time;
		let committer_time = decoded.committer.time;
		let author_date =
			chrono::DateTime::from_timestamp(author_time.seconds, 0).unwrap_or_else(chrono::Utc::now);
		let committer_date =
			chrono::DateTime::from_timestamp(committer_time.seconds, 0).unwrap_or_else(chrono::Utc::now);
		let parent_shas = decoded.parents().map(|id| id.to_string()).collect();
		Ok(CommitInfo {
			sha: commit.id.to_string(),
			message: decoded.message.to_string(),
			author_name: decoded.author.name.to_string(),
			author_email: decoded.author.email.to_string(),
			author_date,
			committer_name: decoded.committer.name.to_string(),
			committer_email: decoded.committer.email.to_string(),
			committer_date,
			parent_shas,
		})
	}

	fn object_to_commit_info(&self, object: &gix::Commit<'_>) -> Result<CommitInfo> {
		let decoded = object
			.decode()
			.map_err(|e| ScmError::GitError(e.to_string()))?;
		let author_time = decoded.author.time;
		let committer_time = decoded.committer.time;
		let author_date =
			chrono::DateTime::from_timestamp(author_time.seconds, 0).unwrap_or_else(chrono::Utc::now);
		let committer_date =
			chrono::DateTime::from_timestamp(committer_time.seconds, 0).unwrap_or_else(chrono::Utc::now);
		let parent_shas = decoded.parents().map(|id| id.to_string()).collect();
		Ok(CommitInfo {
			sha: object.id.to_string(),
			message: decoded.message.to_string(),
			author_name: decoded.author.name.to_string(),
			author_email: decoded.author.email.to_string(),
			author_date,
			committer_name: decoded.committer.name.to_string(),
			committer_email: decoded.committer.email.to_string(),
			committer_date,
			parent_shas,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_init_bare_and_open() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		assert!(repo.path().exists());
		let opened = GitRepository::open(&repo_path).unwrap();
		assert_eq!(opened.path(), repo.path());
	}

	#[test]
	fn test_default_branch_empty_repo() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		let branch = repo.default_branch().unwrap();
		assert!(!branch.is_empty());
	}

	#[test]
	fn test_set_default_branch() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		repo.set_default_branch("cannon").unwrap();
		let content = std::fs::read_to_string(repo_path.join("HEAD")).unwrap();
		assert!(content.contains("refs/heads/cannon"));
	}

	#[test]
	fn test_list_branches_empty() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		let branches = repo.list_branches().unwrap();
		assert!(branches.is_empty());
	}

	#[test]
	fn test_list_tags_empty() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		let tags = repo.list_tags().unwrap();
		assert!(tags.is_empty());
	}

	#[test]
	fn test_ref_exists_nonexistent() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		assert!(!repo.ref_exists("main").unwrap());
	}

	#[test]
	fn test_delete() {
		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test.git");
		let repo = GitRepository::init_bare(&repo_path).unwrap();
		let path = repo.path().to_path_buf();
		assert!(path.exists());
		repo.delete().unwrap();
		assert!(!path.exists());
	}
}
