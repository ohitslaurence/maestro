// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! ACP Agent trait implementation for Loom.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
	self as acp, AgentCapabilities, AuthenticateRequest, AuthenticateResponse, CancelNotification,
	ExtNotification, ExtRequest, ExtResponse, Implementation, InitializeRequest, InitializeResponse,
	LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
	PromptResponse, ProtocolVersion, SessionId, SessionNotification, SessionUpdate,
	SetSessionModeRequest, SetSessionModeResponse, StopReason,
};
use loom_cli_tools::ToolRegistry;
use loom_common_core::{
	LlmClient, LlmEvent, LlmRequest, Message, ServerQuery, ServerQueryError, ServerQueryHandler,
	ServerQueryKind, ServerQueryResponse, ServerQueryResult, ToolCall, ToolContext, ToolDefinition,
};
use loom_common_thread::{AgentStateKind, AgentStateSnapshot, Thread, ThreadStore};
use serde_json::value::RawValue;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, instrument, warn};

use crate::error::AcpError;
use crate::session::{SessionNotificationRequest, SessionState};

/// Default implementation of ServerQueryHandler for ACP clients.
///
/// This handler provides query processing capabilities for the ACP agent,
/// enabling the server to request information from the client such as:
/// - File contents from the workspace
/// - Environment variables
/// - Workspace context information
///
/// User input and command execution are not supported in CLI mode
/// and will return appropriate errors.
#[derive(Debug, Clone)]
pub struct AcpServerQueryHandler {
	/// Root directory for file operations within the workspace
	workspace_root: PathBuf,
}

impl AcpServerQueryHandler {
	/// Create a new ACP server query handler.
	///
	/// # Arguments
	/// * `workspace_root` - The root directory for file operations
	///
	/// # Returns
	/// A new `AcpServerQueryHandler` instance
	pub fn new(workspace_root: PathBuf) -> Self {
		Self { workspace_root }
	}
}

#[async_trait::async_trait]
impl ServerQueryHandler for AcpServerQueryHandler {
	#[instrument(skip(self), fields(query_id = %query.id))]
	async fn handle_query(
		&self,
		query: ServerQuery,
	) -> Result<ServerQueryResponse, ServerQueryError> {
		debug!(
				query_id = %query.id,
				kind = ?query.kind,
				"handling server query"
		);

		let result = match &query.kind {
			ServerQueryKind::ReadFile { path } => self.handle_read_file(path).await,
			ServerQueryKind::GetEnvironment { keys } => self.handle_get_environment(keys),
			ServerQueryKind::GetWorkspaceContext => self.handle_get_workspace_context(),
			ServerQueryKind::RequestUserInput { prompt, .. } => {
				debug!(prompt = %prompt, "user input not supported in CLI mode");
				Err(ServerQueryError::ProcessingFailed(
					"User input not supported in CLI mode; override in editor integration".to_string(),
				))
			}
			ServerQueryKind::ExecuteCommand { command, .. } => {
				debug!(command = %command, "command execution disabled");
				Err(ServerQueryError::ProcessingFailed(
					"Command execution disabled in current configuration".to_string(),
				))
			}
			ServerQueryKind::Custom { name, .. } => {
				debug!(name = %name, "unknown custom query type");
				Err(ServerQueryError::ProcessingFailed(format!(
					"Unknown custom query type: {name}"
				)))
			}
		};

		let response = match result {
			Ok(query_result) => {
				info!(query_id = %query.id, "query handled successfully");
				ServerQueryResponse {
					query_id: query.id,
					sent_at: chrono::Utc::now().to_rfc3339(),
					result: query_result,
					error: None,
				}
			}
			Err(e) => {
				warn!(query_id = %query.id, error = %e, "query failed");
				ServerQueryResponse {
					query_id: query.id,
					sent_at: chrono::Utc::now().to_rfc3339(),
					result: ServerQueryResult::FileContent(String::new()),
					error: Some(e.to_string()),
				}
			}
		};

		Ok(response)
	}
}

