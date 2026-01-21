// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::fmt;
use std::str::FromStr;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::ThreadIdError;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ThreadId(String);

impl ThreadId {
	pub fn new() -> Self {
		let uuid = uuid7::uuid7();
		Self(format!("T-{uuid}"))
	}

	/// Create a ThreadId from an existing string without validation.
	/// Use `parse()` if you need validation.
	pub fn from_string(s: String) -> Self {
		Self(s)
	}

	pub fn parse(s: &str) -> Result<Self, ThreadIdError> {
		if !s.starts_with("T-") {
			return Err(ThreadIdError::InvalidPrefix(
				s.chars().take(2).collect::<String>(),
			));
		}

		let uuid_part = &s[2..];
		uuid::Uuid::parse_str(uuid_part)?;

		Ok(Self(s.to_string()))
	}

	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl Default for ThreadId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for ThreadId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for ThreadId {
	type Err = ThreadIdError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::parse(s)
	}
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
	System,
	User,
	Assistant,
	Tool,
}

impl From<&loom_common_core::Role> for MessageRole {
	fn from(role: &loom_common_core::Role) -> Self {
		match role {
			loom_common_core::Role::System => Self::System,
			loom_common_core::Role::User => Self::User,
			loom_common_core::Role::Assistant => Self::Assistant,
			loom_common_core::Role::Tool => Self::Tool,
		}
	}
}

/// Thread visibility controls how synced threads are exposed on the server.
/// - Organization: visible to organization members (default)
/// - Private: synced but only owner can see
/// - Public: may be listed/exposed publicly
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ThreadVisibility {
	#[default]
	Organization,
	Private,
	Public,
}

impl ThreadVisibility {
	pub fn as_str(&self) -> &'static str {
		match self {
			ThreadVisibility::Organization => "organization",
			ThreadVisibility::Private => "private",
			ThreadVisibility::Public => "public",
		}
	}
}

impl std::str::FromStr for ThreadVisibility {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().as_str() {
			"organization" | "organisation" => Ok(ThreadVisibility::Organization),
			"private" => Ok(ThreadVisibility::Private),
			"public" => Ok(ThreadVisibility::Public),
			_ => Err(format!("invalid visibility: {s}")),
		}
	}
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageSnapshot {
	pub role: MessageRole,
	pub content: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_call_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<ToolCallSnapshot>>,
}

