// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Session state management for ACP integration.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use agent_client_protocol::SessionId;
use loom_common_core::Message;
use loom_common_thread::{Thread, ThreadId};
use tokio::sync::oneshot;

/// A request to send a session notification to the client.
pub struct SessionNotificationRequest {
	pub notification: agent_client_protocol::SessionNotification,
	pub completion_tx: oneshot::Sender<()>,
}

/// Per-session state, mapping an ACP session to Loom internals.
pub struct SessionState {
	/// The ACP session ID
	pub session_id: SessionId,

	/// Corresponding Loom thread ID
	pub thread_id: ThreadId,

	/// Thread data (loaded/created)
	pub thread: Thread,

	/// Workspace root for this session's tool execution
	pub workspace_root: PathBuf,

	/// Conversation messages for LLM requests
	pub messages: Vec<Message>,

	/// Cancellation flag - set when client sends cancel notification
	pub cancelled: Arc<AtomicBool>,
}

impl SessionState {
	/// Create a new session state from a thread.
	pub fn new(session_id: SessionId, thread: Thread, workspace_root: PathBuf) -> Self {
		let thread_id = thread.id.clone();

		// Rebuild messages from thread conversation using bridge
		let messages = crate::bridge::thread_to_messages(&thread);

		Self {
			session_id,
			thread_id,
			thread,
			workspace_root,
			messages,
			cancelled: Arc::new(AtomicBool::new(false)),
		}
	}

	/// Check if this session has been cancelled.
	pub fn is_cancelled(&self) -> bool {
		self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
	}

	/// Mark this session as cancelled.
	pub fn cancel(&self) {
		self
			.cancelled
			.store(true, std::sync::atomic::Ordering::Relaxed);
	}
}