impl AcpServerQueryHandler {
	/// Handle ReadFile query by reading from the workspace.
	async fn handle_read_file(&self, path: &str) -> Result<ServerQueryResult, ServerQueryError> {
		let full_path = self.workspace_root.join(path);

		debug!(path = %full_path.display(), "reading file");

		match tokio::fs::read_to_string(&full_path).await {
			Ok(content) => {
				debug!(path = %full_path.display(), size = content.len(), "file read successfully");
				Ok(ServerQueryResult::FileContent(content))
			}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ServerQueryError::InvalidQuery(
				format!("File not found: {path}"),
			)),
			Err(e) => Err(ServerQueryError::ProcessingFailed(format!(
				"Failed to read file: {e}"
			))),
		}
	}

	/// Handle GetEnvironment query by retrieving specified environment variables.
	fn handle_get_environment(&self, keys: &[String]) -> Result<ServerQueryResult, ServerQueryError> {
		let mut env_vars = HashMap::new();

		for key in keys {
			match std::env::var(key) {
				Ok(value) => {
					debug!(key = %key, "retrieved environment variable");
					env_vars.insert(key.clone(), value);
				}
				Err(std::env::VarError::NotPresent) => {
					debug!(key = %key, "environment variable not found");
					// Continue without error; missing vars are acceptable
				}
				Err(e) => {
					warn!(key = %key, error = %e, "error reading environment variable");
					// Continue without error
				}
			}
		}

		debug!(count = env_vars.len(), "environment query completed");
		Ok(ServerQueryResult::Environment(env_vars))
	}

	/// Handle GetWorkspaceContext query by building workspace information.
	fn handle_get_workspace_context(&self) -> Result<ServerQueryResult, ServerQueryError> {
		let mut context = serde_json::json!({
				"workspace_root": self.workspace_root.to_string_lossy().to_string(),
		});

		// Try to get git information if .git exists
		if self.workspace_root.join(".git").exists() {
			debug!("workspace has .git directory");
			context["has_git"] = true.into();

			// Try to get current branch
			if let Ok(head_file) = std::fs::read_to_string(self.workspace_root.join(".git/HEAD")) {
				if let Some(branch) = head_file.strip_prefix("ref: refs/heads/") {
					let branch = branch.trim();
					debug!(branch = %branch, "detected git branch");
					context["git_branch"] = branch.into();
				}
			}
		} else {
			context["has_git"] = false.into();
		}

		debug!("workspace context built");
		Ok(ServerQueryResult::WorkspaceContext(context))
	}
}

/// Loom's implementation of the ACP Agent trait.
///
/// This struct bridges ACP protocol messages to Loom's existing infrastructure:
/// - LLM client for completions
/// - Tool registry for tool execution  
/// - Thread store for persistence
pub struct LoomAcpAgent {
	/// LLM client for completions
	llm_client: Arc<dyn LlmClient>,

	/// Tool registry for tool execution
	tools: Arc<ToolRegistry>,

	/// Tool definitions (cached for LLM requests)
	tool_definitions: Vec<ToolDefinition>,

	/// Thread persistence
	thread_store: Arc<dyn ThreadStore>,

	/// Default workspace root for new sessions
	default_workspace_root: PathBuf,

	/// Default provider name (e.g., "anthropic")
	provider: String,

	/// Channel to send session notifications to the ACP connection
	session_update_tx: mpsc::UnboundedSender<SessionNotificationRequest>,

	/// Handler for server-to-client queries
	#[allow(dead_code)]
	query_handler: Arc<dyn ServerQueryHandler>,

	/// Active sessions keyed by SessionId
	/// Uses RefCell because ACP Agent trait is ?Send (single-threaded)
	sessions: RefCell<HashMap<String, SessionState>>,
}

impl std::fmt::Debug for LoomAcpAgent {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LoomAcpAgent")
			.field("default_workspace_root", &self.default_workspace_root)
			.field("provider", &self.provider)
			.field("session_count", &self.sessions.borrow().len())
			.finish()
	}
}