impl From<&loom_common_core::Message> for MessageSnapshot {
	fn from(msg: &loom_common_core::Message) -> Self {
		let tool_calls = if msg.tool_calls.is_empty() {
			None
		} else {
			Some(
				msg
					.tool_calls
					.iter()
					.map(|tc| ToolCallSnapshot {
						id: tc.id.clone(),
						tool_name: tc.tool_name.clone(),
						arguments_json: tc.arguments_json.clone(),
					})
					.collect(),
			)
		};
		Self {
			role: MessageRole::from(&msg.role),
			content: msg.content.clone(),
			tool_call_id: msg.tool_call_id.clone(),
			tool_name: msg.name.clone(),
			tool_calls,
		}
	}
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallSnapshot {
	pub id: String,
	pub tool_name: String,
	pub arguments_json: serde_json::Value,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConversationSnapshot {
	pub messages: Vec<MessageSnapshot>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStateKind {
	WaitingForUserInput,
	CallingLlm,
	ProcessingLlmResponse,
	ExecutingTools,
	PostToolsHook,
	Error,
	ShuttingDown,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingToolCallSnapshot {
	pub call_id: String,
	pub tool_name: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
	pub kind: AgentStateKind,
	#[serde(default)]
	pub retries: u32,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub last_error: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub pending_tool_calls: Vec<PendingToolCallSnapshot>,
}

impl From<&loom_common_core::AgentState> for AgentStateSnapshot {
	fn from(state: &loom_common_core::AgentState) -> Self {
		match state {
			loom_common_core::AgentState::WaitingForUserInput { .. } => Self {
				kind: AgentStateKind::WaitingForUserInput,
				retries: 0,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
			loom_common_core::AgentState::CallingLlm { retries, .. } => Self {
				kind: AgentStateKind::CallingLlm,
				retries: *retries,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
			loom_common_core::AgentState::ProcessingLlmResponse { .. } => Self {
				kind: AgentStateKind::ProcessingLlmResponse,
				retries: 0,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
			loom_common_core::AgentState::ExecutingTools { executions, .. } => Self {
				kind: AgentStateKind::ExecutingTools,
				retries: 0,
				last_error: None,
				pending_tool_calls: executions
					.iter()
					.map(|e| PendingToolCallSnapshot {
						call_id: e.call_id().to_string(),
						tool_name: e.tool_name().to_string(),
					})
					.collect(),
			},
			loom_common_core::AgentState::PostToolsHook { .. } => Self {
				kind: AgentStateKind::PostToolsHook,
				retries: 0,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
			loom_common_core::AgentState::Error { error, retries, .. } => Self {
				kind: AgentStateKind::Error,
				retries: *retries,
				last_error: Some(error.to_string()),
				pending_tool_calls: Vec::new(),
			},
			loom_common_core::AgentState::ShuttingDown => Self {
				kind: AgentStateKind::ShuttingDown,
				retries: 0,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
		}
	}
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ThreadMetadata {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub tags: Vec<String>,
	#[serde(default)]
	pub is_pinned: bool,
	#[serde(default, skip_serializing_if = "is_null_or_empty_object")]
	pub extra: serde_json::Value,
}

fn is_null_or_empty_object(v: &serde_json::Value) -> bool {
	v.is_null() || (v.is_object() && v.as_object().is_none_or(|o| o.is_empty()))
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
	pub id: ThreadId,
	pub version: u64,
	pub created_at: String,
	pub updated_at: String,
	pub last_activity_at: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub workspace_root: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cwd: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub loom_version: Option<String>,

	/// Current git branch name (e.g., "main", "feature/xyz")
	/// None if not a git repo, detached HEAD, or git unavailable
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_branch: Option<String>,

	/// Normalized remote URL slug (e.g., "github.com/owner/repo")
	/// None if not a git repo, no remotes configured, or git unavailable
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_remote_url: Option<String>,

	/// Branch when the thread was created
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_branch: Option<String>,

	/// Commit SHA when the thread was created
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_commit_sha: Option<String>,

	/// Latest known commit SHA
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_current_commit_sha: Option<String>,

	/// Whether working tree was dirty at thread creation
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_start_dirty: Option<bool>,

	/// Whether working tree was dirty at last update
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_end_dirty: Option<bool>,

	/// All commit SHAs observed during this session (chronological order)
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub git_commits: Vec<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub provider: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub model: Option<String>,

	pub conversation: ConversationSnapshot,
	pub agent_state: AgentStateSnapshot,
	pub metadata: ThreadMetadata,

	/// Server-side visibility for synced threads
	#[serde(default)]
	pub visibility: ThreadVisibility,

	/// If true, this thread is local-only and NEVER syncs to server
	#[serde(default)]
	pub is_private: bool,

	/// If true, this thread has been shared with the support team
	#[serde(default)]
	pub is_shared_with_support: bool,
}

impl Thread {
	pub fn new() -> Self {
		let now = Utc::now().to_rfc3339();
		Self {
			id: ThreadId::new(),
			version: 1,
			created_at: now.clone(),
			updated_at: now.clone(),
			last_activity_at: now,
			workspace_root: None,
			cwd: None,
			loom_version: None,
			git_branch: None,
			git_remote_url: None,
			git_initial_branch: None,
			git_initial_commit_sha: None,
			git_current_commit_sha: None,
			git_start_dirty: None,
			git_end_dirty: None,
			git_commits: Vec::new(),
			provider: None,
			model: None,
			conversation: ConversationSnapshot::default(),
			agent_state: AgentStateSnapshot {
				kind: AgentStateKind::WaitingForUserInput,
				retries: 0,
				last_error: None,
				pending_tool_calls: Vec::new(),
			},
			metadata: ThreadMetadata::default(),
			visibility: ThreadVisibility::Organization,
			is_private: false,
			is_shared_with_support: false,
		}
	}

	pub fn touch(&mut self) {
		let now = Utc::now().to_rfc3339();
		self.updated_at = now.clone();
		self.last_activity_at = now;
		self.version += 1;
	}
}

impl Default for Thread {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadSummary {
	pub id: ThreadId,
	pub version: u64,
	pub created_at: String,
	pub updated_at: String,
	pub last_activity_at: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub workspace_root: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_branch: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_remote_url: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_commit_sha: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_current_commit_sha: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub provider: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub model: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub tags: Vec<String>,
	pub message_count: u32,
	pub is_pinned: bool,
	pub visibility: ThreadVisibility,
}

impl From<&Thread> for ThreadSummary {
	fn from(thread: &Thread) -> Self {
		Self {
			id: thread.id.clone(),
			version: thread.version,
			created_at: thread.created_at.clone(),
			updated_at: thread.updated_at.clone(),
			last_activity_at: thread.last_activity_at.clone(),
			title: thread.metadata.title.clone(),
			workspace_root: thread.workspace_root.clone(),
			git_branch: thread.git_branch.clone(),
			git_remote_url: thread.git_remote_url.clone(),
			git_initial_commit_sha: thread.git_initial_commit_sha.clone(),
			git_current_commit_sha: thread.git_current_commit_sha.clone(),
			provider: thread.provider.clone(),
			model: thread.model.clone(),
			tags: thread.metadata.tags.clone(),
			message_count: thread.conversation.messages.len() as u32,
			is_pinned: thread.metadata.is_pinned,
			visibility: thread.visibility.clone(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	/// **Property: Thread JSON roundtrip preserves all data**
	///
	/// Why this is important: Thread persistence relies on JSON serialization.
	/// Any data loss during serialization/deserialization would corrupt user
	/// conversations, leading to lost work and broken state restoration.
	///
	/// Invariant: serialize(deserialize(serialize(thread))) == serialize(thread)
	#[test]
	fn test_thread_json_roundtrip() {
		let mut thread = Thread::new();
		thread.workspace_root = Some("/home/user/project".to_string());
		thread.metadata.title = Some("Test conversation".to_string());
		thread.metadata.tags = vec!["rust".to_string(), "testing".to_string()];
		thread.conversation.messages.push(MessageSnapshot {
			role: MessageRole::User,
			content: "Hello".to_string(),
			tool_call_id: None,
			tool_name: None,
			tool_calls: None,
		});

		let json = serde_json::to_string(&thread).expect("serialize");
		let restored: Thread = serde_json::from_str(&json).expect("deserialize");
		let json2 = serde_json::to_string(&restored).expect("serialize again");

		assert_eq!(json, json2);
	}

	/// **Property: ThreadId format is always T-{uuid7}**
	///
	/// Why this is important: ThreadIds are used as filenames and API
	/// identifiers. Inconsistent format would break file lookups, URL routing,
	/// and cross-system thread identification.
	///
	/// Invariant: All generated ThreadIds start with "T-" followed by a valid
	/// UUID
	#[test]
	fn test_thread_id_format_validation() {
		let id = ThreadId::new();
		let s = id.to_string();

		assert!(s.starts_with("T-"), "ThreadId must start with 'T-'");
		assert_eq!(s.len(), 2 + 36, "ThreadId must be T- plus 36-char UUID");

		let uuid_part = &s[2..];
		uuid::Uuid::parse_str(uuid_part).expect("UUID part must be valid");
	}

	/// **Property: ThreadId parsing rejects invalid formats**
	///
	/// Why this is important: Accepting malformed IDs could lead to filesystem
	/// path injection, broken lookups, or inconsistent state across systems.
	///
	/// Invariant: parse() returns Err for any string not matching T-{valid-uuid}
	#[test]
	fn test_thread_id_parse_rejects_invalid() {
		assert!(ThreadId::parse("invalid").is_err());
		assert!(ThreadId::parse("X-12345678-1234-1234-1234-123456789abc").is_err());
		assert!(ThreadId::parse("T-not-a-uuid").is_err());
		assert!(ThreadId::parse("T-").is_err());
		assert!(ThreadId::parse("").is_err());
	}

	/// **Property: ThreadId roundtrip through string preserves identity**
	///
	/// Why this is important: ThreadIds are frequently converted to strings for
	/// storage and back for lookups. Any mutation would cause lookup failures.
	///
	/// Invariant: parse(id.to_string()) == Ok(id)
	#[test]
	fn test_thread_id_string_roundtrip() {
		let id = ThreadId::new();
		let s = id.to_string();
		let parsed = ThreadId::parse(&s).expect("should parse");
		assert_eq!(id, parsed);
	}

	proptest! {
			/// **Property: Version always increases on touch()**
			///
			/// Why this is important: Version numbers enable optimistic concurrency
			/// control for sync. Non-monotonic versions would cause sync conflicts
			/// and potential data loss.
			///
			/// Invariant: After any number of touch() calls, version > initial_version
			#[test]
			fn test_version_monotonicity(touch_count in 1usize..100) {
					let mut thread = Thread::new();
					let initial_version = thread.version;

					for _ in 0..touch_count {
							thread.touch();
					}

					prop_assert!(thread.version > initial_version);
					prop_assert_eq!(thread.version, initial_version + touch_count as u64);
			}

			/// **Property: AgentStateKind serializes to snake_case**
			///
			/// Why this is important: API contracts expect snake_case for JSON field
			/// values. Inconsistent casing would break deserialization on other systems.
			///
			/// Invariant: All AgentStateKind variants serialize to lowercase snake_case
			#[test]
			fn test_agent_state_kind_serde_format(_dummy in 0u8..1u8) {
					let kinds = [
							(AgentStateKind::WaitingForUserInput, "\"waiting_for_user_input\""),
							(AgentStateKind::CallingLlm, "\"calling_llm\""),
							(AgentStateKind::ProcessingLlmResponse, "\"processing_llm_response\""),
							(AgentStateKind::ExecutingTools, "\"executing_tools\""),
							(AgentStateKind::Error, "\"error\""),
							(AgentStateKind::ShuttingDown, "\"shutting_down\""),
					];

					for (kind, expected) in kinds {
							let json = serde_json::to_string(&kind).unwrap();
							prop_assert_eq!(json, expected);
					}
			}
	}

	/// **Property: ThreadVisibility serializes to lowercase**
	///
	/// Why this is important: API contracts expect lowercase visibility values
	/// for JSON compatibility and cross-system interoperability.
	///
	/// Invariant: All ThreadVisibility variants serialize to their lowercase name
	#[test]
	fn test_visibility_serde_format() {
		let variants = [
			(ThreadVisibility::Organization, "\"organization\""),
			(ThreadVisibility::Private, "\"private\""),
			(ThreadVisibility::Public, "\"public\""),
		];
		for (vis, expected) in variants {
			let json = serde_json::to_string(&vis).unwrap();
			assert_eq!(json, expected);
		}
	}

	/// **Property: Default thread has organization visibility, is not local-only,
	/// and not shared with support**
	///
	/// Why this is important: New threads should sync by default
	/// (is_private=false) and have organization visibility for team
	/// collaboration.
	#[test]
	fn test_default_thread_visibility() {
		let thread = Thread::new();
		assert_eq!(thread.visibility, ThreadVisibility::Organization);
		assert!(!thread.is_private);
		assert!(!thread.is_shared_with_support);
	}

	/// **Property: ThreadVisibility FromStr accepts both spellings of
	/// organization**
	///
	/// Why this is important: Users may use American or British spelling.
	#[test]
	fn test_visibility_from_str_spellings() {
		assert_eq!(
			"organization".parse::<ThreadVisibility>().unwrap(),
			ThreadVisibility::Organization
		);
		assert_eq!(
			"organisation".parse::<ThreadVisibility>().unwrap(),
			ThreadVisibility::Organization
		);
		assert_eq!(
			"public".parse::<ThreadVisibility>().unwrap(),
			ThreadVisibility::Public
		);
	}

	/// **Property: is_shared_with_support is independent of visibility**
	///
	/// Why this is important: Sharing with support should not change the
	/// thread's visibility setting - they are orthogonal concepts.
	#[test]
	fn test_is_shared_with_support_independent_of_visibility() {
		let mut thread = Thread::new();
		thread.visibility = ThreadVisibility::Private;
		thread.is_shared_with_support = true;

		let json = serde_json::to_string(&thread).expect("serialize");
		let restored: Thread = serde_json::from_str(&json).expect("deserialize");

		assert_eq!(restored.visibility, ThreadVisibility::Private);
		assert!(restored.is_shared_with_support);
	}

	/// **Property: Thread with visibility roundtrips through JSON**
	///
	/// Why this is important: Visibility must survive serialization for
	/// both local persistence and server sync.
	#[test]
	fn test_thread_visibility_json_roundtrip() {
		let mut thread = Thread::new();
		thread.visibility = ThreadVisibility::Public;
		thread.is_private = true;

		let json = serde_json::to_string(&thread).expect("serialize");
		let restored: Thread = serde_json::from_str(&json).expect("deserialize");

		assert_eq!(restored.visibility, ThreadVisibility::Public);
		assert!(restored.is_private);
	}

	/// **Property: Git metadata survives JSON roundtrip**
	///
	/// Why this is important: Git metadata (branch and remote URL) is used for
	/// filtering threads by repository and branch in the UI. If these fields
	/// are lost or corrupted during serialization, users cannot find threads
	/// associated with specific repos/branches, breaking workspace-based
	/// navigation.
	///
	/// Invariant: git_branch and git_remote_url preserve their values through
	/// serialize -> deserialize cycles for both Thread and ThreadSummary
	#[test]
	fn test_git_metadata_json_roundtrip() {
		let mut thread = Thread::new();
		thread.git_branch = Some("feature/my-branch".to_string());
		thread.git_remote_url = Some("github.com/owner/repo".to_string());

		let json = serde_json::to_string(&thread).expect("serialize");
		let restored: Thread = serde_json::from_str(&json).expect("deserialize");

		assert_eq!(restored.git_branch, Some("feature/my-branch".to_string()));
		assert_eq!(
			restored.git_remote_url,
			Some("github.com/owner/repo".to_string())
		);

		let summary = ThreadSummary::from(&thread);
		let summary_json = serde_json::to_string(&summary).expect("serialize summary");
		let restored_summary: ThreadSummary =
			serde_json::from_str(&summary_json).expect("deserialize summary");

		assert_eq!(
			restored_summary.git_branch,
			Some("feature/my-branch".to_string())
		);
		assert_eq!(
			restored_summary.git_remote_url,
			Some("github.com/owner/repo".to_string())
		);
	}

	/// **Property: Git metadata None values are omitted from JSON**
	///
	/// Why this is important: Optional fields should not clutter the JSON when
	/// absent. This reduces storage size and maintains backward compatibility
	/// with older clients that don't recognize these fields.
	///
	/// Invariant: When git_branch and git_remote_url are None, they do not
	/// appear in the serialized JSON output
	#[test]
	fn test_git_metadata_none_omitted_from_json() {
		let thread = Thread::new();
		assert!(thread.git_branch.is_none());
		assert!(thread.git_remote_url.is_none());

		let json = serde_json::to_string(&thread).expect("serialize");
		assert!(!json.contains("git_branch"));
		assert!(!json.contains("git_remote_url"));

		let summary = ThreadSummary::from(&thread);
		let summary_json = serde_json::to_string(&summary).expect("serialize summary");
		assert!(!summary_json.contains("git_branch"));
		assert!(!summary_json.contains("git_remote_url"));
	}

	/// **Property: Advanced git metadata fields survive JSON roundtrip**
	///
	/// Why this is important: These fields track the complete git state
	/// throughout a coding session - initial branch/commit, current commit, dirty
	/// state, and all commits observed. This data is essential for:
	/// 1. Audit trails: Understanding what code state the AI was working with
	/// 2. Reproducibility: Recreating the exact environment for debugging
	/// 3. Session analysis: Tracking how many commits were made during a session
	/// 4. Conflict detection: Knowing if uncommitted changes existed
	///
	/// If any of these fields are lost or corrupted during serialization, users
	/// lose visibility into the git context of their AI sessions, making it
	/// impossible to correlate thread activity with repository history.
	///
	/// Invariant: All advanced git metadata fields (git_initial_branch,
	/// git_initial_commit_sha, git_current_commit_sha, git_start_dirty,
	/// git_end_dirty, git_commits) preserve their values through
	/// serialize -> deserialize cycles
	#[test]
	fn test_advanced_git_metadata_json_roundtrip() {
		let mut thread = Thread::new();
		thread.git_initial_branch = Some("feature/add-auth".to_string());
		thread.git_initial_commit_sha = Some("abc1234def5678".to_string());
		thread.git_current_commit_sha = Some("def5678abc1234".to_string());
		thread.git_start_dirty = Some(true);
		thread.git_end_dirty = Some(false);
		thread.git_commits = vec![
			"abc1234def5678".to_string(),
			"111222333444555".to_string(),
			"def5678abc1234".to_string(),
		];

		let json = serde_json::to_string(&thread).expect("serialize");
		let restored: Thread = serde_json::from_str(&json).expect("deserialize");

		assert_eq!(
			restored.git_initial_branch,
			Some("feature/add-auth".to_string())
		);
		assert_eq!(
			restored.git_initial_commit_sha,
			Some("abc1234def5678".to_string())
		);
		assert_eq!(
			restored.git_current_commit_sha,
			Some("def5678abc1234".to_string())
		);
		assert_eq!(restored.git_start_dirty, Some(true));
		assert_eq!(restored.git_end_dirty, Some(false));
		assert_eq!(restored.git_commits.len(), 3);
		assert_eq!(restored.git_commits[0], "abc1234def5678");
		assert_eq!(restored.git_commits[1], "111222333444555");
		assert_eq!(restored.git_commits[2], "def5678abc1234");

		let summary = ThreadSummary::from(&thread);
		let summary_json = serde_json::to_string(&summary).expect("serialize summary");
		let restored_summary: ThreadSummary =
			serde_json::from_str(&summary_json).expect("deserialize summary");

		assert_eq!(
			restored_summary.git_initial_commit_sha,
			Some("abc1234def5678".to_string())
		);
		assert_eq!(
			restored_summary.git_current_commit_sha,
			Some("def5678abc1234".to_string())
		);
	}
}