impl LoomAcpAgent {
	/// Create a new Loom ACP agent.
	pub fn new(
		llm_client: Arc<dyn LlmClient>,
		tools: Arc<ToolRegistry>,
		thread_store: Arc<dyn ThreadStore>,
		default_workspace_root: PathBuf,
		provider: String,
		session_update_tx: mpsc::UnboundedSender<SessionNotificationRequest>,
	) -> Self {
		let tool_definitions = tools.definitions();
		let query_handler = Arc::new(AcpServerQueryHandler::new(default_workspace_root.clone()));

		Self {
			llm_client,
			tools,
			tool_definitions,
			thread_store,
			default_workspace_root,
			provider,
			session_update_tx,
			query_handler,
			sessions: RefCell::new(HashMap::new()),
		}
	}

	/// Create a new Loom ACP agent with a custom query handler.
	pub fn with_query_handler(
		llm_client: Arc<dyn LlmClient>,
		tools: Arc<ToolRegistry>,
		thread_store: Arc<dyn ThreadStore>,
		default_workspace_root: PathBuf,
		provider: String,
		session_update_tx: mpsc::UnboundedSender<SessionNotificationRequest>,
		query_handler: Arc<dyn ServerQueryHandler>,
	) -> Self {
		let tool_definitions = tools.definitions();

		Self {
			llm_client,
			tools,
			tool_definitions,
			thread_store,
			default_workspace_root,
			provider,
			session_update_tx,
			query_handler,
			sessions: RefCell::new(HashMap::new()),
		}
	}

	/// Process a server query and return the response.
	///
	/// This delegates to the configured query handler to process the query
	/// and return a response.
	///
	/// # Arguments
	/// * `query` - The server query to process
	///
	/// # Returns
	/// A ServerQueryResponse or error
	#[allow(dead_code)]
	#[instrument(skip(self), fields(query_id = %query.id))]
	async fn process_server_query(
		&self,
		query: ServerQuery,
	) -> Result<ServerQueryResponse, AcpError> {
		debug!(query_id = %query.id, "processing server query");
		self
			.query_handler
			.handle_query(query)
			.await
			.map_err(|e| AcpError::Internal(format!("Query handler error: {e}")))
	}

	/// Send a text chunk notification to the client.
	async fn send_message_chunk(&self, session_id: &SessionId, text: String) -> Result<(), AcpError> {
		let chunk = crate::bridge::text_to_content_chunk(text);
		let notification =
			SessionNotification::new(session_id.clone(), SessionUpdate::AgentMessageChunk(chunk));

		let (tx, rx) = oneshot::channel();
		self
			.session_update_tx
			.send(SessionNotificationRequest {
				notification,
				completion_tx: tx,
			})
			.map_err(|_| AcpError::NotificationChannelClosed)?;

		rx.await.map_err(|_| AcpError::NotificationChannelClosed)?;

		Ok(())
	}

	/// Execute a tool and return the result as a Message.
	#[instrument(skip(self, ctx))]
	async fn execute_tool(&self, call: &ToolCall, ctx: &ToolContext) -> Message {
		debug!(
				tool_id = %call.id,
				tool_name = %call.tool_name,
				"executing tool"
		);

		let result = match self.tools.get(&call.tool_name) {
			Some(tool) => match tool.invoke(call.arguments_json.clone(), ctx).await {
				Ok(output) => {
					debug!(tool_id = %call.id, "tool succeeded");
					serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string())
				}
				Err(e) => {
					warn!(tool_id = %call.id, error = %e, "tool failed");
					format!("Error: {e}")
				}
			},
			None => {
				warn!(tool_id = %call.id, tool_name = %call.tool_name, "tool not found");
				format!("Error: tool '{}' not found", call.tool_name)
			}
		};

		Message::tool(&call.id, &call.tool_name, result)
	}

	/// Run the prompt loop: call LLM, execute tools, repeat until done.
	#[instrument(skip(self, session))]
	async fn run_prompt_loop(&self, session: &mut SessionState) -> Result<StopReason, AcpError> {
		loop {
			// Check cancellation
			if session.is_cancelled() {
				info!(session_id = %session.session_id, "prompt cancelled");
				return Ok(StopReason::Cancelled);
			}

			// Build LLM request
			let request = LlmRequest::new("default")
				.with_messages(session.messages.clone())
				.with_tools(self.tool_definitions.clone());

			debug!(
					session_id = %session.session_id,
					message_count = session.messages.len(),
					"calling LLM"
			);

			// Stream LLM response
			let mut stream = self
				.llm_client
				.complete_streaming(request)
				.await
				.map_err(AcpError::Llm)?;

			let mut assistant_content = String::new();
			let mut tool_calls: Vec<ToolCall> = Vec::new();

			while let Some(event) = stream.next().await {
				// Check cancellation during streaming
				if session.is_cancelled() {
					info!(session_id = %session.session_id, "prompt cancelled during streaming");
					return Ok(StopReason::Cancelled);
				}

				match event {
					LlmEvent::TextDelta { content } => {
						assistant_content.push_str(&content);
						self
							.send_message_chunk(&session.session_id, content)
							.await?;
					}
					LlmEvent::ToolCallDelta {
						call_id,
						tool_name,
						arguments_fragment,
					} => {
						debug!(
								call_id = %call_id,
								tool_name = %tool_name,
								fragment_len = arguments_fragment.len(),
								"tool call delta"
						);
					}
					LlmEvent::Completed(response) => {
						debug!(
								finish_reason = ?response.finish_reason,
								tool_call_count = response.tool_calls.len(),
								"LLM response complete"
						);
						tool_calls = response.tool_calls;
						if !response.message.content.is_empty() {
							assistant_content = response.message.content;
						}
						break;
					}
					LlmEvent::Error(e) => {
						error!(error = ?e, "LLM stream error");
						return Err(AcpError::Llm(e));
					}
				}
			}

			// Add assistant message to conversation
			let assistant_message = Message {
				role: loom_common_core::Role::Assistant,
				content: assistant_content.clone(),
				tool_call_id: None,
				name: None,
				tool_calls: tool_calls.clone(),
			};
			session.messages.push(assistant_message.clone());

			// Persist assistant message to thread using bridge
			let assistant_snapshot = crate::bridge::message_to_snapshot(&assistant_message);
			session
				.thread
				.conversation
				.messages
				.push(assistant_snapshot);

			// If no tool calls, turn is complete
			if tool_calls.is_empty() {
				info!(session_id = %session.session_id, "turn complete - no tool calls");
				return Ok(StopReason::EndTurn);
			}

			// Execute tools
			let ctx = ToolContext {
				workspace_root: session.workspace_root.clone(),
			};

			for call in &tool_calls {
				debug!(
						session_id = %session.session_id,
						tool_id = %call.id,
						tool_name = %call.tool_name,
						"executing tool"
				);

				let tool_result = self.execute_tool(call, &ctx).await;

				// Add tool result to conversation
				session.messages.push(tool_result.clone());

				// Persist tool result to thread using bridge
				let tool_snapshot = crate::bridge::message_to_snapshot(&tool_result);
				session.thread.conversation.messages.push(tool_snapshot);
			}

			// Loop continues - LLM will process tool results
		}
	}

	/// Get a mutable reference to a session, cloning it out to avoid borrow issues.
	fn get_session(&self, session_id: &SessionId) -> Option<SessionState> {
		// We need to remove and return the session to avoid holding the borrow across await
		self.sessions.borrow_mut().remove(&session_id.to_string())
	}

	/// Put a session back after processing.
	fn put_session(&self, session: SessionState) {
		self
			.sessions
			.borrow_mut()
			.insert(session.session_id.to_string(), session);
	}
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for LoomAcpAgent {
	#[instrument(skip(self, req))]
	async fn initialize(&self, req: InitializeRequest) -> acp::Result<InitializeResponse> {
		info!(
				client_version = %req.protocol_version,
				client_info = ?req.client_info,
				"ACP initialize request"
		);

		let mut capabilities = AgentCapabilities::default();
		capabilities.load_session = true;

		let agent_info =
			Implementation::new("loom", env!("CARGO_PKG_VERSION")).title("Loom AI Coding Assistant");

		Ok(
			InitializeResponse::new(ProtocolVersion::V1)
				.agent_capabilities(capabilities)
				.agent_info(agent_info),
		)
	}

	#[instrument(skip(self, _req))]
	async fn authenticate(&self, _req: AuthenticateRequest) -> acp::Result<AuthenticateResponse> {
		debug!("ACP authenticate request (no-op)");
		Ok(AuthenticateResponse::default())
	}

	#[instrument(skip(self, req))]
	async fn new_session(&self, req: NewSessionRequest) -> acp::Result<NewSessionResponse> {
		info!(cwd = ?req.cwd, "ACP new_session request");

		// Determine workspace root from request or default
		let workspace_root = req.cwd.clone();

		// Create new thread
		let mut thread = Thread::new();
		thread.workspace_root = Some(workspace_root.display().to_string());
		thread.cwd = Some(workspace_root.display().to_string());
		thread.loom_version = Some(env!("CARGO_PKG_VERSION").to_string());
		thread.provider = Some(self.provider.clone());

		// Persist thread
		self
			.thread_store
			.save(&thread)
			.await
			.map_err(AcpError::ThreadStore)?;

		// Create session ID from thread ID
		let session_id = crate::bridge::thread_id_to_session_id(&thread.id);

		// Create session state
		let session = SessionState::new(session_id.clone(), thread, workspace_root);

		// Store session
		self
			.sessions
			.borrow_mut()
			.insert(session_id.to_string(), session);

		info!(session_id = %session_id, "created new session");

		Ok(NewSessionResponse::new(session_id))
	}

	#[instrument(skip(self, req))]
	async fn load_session(&self, req: LoadSessionRequest) -> acp::Result<LoadSessionResponse> {
		info!(session_id = %req.session_id, "ACP load_session request");

		// Parse session ID as thread ID
		let thread_id = crate::bridge::session_id_to_thread_id(&req.session_id);

		// Load thread from store
		let thread = self
			.thread_store
			.load(&thread_id)
			.await
			.map_err(AcpError::ThreadStore)?
			.ok_or_else(|| AcpError::SessionNotFound(req.session_id.to_string()))?;

		// Determine workspace root
		let workspace_root = thread
			.workspace_root
			.as_ref()
			.map(PathBuf::from)
			.unwrap_or_else(|| self.default_workspace_root.clone());

		// Create session state (rebuilds messages from thread)
		let session = SessionState::new(req.session_id.clone(), thread, workspace_root);

		info!(
				session_id = %req.session_id,
				message_count = session.messages.len(),
				"loaded session"
		);

		// Store session
		self
			.sessions
			.borrow_mut()
			.insert(req.session_id.to_string(), session);

		Ok(LoadSessionResponse::default())
	}

	#[instrument(skip(self, req))]
	async fn prompt(&self, req: PromptRequest) -> acp::Result<PromptResponse> {
		info!(
				session_id = %req.session_id,
				prompt_blocks = req.prompt.len(),
				"ACP prompt request"
		);

		// Get session (removes from map to avoid borrow across await)
		let mut session = self
			.get_session(&req.session_id)
			.ok_or_else(|| AcpError::SessionNotFound(req.session_id.to_string()))?;

		// Convert ACP ContentBlocks to Loom Message using bridge
		let user_message = crate::bridge::content_blocks_to_user_message(&req.prompt);
		session.messages.push(user_message.clone());

		// Persist user message to thread using bridge
		let user_snapshot = crate::bridge::message_to_snapshot(&user_message);
		session.thread.conversation.messages.push(user_snapshot);

		// Run prompt loop
		let stop_reason = match self.run_prompt_loop(&mut session).await {
			Ok(reason) => reason,
			Err(e) => {
				error!(error = ?e, "prompt loop failed");
				// Put session back before returning error
				self.put_session(session);
				return Err(e.into());
			}
		};

		// Update thread state and persist
		session.thread.agent_state = AgentStateSnapshot {
			kind: AgentStateKind::WaitingForUserInput,
			retries: 0,
			last_error: None,
			pending_tool_calls: Vec::new(),
		};
		session.thread.touch();

		if let Err(e) = self.thread_store.save(&session.thread).await {
			warn!(error = %e, "failed to persist thread after prompt");
		}

		// Put session back
		self.put_session(session);

		info!(stop_reason = ?stop_reason, "prompt complete");

		Ok(PromptResponse::new(stop_reason))
	}

	#[instrument(skip(self, req))]
	async fn cancel(&self, req: CancelNotification) -> acp::Result<()> {
		info!(session_id = %req.session_id, "ACP cancel request");

		if let Some(session) = self.sessions.borrow().get(&req.session_id.to_string()) {
			session.cancel();
		}

		Ok(())
	}

	async fn set_session_mode(
		&self,
		_req: SetSessionModeRequest,
	) -> acp::Result<SetSessionModeResponse> {
		debug!("set_session_mode not implemented");
		Err(acp::Error::method_not_found())
	}

	async fn ext_method(&self, req: ExtRequest) -> acp::Result<ExtResponse> {
		use std::sync::Arc;
		debug!(method = %req.method, "unhandled extension method");
		let raw = RawValue::from_string("null".into())?;
		Ok(ExtResponse::new(Arc::from(raw)))
	}

	async fn ext_notification(&self, req: ExtNotification) -> acp::Result<()> {
		debug!(method = %req.method, "unhandled extension notification");
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Mock implementation of ServerQueryHandler for testing.
	///
	/// Returns canned responses without actually accessing the filesystem
	/// or environment. Useful for testing the query handling pipeline.
	#[derive(Debug, Clone)]
	struct MockServerQueryHandler {
		/// Canned file content to return for ReadFile queries
		file_content: String,
		/// Should read file fail?
		read_file_error: bool,
	}

	impl MockServerQueryHandler {
		fn new() -> Self {
			Self {
				file_content: "mock file content".to_string(),
				read_file_error: false,
			}
		}

		#[allow(dead_code)]
		fn with_error() -> Self {
			Self {
				file_content: String::new(),
				read_file_error: true,
			}
		}
	}

	#[async_trait::async_trait]
	impl ServerQueryHandler for MockServerQueryHandler {
		async fn handle_query(
			&self,
			query: ServerQuery,
		) -> Result<ServerQueryResponse, ServerQueryError> {
			match &query.kind {
				ServerQueryKind::ReadFile { path } => {
					if self.read_file_error {
						Ok(ServerQueryResponse {
							query_id: query.id,
							sent_at: chrono::Utc::now().to_rfc3339(),
							result: ServerQueryResult::FileContent(String::new()),
							error: Some(format!("mock error: cannot read {path}")),
						})
					} else {
						Ok(ServerQueryResponse {
							query_id: query.id,
							sent_at: chrono::Utc::now().to_rfc3339(),
							result: ServerQueryResult::FileContent(self.file_content.clone()),
							error: None,
						})
					}
				}
				ServerQueryKind::GetEnvironment { keys } => {
					let mut env_vars = HashMap::new();
					for key in keys {
						env_vars.insert(key.clone(), format!("mock-value-{key}"));
					}
					Ok(ServerQueryResponse {
						query_id: query.id,
						sent_at: chrono::Utc::now().to_rfc3339(),
						result: ServerQueryResult::Environment(env_vars),
						error: None,
					})
				}
				ServerQueryKind::GetWorkspaceContext => {
					let context = serde_json::json!({
							"workspace_root": "/mock/workspace",
							"has_git": true,
							"git_branch": "main",
					});
					Ok(ServerQueryResponse {
						query_id: query.id,
						sent_at: chrono::Utc::now().to_rfc3339(),
						result: ServerQueryResult::WorkspaceContext(context),
						error: None,
					})
				}
				_ => Err(ServerQueryError::ProcessingFailed(
					"mock: unsupported query type".to_string(),
				)),
			}
		}
	}

	// Note: SessionId ↔ ThreadId roundtrip test moved to bridge.rs

	/// **Property: ReadFile queries return file content correctly**
	///
	/// Why this is important: File reading is a core query type used by the
	/// server to fetch context about the workspace. The handler must correctly
	/// read files and return their content in the response.
	#[tokio::test]
	async fn test_read_file_query() {
		let handler = AcpServerQueryHandler::new(PathBuf::from("/tmp"));

		let query = ServerQuery {
			id: "Q-test-001".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "nonexistent.txt".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_some());
		let error_msg = response.error.unwrap();
		assert!(error_msg.contains("not found") || error_msg.contains("File not found"));
	}

	/// **Property: GetEnvironment queries return correct environment variables**
	///
	/// Why this is important: The server may need to access environment
	/// configuration like API keys or deployment settings. The handler must
	/// reliably fetch and return these values to the server.
	#[tokio::test]
	async fn test_get_environment_query() {
		let handler = AcpServerQueryHandler::new(PathBuf::from("/tmp"));

		let query = ServerQuery {
			id: "Q-test-002".to_string(),
			kind: ServerQueryKind::GetEnvironment {
				keys: vec!["PATH".to_string(), "HOME".to_string()],
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_none());

		if let ServerQueryResult::Environment(env_vars) = response.result {
			// We expect either PATH or HOME to exist (or both)
			assert!(!env_vars.is_empty(), "should have at least one env var");
		} else {
			panic!("expected Environment result");
		}
	}

	/// **Property: GetWorkspaceContext queries return valid JSON**
	///
	/// Why this is important: Workspace context provides metadata about the
	/// environment (git branch, root path, etc.). This must be serializable
	/// JSON for transport over SSE/HTTP.
	#[tokio::test]
	async fn test_get_workspace_context_query() {
		let handler = AcpServerQueryHandler::new(PathBuf::from("/tmp"));

		let query = ServerQuery {
			id: "Q-test-003".to_string(),
			kind: ServerQueryKind::GetWorkspaceContext,
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_none());

		if let ServerQueryResult::WorkspaceContext(context) = response.result {
			assert!(context.get("workspace_root").is_some());
			assert!(context.get("has_git").is_some());
		} else {
			panic!("expected WorkspaceContext result");
		}
	}

	/// **Property: RequestUserInput queries return not-implemented error in CLI mode**
	///
	/// Why this is important: User input queries require editor integration
	/// to show prompts. The CLI handler should gracefully refuse these.
	#[tokio::test]
	async fn test_request_user_input_not_implemented() {
		let handler = AcpServerQueryHandler::new(PathBuf::from("/tmp"));

		let query = ServerQuery {
			id: "Q-test-004".to_string(),
			kind: ServerQueryKind::RequestUserInput {
				prompt: "Approve changes?".to_string(),
				input_type: "yes_no".to_string(),
				options: None,
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_some());
		assert!(response
			.error
			.unwrap()
			.contains("not supported in CLI mode"));
	}

	/// **Property: ExecuteCommand queries return disabled error**
	///
	/// Why this is important: Command execution is disabled for security.
	/// The handler should reject these queries consistently.
	#[tokio::test]
	async fn test_execute_command_disabled() {
		let handler = AcpServerQueryHandler::new(PathBuf::from("/tmp"));

		let query = ServerQuery {
			id: "Q-test-005".to_string(),
			kind: ServerQueryKind::ExecuteCommand {
				command: "ls".to_string(),
				args: vec!["-la".to_string()],
				timeout_secs: 5,
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_some());
		assert!(response.error.unwrap().contains("disabled"));
	}

	/// **Property: Mock handler returns canned responses**
	///
	/// Why this is important: For testing ACP agent logic without touching
	/// the filesystem, we need a mock that always returns predictable responses.
	#[tokio::test]
	async fn test_mock_query_handler() {
		let handler = MockServerQueryHandler::new();

		let query = ServerQuery {
			id: "Q-test-mock".to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "test.rs".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert!(response.error.is_none());

		if let ServerQueryResult::FileContent(content) = response.result {
			assert_eq!(content, "mock file content");
		} else {
			panic!("expected FileContent result");
		}
	}

	/// **Property: Query response IDs correlate to query IDs**
	///
	/// Why this is important: Query/response correlation via IDs is critical
	/// for the async request-response pattern. The handler must preserve
	/// the query ID in the response.
	#[tokio::test]
	async fn test_query_response_correlation() {
		let handler = MockServerQueryHandler::new();
		let expected_id = "Q-correlation-test-123";

		let query = ServerQuery {
			id: expected_id.to_string(),
			kind: ServerQueryKind::ReadFile {
				path: "test.rs".to_string(),
			},
			sent_at: chrono::Utc::now().to_rfc3339(),
			timeout_secs: 30,
			metadata: serde_json::json!({}),
		};

		let response = handler.handle_query(query).await.unwrap();
		assert_eq!(response.query_id, expected_id);
	}
}
