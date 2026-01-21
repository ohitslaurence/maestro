// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Loom CLI - Interactive AI coding assistant
//!
//! This binary provides a REPL interface for interacting with LLM-powered
//! coding agents. It supports multiple LLM providers and includes tools
//! for file system operations.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use loom_server_logs::RedactingMakeWriter;

use loom_cli_auto_commit::{
	AutoCommitConfig, AutoCommitResult, AutoCommitService, CompletedToolInfo,
};
use loom_cli_config::{
	load_config_with_cli,
	runtime::{LogFormat, LogLevel},
	sources::CliOverrides,
};
use loom_cli_git::{detect_repo_status, CommandGitClient};
use loom_common_core::{
	LlmClient, LlmEvent, Message, ToolCall, ToolContext, ToolDefinition, ToolExecutionOutcome,
};
use loom_common_thread::{
	AgentStateKind, AgentStateSnapshot, LocalThreadStore, MessageRole, MessageSnapshot,
	SyncingThreadStore, Thread, ThreadId, ThreadStore, ThreadSyncClient, ThreadVisibility,
	ToolCallSnapshot,
};
use loom_server_llm_proxy::{LlmProvider, ProxyLlmClient};

#[derive(clap::ValueEnum, Clone, Debug)]
enum ShareVisibilityArg {
	Organization,
	Private,
	Public,
}

impl From<ShareVisibilityArg> for ThreadVisibility {
	fn from(v: ShareVisibilityArg) -> Self {
		match v {
			ShareVisibilityArg::Organization => ThreadVisibility::Organization,
			ShareVisibilityArg::Private => ThreadVisibility::Private,
			ShareVisibilityArg::Public => ThreadVisibility::Public,
		}
	}
}
use loom_cli_tools::{
	BashTool, EditFileTool, ListFilesTool, OracleTool, ReadFileTool, ToolRegistry,
	WebSearchToolGoogle, WebSearchToolSerper,
};
use url::Url;

mod auth;
mod credential_helper;
mod locale;
mod update;
mod version;
mod weaver_client;

use locale::get_locale;

// loom-auto-commit now re-exports loom-git types directly, so we can use CommandGitClient

/// Loom - AI-powered coding assistant
#[derive(Parser, Debug)]
#[command(name = "loom", version, about, long_about = None)]
struct Args {
	/// Path to custom configuration file
	#[arg(short, long)]
	config: Option<PathBuf>,

	/// Workspace directory for file operations
	#[arg(short, long)]
	workspace: Option<PathBuf>,

	/// Log level (overrides config)
	#[arg(short, long)]
	log_level: Option<String>,

	/// Output logs as JSON (overrides config)
	#[arg(long)]
	json_logs: bool,

	/// Loom server URL for LLM proxy
	#[arg(long, env = "LOOM_SERVER_URL", default_value = "http://localhost:8080")]
	server_url: String,

	/// LLM provider to use (anthropic or openai)
	#[arg(short, long, env = "LOOM_LLM_PROVIDER", default_value = "anthropic")]
	provider: String,

	#[command(subcommand)]
	command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum WeaverCommand {
	/// Create a new remote weaver session
	New {
		/// Container image to use
		#[arg(long, short)]
		image: Option<String>,
		/// Organization ID (defaults to personal org if not specified)
		#[arg(long, short)]
		org: Option<String>,
		/// Git repository to clone (public https URL)
		#[arg(long)]
		repo: Option<String>,
		/// Branch to checkout
		#[arg(long)]
		branch: Option<String>,
		/// Environment variable (repeatable: -e KEY=VALUE)
		#[arg(long, short = 'e', value_name = "KEY=VALUE")]
		env: Vec<String>,
		/// Lifetime in hours (default: 4, max: 48)
		#[arg(long)]
		ttl: Option<u32>,
	},
	/// Attach to a running weaver
	Attach {
		/// Weaver ID to attach to
		weaver_id: String,
	},
	/// List running weavers
	Ps {
		/// Output as JSON
		#[arg(long)]
		json: bool,
	},
	/// Delete a weaver
	Delete {
		/// Weaver ID to delete
		weaver_id: String,
	},
}

#[derive(Subcommand, Debug)]
enum Command {
	/// Authenticate with Loom services
	Login,
	/// Log out from Loom services
	Logout,
	/// List local threads
	List,
	/// Resume an existing thread
	Resume {
		/// Thread ID to resume (uses most recent if not specified)
		thread_id: Option<String>,
	},
	/// Start a new private (local-only) session that never syncs
	Private,
	/// Change server-side visibility of a synced thread
	Share {
		/// Thread ID to share (uses most recent if not specified)
		thread_id: Option<String>,
		/// Desired visibility: organization, private, support, or public
		#[arg(long, value_enum, conflicts_with = "support")]
		visibility: Option<ShareVisibilityArg>,
		/// Share thread with support team (shortcut for --visibility support)
		#[arg(long)]
		support: bool,
	},
	/// Search threads by content, git metadata, or commit SHA
	Search {
		/// Search query (text, branch name, repo URL, or commit SHA prefix)
		query: String,
		/// Maximum number of results
		#[arg(short, long, default_value = "20")]
		limit: usize,
		/// Output raw JSON
		#[arg(long)]
		json: bool,
	},
	/// Show version and build information
	Version,
	/// Update Loom to the latest version from the server
	Update,
	/// Run as ACP agent over stdio (for editor integration)
	AcpAgent,
	/// Create a new remote weaver session (alias: weaver new)
	New {
		/// Container image to use
		#[arg(long, short)]
		image: Option<String>,
		/// Organization ID (defaults to personal org if not specified)
		#[arg(long, short)]
		org: Option<String>,
		/// Git repository to clone (public https URL)
		#[arg(long)]
		repo: Option<String>,
		/// Branch to checkout
		#[arg(long)]
		branch: Option<String>,
		/// Environment variable (repeatable: -e KEY=VALUE)
		#[arg(long, short = 'e', value_name = "KEY=VALUE")]
		env: Vec<String>,
		/// Lifetime in hours (default: 4, max: 48)
		#[arg(long)]
		ttl: Option<u32>,
	},
	/// Attach to a running weaver (alias: weaver attach)
	Attach {
		/// Weaver ID to attach to
		weaver_id: String,
	},
	/// Weaver management commands
	Weaver {
		#[command(subcommand)]
		command: WeaverCommand,
	},
	/// Git credential helper for Loom SCM authentication
	///
	/// Configure git to use this helper:
	///   git config --global credential.https://loom.ghuntley.com.helper 'loom credential-helper'
	#[command(name = "credential-helper")]
	CredentialHelper(credential_helper::CredentialHelperArgs),
	/// Spool version control (jj-based VCS with tapestry naming)
	Spool {
		#[command(subcommand)]
		command: loom_cli_spool::SpoolCommands,
	},
	/// WireGuard tunnel management
	Tunnel {
		#[command(subcommand)]
		command: loom_cli_wgtunnel::TunnelCommands,
	},
	/// SSH to a weaver through WireGuard tunnel
	Ssh(loom_cli_wgtunnel::SshArgs),
	/// WireGuard device management
	Wg {
		#[command(subcommand)]
		command: WgCommand,
	},
}

#[derive(Subcommand, Debug)]
enum WgCommand {
	/// Device management subcommands
	Devices {
		#[command(subcommand)]
		command: loom_cli_wgtunnel::DevicesCommands,
	},
}

impl From<&Args> for CliOverrides {
	fn from(args: &Args) -> Self {
		Self {
			provider: None,
			model: None,
			workspace: args.workspace.clone(),
			log_level: args.log_level.clone(),
			log_format: if args.json_logs {
				Some("json".to_string())
			} else {
				None
			},
			config_file: args.config.clone(),
		}
	}
}

fn log_level_to_tracing(level: LogLevel) -> tracing::Level {
	match level {
		LogLevel::Trace => tracing::Level::TRACE,
		LogLevel::Debug => tracing::Level::DEBUG,
		LogLevel::Info => tracing::Level::INFO,
		LogLevel::Warn => tracing::Level::WARN,
		LogLevel::Error => tracing::Level::ERROR,
	}
}

fn init_tracing(logging: &loom_cli_config::runtime::LoggingConfig) {
	let filter = EnvFilter::try_from_default_env()
		.unwrap_or_else(|_| EnvFilter::new(format!("loom={}", log_level_to_tracing(logging.level))));

	let redacting_writer = RedactingMakeWriter::new(std::io::stdout);

	match logging.format {
		LogFormat::Json => {
			tracing_subscriber::registry()
				.with(filter)
				.with(fmt::layer().json().with_writer(redacting_writer))
				.init();
		}
		LogFormat::Compact => {
			tracing_subscriber::registry()
				.with(filter)
				.with(fmt::layer().compact().with_writer(redacting_writer))
				.init();
		}
		LogFormat::Pretty => {
			tracing_subscriber::registry()
				.with(filter)
				.with(fmt::layer().with_writer(redacting_writer))
				.init();
		}
	}
}

fn create_llm_client(
	server_url: &str,
	provider: &str,
	auth_token: Option<loom_common_secret::SecretString>,
) -> Result<Arc<dyn LlmClient>> {
	let llm_provider = match provider.to_lowercase().as_str() {
		"anthropic" => LlmProvider::Anthropic,
		"openai" => LlmProvider::OpenAi,
		other => anyhow::bail!("Unknown LLM provider: {other}. Use 'anthropic' or 'openai'"),
	};

	info!(server_url = %server_url, provider = %provider, "creating proxy LLM client");
	let mut client = ProxyLlmClient::new(server_url, llm_provider);
	if let Some(token) = auth_token {
		client = client.with_auth_token(token);
	}
	Ok(Arc::new(client))
}

fn build_auto_commit_config() -> AutoCommitConfig {
	let disabled = std::env::var("LOOM_AUTO_COMMIT_DISABLE")
		.map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
		.unwrap_or(false);

	if disabled {
		debug!("auto-commit disabled via LOOM_AUTO_COMMIT_DISABLE env var");
	}

	AutoCommitConfig {
		enabled: !disabled,
		model: "claude-3-haiku-20240307".to_string(),
		max_diff_bytes: 32 * 1024,
		trigger_tools: vec!["edit_file".to_string(), "bash".to_string()],
	}
}

async fn run_auto_commit(
	service: &AutoCommitService<CommandGitClient, ProxyLlmClient>,
	workspace: &std::path::Path,
	completed_tools: &[CompletedToolInfo],
) -> AutoCommitResult {
	let result = service.run(workspace, completed_tools).await;

	if result.committed {
		info!(
				commit_hash = ?result.commit_hash,
				files_changed = result.files_changed,
				message = ?result.message,
				"auto-commit successful"
		);
		println!(
			"\n[Auto-commit: {}]",
			result.message.as_deref().unwrap_or("committed")
		);
	} else if let Some(ref reason) = result.skip_reason {
		debug!(reason = %reason, "auto-commit skipped");
	}

	result
}

#[instrument]
fn create_tool_registry() -> ToolRegistry {
	info!("creating tool registry");

	let mut registry = ToolRegistry::new();
	registry.register(Box::new(ReadFileTool::new()));
	registry.register(Box::new(ListFilesTool::new()));
	registry.register(Box::new(EditFileTool::new()));
	registry.register(Box::new(BashTool::new()));
	registry.register(Box::new(OracleTool::default()));
	registry.register(Box::new(WebSearchToolGoogle::default()));
	registry.register(Box::new(WebSearchToolSerper::default()));

	let definitions = registry.definitions();
	debug!(tool_count = definitions.len(), "tool registry initialized");

	registry
}

fn get_tool_definitions(registry: &ToolRegistry) -> Vec<ToolDefinition> {
	registry.definitions()
}

#[instrument(skip(registry, ctx))]
async fn execute_tool(
	registry: &ToolRegistry,
	tool_call: &ToolCall,
	ctx: &ToolContext,
) -> ToolExecutionOutcome {
	debug!(
			tool_id = %tool_call.id,
			tool_name = %tool_call.tool_name,
			"executing tool"
	);

	match registry.get(&tool_call.tool_name) {
		Some(tool) => match tool.invoke(tool_call.arguments_json.clone(), ctx).await {
			Ok(output) => {
				debug!(
						tool_id = %tool_call.id,
						"tool execution succeeded"
				);
				ToolExecutionOutcome::Success {
					call_id: tool_call.id.clone(),
					output,
				}
			}
			Err(e) => {
				warn!(
						tool_id = %tool_call.id,
						error = %e,
						"tool execution failed"
				);
				ToolExecutionOutcome::Error {
					call_id: tool_call.id.clone(),
					error: e,
				}
			}
		},
		None => {
			warn!(
					tool_id = %tool_call.id,
					tool_name = %tool_call.tool_name,
					"tool not found"
			);
			ToolExecutionOutcome::Error {
				call_id: tool_call.id.clone(),
				error: loom_common_core::ToolError::NotFound(tool_call.tool_name.clone()),
			}
		}
	}
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip(
	llm_client,
	tool_registry,
	tool_ctx,
	thread,
	thread_store,
	shutdown_rx,
	workspace,
	auto_commit_service
))]
async fn run_repl(
	llm_client: &dyn LlmClient,
	tool_registry: &ToolRegistry,
	tool_definitions: &[ToolDefinition],
	tool_ctx: &ToolContext,
	thread: &mut Thread,
	thread_store: &dyn ThreadStore,
	mut shutdown_rx: watch::Receiver<bool>,
	workspace: &std::path::Path,
	auto_commit_service: Option<&AutoCommitService<CommandGitClient, ProxyLlmClient>>,
) -> Result<()> {
	let stdin = tokio::io::stdin();
	let mut reader = BufReader::new(stdin);
	let mut stdout = io::stdout();

	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.repl.welcome")
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.repl.thread_id",
			&[("id", &thread.id.to_string())]
		)
	);
	println!(
		"{}\n",
		loom_common_i18n::t(get_locale(), "client.repl.instructions")
	);

	let mut messages: Vec<Message> = Vec::new();

	loop {
		print!("> ");
		stdout.flush()?;

		let mut input = String::new();

		tokio::select! {
			biased;

			_ = shutdown_rx.changed() => {
				if *shutdown_rx.borrow() {
					info!("shutdown requested, saving thread");
					snapshot_git_state(thread, workspace);
					thread.touch();
					if let Err(e) = thread_store.save(thread).await {
						warn!(error = %e, "failed to save thread on shutdown");
					}
					println!("{}", loom_common_i18n::t(get_locale(), "client.repl.interrupted"));
					break;
				}
			}

			result = reader.read_line(&mut input) => {
				match result {
					Ok(0) => {
						info!("EOF received, shutting down");
						snapshot_git_state(thread, workspace);
						thread.touch();
						if let Err(e) = thread_store.save(thread).await {
							warn!(error = %e, "failed to save thread on exit");
						}
						break;
					}
					Ok(_) => {
						let input = input.trim();
						if input.is_empty() {
							continue;
						}

						debug!(input_length = input.len(), "received user input");

						let user_message = Message::user(input);
						messages.push(user_message.clone());

						thread
							.conversation
							.messages
							.push(MessageSnapshot::from(&user_message));

						let request = loom_common_core::LlmRequest::new("default")
							.with_messages(messages.clone())
							.with_tools(tool_definitions.to_vec());

						match llm_client.complete_streaming(request).await {
							Ok(mut stream) => {
								let mut assistant_content = String::new();
								let mut tool_calls: Vec<ToolCall> = Vec::new();

								while let Some(event) = stream.next().await {
									match event {
										LlmEvent::TextDelta { content } => {
											print!("{content}");
											let _ = io::stdout().flush();
											assistant_content.push_str(&content);
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
											info!(
												finish_reason = ?response.finish_reason,
												"LLM response complete"
											);
											println!();
											tool_calls = response.tool_calls;
											if !response.message.content.is_empty() {
												assistant_content = response.message.content.clone();
											}
										}
										LlmEvent::Error(e) => {
											error!(error = ?e, "LLM stream error");
										}
									}
								}

								messages.push(Message::assistant_with_tool_calls(
									&assistant_content,
									tool_calls.clone(),
								));

								thread.conversation.messages.push(MessageSnapshot {
									role: MessageRole::Assistant,
									content: assistant_content.clone(),
									tool_call_id: None,
									tool_name: None,
									tool_calls: if tool_calls.is_empty() {
										None
									} else {
										Some(
											tool_calls
												.iter()
												.map(|tc| ToolCallSnapshot {
													id: tc.id.clone(),
													tool_name: tc.tool_name.clone(),
													arguments_json: tc.arguments_json.clone(),
												})
												.collect(),
										)
									},
								});

								let mut tool_outcomes: Vec<(String, bool)> = Vec::new();
								for tool_call in &tool_calls {
									info!(
										tool_name = %tool_call.tool_name,
										tool_id = %tool_call.id,
										"executing tool call"
									);

									let outcome =
										execute_tool(tool_registry, tool_call, tool_ctx).await;
									let succeeded =
										matches!(&outcome, ToolExecutionOutcome::Success { .. });
									tool_outcomes.push((tool_call.tool_name.clone(), succeeded));

									let (tool_result, is_error) = match &outcome {
										ToolExecutionOutcome::Success { output, .. } => {
											(output.to_string(), false)
										}
										ToolExecutionOutcome::Error { error, .. } => {
											(format!("Error: {error}"), true)
										}
									};

									if is_error {
										warn!(tool_id = %tool_call.id, result = %tool_result, "tool returned error");
									} else {
										debug!(
											tool_id = %tool_call.id,
											"tool completed successfully"
										);
									}

									messages.push(Message::tool(
										&tool_call.id,
										&tool_call.tool_name,
										&tool_result,
									));

									thread.conversation.messages.push(MessageSnapshot {
										role: MessageRole::Tool,
										content: tool_result.clone(),
										tool_call_id: Some(tool_call.id.clone()),
										tool_name: Some(tool_call.tool_name.clone()),
										tool_calls: None,
									});
								}

								if !tool_calls.is_empty() {
									if let Some(auto_commit_svc) = auto_commit_service {
										let completed: Vec<CompletedToolInfo> = tool_outcomes
											.iter()
											.map(|(name, succeeded)| CompletedToolInfo {
												tool_name: name.clone(),
												succeeded: *succeeded,
											})
											.collect();

										run_auto_commit(auto_commit_svc, workspace, &completed)
											.await;
									}
								}

								thread.agent_state = AgentStateSnapshot {
									kind: AgentStateKind::WaitingForUserInput,
									retries: 0,
									last_error: None,
									pending_tool_calls: Vec::new(),
								};
								snapshot_git_state(thread, workspace);
								thread.touch();

								if let Err(e) = thread_store.save(thread).await {
									warn!(error = %e, "failed to save thread");
								}
							}
							Err(e) => {
								error!(error = %e, "failed to start LLM request");
								eprintln!(
									"{}",
									loom_common_i18n::t_fmt(get_locale(), "client.repl.error", &[("error", &e.to_string())])
								);
							}
						}
					}
					Err(e) => {
						error!(error = %e, "failed to read input");
						break;
					}
				}
			}
		}
	}

	Ok(())
}

async fn start_repl_session(
	config: &loom_cli_config::LoomConfig,
	args: &Args,
	thread_store: Arc<dyn ThreadStore>,
	mut thread: Thread,
) -> Result<()> {
	let workspace = config
		.global
		.workspace_root
		.clone()
		.or_else(|| config.tools.workspace.root.clone())
		.unwrap_or_else(|| PathBuf::from("."))
		.canonicalize()
		.context("invalid workspace path")?;

	let token = auth::load_token(&args.server_url).await;
	let llm_client = create_llm_client(&args.server_url, &args.provider, token.clone())?;

	let tool_registry = create_tool_registry();
	let tool_definitions = get_tool_definitions(&tool_registry);
	let tool_ctx = ToolContext::new(&workspace);

	let auto_commit_config = build_auto_commit_config();
	let auto_commit_enabled = auto_commit_config.enabled;
	let auto_commit_service = if auto_commit_enabled {
		let git_client = Arc::new(CommandGitClient::new());
		let mut haiku = ProxyLlmClient::new(&args.server_url, LlmProvider::Anthropic);
		if let Some(t) = &token {
			haiku = haiku.with_auth_token(t.clone());
		}
		let haiku_client = Arc::new(haiku);
		Some(AutoCommitService::new(
			git_client,
			haiku_client,
			auto_commit_config,
		))
	} else {
		None
	};

	info!(
			tool_count = tool_definitions.len(),
			workspace = %workspace.display(),
			auto_commit = auto_commit_enabled,
			"initialized"
	);

	let (shutdown_tx, shutdown_rx) = watch::channel(false);
	setup_ctrlc_handler(shutdown_tx)?;

	run_repl(
		llm_client.as_ref(),
		&tool_registry,
		&tool_definitions,
		&tool_ctx,
		&mut thread,
		thread_store.as_ref(),
		shutdown_rx,
		&workspace,
		auto_commit_service.as_ref(),
	)
	.await
}

fn snapshot_git_state(thread: &mut Thread, workspace_path: &std::path::Path) {
	match detect_repo_status(workspace_path) {
		Ok(Some(status)) => {
			if thread.git_remote_url.is_none() {
				thread.git_remote_url = status.remote_slug.clone();
			}

			if thread.git_initial_branch.is_none() {
				thread.git_initial_branch = status.branch.clone();
			}

			thread.git_branch = status.branch;

			if let Some(ref head) = status.head {
				let sha = head.sha.clone();

				if thread.git_initial_commit_sha.is_none() {
					thread.git_initial_commit_sha = Some(sha.clone());
				}

				thread.git_current_commit_sha = Some(sha.clone());

				if !thread.git_commits.contains(&sha) {
					thread.git_commits.push(sha);
				}
			}

			if thread.git_start_dirty.is_none() {
				thread.git_start_dirty = status.is_dirty;
			}
			thread.git_end_dirty = status.is_dirty;

			debug!(
					git_branch = ?thread.git_branch,
					git_remote_url = ?thread.git_remote_url,
					git_current_commit_sha = ?thread.git_current_commit_sha,
					git_is_dirty = ?thread.git_end_dirty,
					"snapshot git state"
			);
		}
		Ok(None) => {
			debug!("not a git repository or git unavailable");
		}
		Err(e) => {
			debug!(error = %e, "failed to detect git repository");
		}
	}
}

async fn run_search(
	query: &str,
	limit: usize,
	json_output: bool,
	_thread_store: &dyn ThreadStore,
) -> Result<()> {
	let query = query.trim();
	if query.is_empty() {
		anyhow::bail!("Search query cannot be empty");
	}

	// Try server search first if sync is enabled
	if let Ok(sync_url) = std::env::var("LOOM_THREAD_SYNC_URL") {
		match search_server(&sync_url, query, limit).await {
			Ok(results) => {
				if json_output {
					println!("{}", serde_json::to_string_pretty(&results)?);
				} else {
					print_search_results(&results, query);
				}
				return Ok(());
			}
			Err(e) => {
				debug!(error = %e, "Server search failed, falling back to local");
			}
		}
	}

	// Fall back to local search
	let local_store = LocalThreadStore::from_xdg()?;
	let results = local_store.search(query, limit).await?;

	if json_output {
		println!("{}", serde_json::to_string_pretty(&results)?);
	} else {
		print_local_search_results(&results, query);
	}

	Ok(())
}

async fn search_server(
	base_url: &str,
	query: &str,
	limit: usize,
) -> Result<Vec<serde_json::Value>> {
	let client = loom_common_http::new_client();
	let url = format!("{}/api/threads/search", base_url.trim_end_matches('/'));

	let response = client
		.get(&url)
		.query(&[("q", query), ("limit", &limit.to_string())])
		.timeout(std::time::Duration::from_secs(10))
		.send()
		.await?;

	if !response.status().is_success() {
		anyhow::bail!("Server returned {}", response.status());
	}

	let body: serde_json::Value = response.json().await?;
	let hits = body
		.get("hits")
		.and_then(|h| h.as_array())
		.cloned()
		.unwrap_or_default();

	Ok(hits)
}

fn print_search_results(results: &[serde_json::Value], query: &str) {
	if results.is_empty() {
		println!(
			"{}",
			loom_common_i18n::t_fmt(
				get_locale(),
				"client.search.no_results",
				&[("query", query)]
			)
		);
		return;
	}

	println!(
		"{}\n",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.search.results_header",
			&[("query", query), ("count", &results.len().to_string())]
		)
	);

	for (i, hit) in results.iter().enumerate() {
		let summary = hit.get("summary").unwrap_or(hit);
		let id = summary.get("id").and_then(|v| v.as_str()).unwrap_or("?");
		let title = summary
			.get("title")
			.and_then(|v| v.as_str())
			.unwrap_or("(untitled)");
		let branch = summary
			.get("git_branch")
			.and_then(|v| v.as_str())
			.unwrap_or("-");
		let remote = summary
			.get("git_remote_url")
			.and_then(|v| v.as_str())
			.unwrap_or("-");
		let score = hit.get("score").and_then(|v| v.as_f64());

		println!("{}) {}", i + 1, id);
		if remote != "-" {
			println!("   [{remote}] {branch}");
		}
		println!("   \"{title}\"");
		if let Some(s) = score {
			if s != 0.0 {
				println!("   score: {s:.3}");
			}
		}
		println!();
	}
}

fn print_local_search_results(results: &[loom_common_thread::ThreadSummary], query: &str) {
	if results.is_empty() {
		println!(
			"{}",
			loom_common_i18n::t_fmt(
				get_locale(),
				"client.search.local_no_results",
				&[("query", query)]
			)
		);
		return;
	}

	println!(
		"{}\n",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.search.local_results_header",
			&[("query", query), ("count", &results.len().to_string())]
		)
	);

	for (i, summary) in results.iter().enumerate() {
		let title = summary.title.as_deref().unwrap_or("(untitled)");
		let branch = summary.git_branch.as_deref().unwrap_or("-");
		let remote = summary.git_remote_url.as_deref().unwrap_or("-");

		println!("{}) {}", i + 1, summary.id);
		if remote != "-" {
			println!("   [{remote}] {branch}");
		}
		println!("   \"{title}\"");
		println!();
	}
}

fn create_new_thread(config: &loom_cli_config::LoomConfig, args: &Args) -> Result<Thread> {
	let workspace = config
		.global
		.workspace_root
		.clone()
		.or_else(|| config.tools.workspace.root.clone())
		.unwrap_or_else(|| PathBuf::from("."))
		.canonicalize()
		.context("invalid workspace path")?;

	let mut thread = Thread::new();
	thread.workspace_root = Some(workspace.display().to_string());
	thread.cwd = Some(std::env::current_dir()?.display().to_string());
	thread.loom_version = Some(loom_common_version::loom_version().to_string());
	thread.provider = Some(args.provider.clone());
	thread.model = None;

	snapshot_git_state(&mut thread, &workspace);

	info!(thread_id = %thread.id, provider = %args.provider, "created new thread");
	Ok(thread)
}

#[tokio::main]
async fn main() -> Result<()> {
	let args = Args::parse();

	if let Some(Command::CredentialHelper(cred_args)) = &args.command {
		return credential_helper::run(cred_args.clone()).await;
	}

	// Fast path for attach command - skip full initialization to avoid tracing
	// interfering with terminal I/O
	if let Some(Command::Attach { weaver_id }) = &args.command {
		let token = auth::load_token(&args.server_url).await;
		return run_weaver_attach(&args.server_url, token, weaver_id).await;
	}
	if let Some(Command::Weaver {
		command: WeaverCommand::Attach { weaver_id },
	}) = &args.command
	{
		let token = auth::load_token(&args.server_url).await;
		return run_weaver_attach(&args.server_url, token, weaver_id).await;
	}

	let cli_overrides = CliOverrides::from(&args);
	let config = load_config_with_cli(cli_overrides).context("failed to load configuration")?;

	init_tracing(&config.logging);

	info!(
			provider = %config.global.default_provider,
			"starting loom"
	);

	// Get auth token for thread sync
	let auth_token = auth::load_token(&args.server_url).await;

	let thread_store: Arc<dyn ThreadStore> = {
		let local_store =
			LocalThreadStore::from_xdg().context("failed to create local thread store")?;

		// Use server_url for thread sync (append /api/ if needed)
		let sync_url = format!("{}/api/", args.server_url.trim_end_matches('/'));
		let base_url = Url::parse(&sync_url).context("invalid server URL for thread sync")?;
		let http_client = loom_common_http::new_client();

		let mut sync_client = ThreadSyncClient::new(base_url, http_client);
		if let Some(token) = auth_token.clone() {
			sync_client = sync_client.with_auth_token(token);
		}
		Arc::new(SyncingThreadStore::with_sync(local_store, sync_client))
	};

	match &args.command {
		Some(Command::Version) => {
			println!("{}", version::format_version_info());
			Ok(())
		}
		Some(Command::Update) => update::run_update().await,
		Some(Command::Login) => auth::login(&args.server_url).await,
		Some(Command::Logout) => auth::logout(&args.server_url).await,
		Some(Command::List) => {
			let threads = thread_store
				.list(100)
				.await
				.context("failed to list threads")?;

			if threads.is_empty() {
				println!(
					"{}",
					loom_common_i18n::t(get_locale(), "client.threads.no_threads")
				);
			} else {
				println!(
					"{:<42} {:<30} {:>6} {:<20}",
					"ID", "TITLE", "MSGS", "LAST ACTIVITY"
				);
				println!("{}", "-".repeat(100));
				for summary in threads {
					let title = summary.title.as_deref().unwrap_or("(untitled)");
					let title_display = if title.len() > 28 {
						format!("{}...", &title[..25])
					} else {
						title.to_string()
					};
					println!(
						"{:<42} {:<30} {:>6} {:<20}",
						summary.id, title_display, summary.message_count, summary.last_activity_at
					);
				}
			}
			Ok(())
		}
		Some(Command::Resume { thread_id }) => {
			let thread = match thread_id {
				Some(id) => {
					let tid = ThreadId::from_string(id.clone());
					thread_store
						.load(&tid)
						.await
						.context("failed to load thread")?
						.with_context(|| format!("thread '{id}' not found"))?
				}
				None => {
					let threads = thread_store
						.list(1)
						.await
						.context("failed to list threads")?;
					let summary = threads
						.into_iter()
						.next()
						.context("no threads found to resume")?;
					thread_store
						.load(&summary.id)
						.await
						.context("failed to load thread")?
						.context("thread not found")?
				}
			};
			info!(thread_id = %thread.id, "resuming thread");
			start_repl_session(&config, &args, thread_store, thread).await
		}
		Some(Command::Private) => {
			let mut thread = create_new_thread(&config, &args)?;
			thread.is_private = true;
			thread.visibility = ThreadVisibility::Private;
			info!(thread_id = %thread.id, "created new private (local-only) thread");
			println!(
				"{}",
				loom_common_i18n::t(get_locale(), "client.threads.private_session")
			);
			println!(
				"{}",
				loom_common_i18n::t_fmt(
					get_locale(),
					"client.repl.thread_id",
					&[("id", &thread.id.to_string())]
				)
			);
			start_repl_session(&config, &args, thread_store, thread).await
		}
		Some(Command::Search { query, limit, json }) => {
			run_search(query, *limit, *json, thread_store.as_ref()).await
		}
		Some(Command::Share {
			thread_id,
			visibility,
			support,
		}) => {
			let thread = match thread_id {
				Some(id) => {
					let tid = ThreadId::from_string(id.clone());
					thread_store
						.load(&tid)
						.await
						.context("failed to load thread")?
						.with_context(|| format!("thread '{id}' not found"))?
				}
				None => {
					let threads = thread_store
						.list(1)
						.await
						.context("failed to list threads")?;
					let summary = threads
						.into_iter()
						.next()
						.context("no threads found to share")?;
					thread_store
						.load(&summary.id)
						.await
						.context("failed to load thread")?
						.context("thread not found")?
				}
			};

			if thread.is_private {
				anyhow::bail!(
					"Thread {} is a local-only private session and cannot be shared. \
                     Start a normal session if you want to sync to the server.",
					thread.id
				);
			}

			let mut updated = thread.clone();

			if *support {
				updated.is_shared_with_support = true;
				updated.touch();

				info!(
						thread_id = %updated.id,
						"sharing thread with support"
				);

				thread_store
					.save_and_sync(&updated)
					.await
					.context("failed to save and sync thread")?;

				println!(
					"{}",
					loom_common_i18n::t_fmt(
						get_locale(),
						"client.threads.shared_support",
						&[("id", &updated.id.to_string())]
					)
				);
			} else if let Some(v) = visibility {
				updated.visibility = ThreadVisibility::from(v.clone());
				updated.touch();

				info!(
						thread_id = %updated.id,
						visibility = ?updated.visibility,
						"updating thread visibility"
				);

				thread_store
					.save_and_sync(&updated)
					.await
					.context("failed to save and sync thread with updated visibility")?;

				println!(
					"{}",
					loom_common_i18n::t_fmt(
						get_locale(),
						"client.threads.visibility_changed",
						&[
							("id", &updated.id.to_string()),
							("visibility", &format!("{:?}", updated.visibility)),
						]
					)
				);
			} else {
				anyhow::bail!("Either --visibility or --support must be specified");
			}

			Ok(())
		}
		Some(Command::AcpAgent) => run_acp_agent(&config, &args, thread_store).await,
		Some(Command::CredentialHelper(_)) => unreachable!("handled early in main"),
		Some(Command::New {
			image,
			org,
			repo,
			branch,
			env,
			ttl,
		}) => {
			let token = auth::load_token(&args.server_url).await;
			run_weaver_new(
				&args.server_url,
				token,
				image.clone(),
				org.clone(),
				repo.clone(),
				branch.clone(),
				env.clone(),
				*ttl,
			)
			.await
		}
		Some(Command::Attach { weaver_id }) => {
			let token = auth::load_token(&args.server_url).await;
			run_weaver_attach(&args.server_url, token, weaver_id).await
		}
		Some(Command::Weaver { command }) => match command {
			WeaverCommand::New {
				image,
				org,
				repo,
				branch,
				env,
				ttl,
			} => {
				let token = auth::load_token(&args.server_url).await;
				run_weaver_new(
					&args.server_url,
					token,
					image.clone(),
					org.clone(),
					repo.clone(),
					branch.clone(),
					env.clone(),
					*ttl,
				)
				.await
			}
			WeaverCommand::Attach { weaver_id } => {
				let token = auth::load_token(&args.server_url).await;
				run_weaver_attach(&args.server_url, token, weaver_id).await
			}
			WeaverCommand::Ps { json } => {
				let token = auth::load_token(&args.server_url).await;
				run_weaver_ps(&args.server_url, token, *json).await
			}
			WeaverCommand::Delete { weaver_id } => {
				let token = auth::load_token(&args.server_url).await;
				run_weaver_delete(&args.server_url, token, weaver_id).await
			}
		},
		Some(Command::Spool { command }) => loom_cli_spool::run(command).await,
		Some(Command::Tunnel { command }) => {
			let ctx = create_wgtunnel_context(&args).await?;
			match command {
				loom_cli_wgtunnel::TunnelCommands::Up(ref up_args) => {
					loom_cli_wgtunnel::handle_tunnel_up(up_args.clone(), &ctx).await
				}
				loom_cli_wgtunnel::TunnelCommands::Down => {
					loom_cli_wgtunnel::handle_tunnel_down(&ctx).await
				}
				loom_cli_wgtunnel::TunnelCommands::Status => {
					loom_cli_wgtunnel::handle_tunnel_status(&ctx).await
				}
			}
		}
		Some(Command::Ssh(ref ssh_args)) => {
			let ctx = create_wgtunnel_context(&args).await?;
			loom_cli_wgtunnel::handle_ssh(ssh_args.clone(), &ctx).await
		}
		Some(Command::Wg { command }) => {
			let ctx = create_wgtunnel_context(&args).await?;
			match command {
				WgCommand::Devices { command: dev_cmd } => match dev_cmd {
					loom_cli_wgtunnel::DevicesCommands::List => {
						loom_cli_wgtunnel::handle_devices_list(&ctx).await
					}
					loom_cli_wgtunnel::DevicesCommands::Register(ref register_args) => {
						loom_cli_wgtunnel::handle_devices_register(register_args.clone(), &ctx).await
					}
					loom_cli_wgtunnel::DevicesCommands::Revoke(ref revoke_args) => {
						loom_cli_wgtunnel::handle_devices_revoke(revoke_args.clone(), &ctx).await
					}
				},
			}
		}
		None => {
			let thread = create_new_thread(&config, &args)?;
			start_repl_session(&config, &args, thread_store, thread).await
		}
	}
}

async fn run_acp_agent(
	config: &loom_cli_config::LoomConfig,
	args: &Args,
	thread_store: Arc<dyn ThreadStore>,
) -> Result<()> {
	use agent_client_protocol::{self as acp, Client as _};
	use loom_cli_acp::{LoomAcpAgent, SessionNotificationRequest};
	use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

	info!("starting ACP agent mode");

	let workspace = config
		.global
		.workspace_root
		.clone()
		.or_else(|| config.tools.workspace.root.clone())
		.unwrap_or_else(|| PathBuf::from("."))
		.canonicalize()
		.context("invalid workspace path")?;

	let token = auth::load_token(&args.server_url).await;
	let llm_client = create_llm_client(&args.server_url, &args.provider, token)?;
	let tools = Arc::new(create_tool_registry());

	let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SessionNotificationRequest>();

	let agent = LoomAcpAgent::new(
		llm_client,
		tools,
		thread_store,
		workspace,
		args.provider.clone(),
		tx,
	);

	let stdin = tokio::io::stdin().compat();
	let stdout = tokio::io::stdout().compat_write();

	let local_set = tokio::task::LocalSet::new();
	local_set
		.run_until(async move {
			let (conn, io_task) = acp::AgentSideConnection::new(agent, stdout, stdin, |fut| {
				tokio::task::spawn_local(fut);
			});

			// Background task: forward session notifications to client
			tokio::task::spawn_local(async move {
				while let Some(req) = rx.recv().await {
					if let Err(e) = conn.session_notification(req.notification).await {
						error!(error = %e, "failed to send session notification");
						break;
					}
					req.completion_tx.send(()).ok();
				}
			});

			// Run until stdio closes
			if let Err(e) = io_task.await {
				error!(error = %e, "ACP I/O error");
			}

			Ok(())
		})
		.await
}

fn setup_ctrlc_handler(shutdown_tx: watch::Sender<bool>) -> Result<()> {
	ctrlc::set_handler(move || {
		info!("received Ctrl+C, requesting shutdown");
		let _ = shutdown_tx.send(true);
		eprintln!();
	})
	.context("failed to set Ctrl+C handler")?;

	Ok(())
}

async fn create_wgtunnel_context(args: &Args) -> Result<loom_cli_wgtunnel::CliContext> {
	let server_url: Url = args.server_url.parse().context("invalid server URL")?;

	let token = auth::load_token(&args.server_url)
		.await
		.ok_or_else(|| anyhow::anyhow!("not logged in, run 'loom login' first"))?;

	let config_dir = dirs::config_dir()
		.unwrap_or_else(|| PathBuf::from("."))
		.join("loom")
		.join("wgtunnel");

	tokio::fs::create_dir_all(&config_dir)
		.await
		.context("failed to create wgtunnel config directory")?;

	Ok(loom_cli_wgtunnel::CliContext::new(
		server_url, token, config_dir,
	))
}

#[allow(clippy::too_many_arguments)]
async fn run_weaver_new(
	server_url: &str,
	token: Option<loom_common_secret::SecretString>,
	image: Option<String>,
	org_id: Option<String>,
	repo: Option<String>,
	branch: Option<String>,
	env: Vec<String>,
	ttl: Option<u32>,
) -> Result<()> {
	let mut client = weaver_client::WeaverClient::new(server_url)?;
	if let Some(token) = token {
		client = client.with_token(token);
	}

	let org_id = match org_id {
		Some(org_ref) => client.resolve_org_id(&org_ref).await?,
		None => {
			let personal_org = client.get_personal_org().await?;
			personal_org.id
		}
	};

	let mut env_map = std::collections::HashMap::new();
	for e in env {
		if let Some((k, v)) = e.split_once('=') {
			env_map.insert(k.to_string(), v.to_string());
		}
	}

	let request = weaver_client::CreateWeaverRequest {
		image: image.unwrap_or_else(|| "ghcr.io/ghuntley/loom/weaver:latest".to_string()),
		org_id,
		env: env_map,
		repo,
		branch,
		lifetime_hours: ttl,
	};

	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.weaver.creating")
	);
	let weaver = client.create_weaver(&request).await?;

	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.created_id",
			&[("id", &weaver.id)]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.created_image",
			&[("image", &weaver.image.clone().unwrap_or_default())]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.created_ttl",
			&[("hours", &weaver.lifetime_hours.unwrap_or(4).to_string())]
		)
	);
	println!();

	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.weaver.attaching")
	);
	client.attach_terminal(&weaver.id).await?;

	println!(
		"\n{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.detached",
			&[("id", &weaver.id)]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.reattach_hint",
			&[("id", &weaver.id)]
		)
	);

	Ok(())
}

async fn run_weaver_ps(
	server_url: &str,
	token: Option<loom_common_secret::SecretString>,
	json: bool,
) -> Result<()> {
	let mut client = weaver_client::WeaverClient::new(server_url)?;
	if let Some(token) = token {
		client = client.with_token(token);
	}
	let list = client.list_weavers().await?;

	if json {
		println!("{}", serde_json::to_string_pretty(&list.weavers)?);
	} else if list.weavers.is_empty() {
		println!(
			"{}",
			loom_common_i18n::t(get_locale(), "client.weaver.no_weavers")
		);
	} else {
		println!(
			"{:<40} {:<30} {:<10} {:<8} {:<8}",
			"ID", "IMAGE", "STATUS", "AGE", "TTL"
		);
		println!("{}", "-".repeat(96));
		for w in &list.weavers {
			let image = w.image.as_deref().unwrap_or("-");
			let image_display = if image.len() > 28 {
				format!("{}...", &image[..25])
			} else {
				image.to_string()
			};
			let age = w
				.age_hours
				.map(|h| format!("{h:.1}h"))
				.unwrap_or_else(|| "-".to_string());
			let ttl = w
				.lifetime_hours
				.map(|h| format!("{h}h"))
				.unwrap_or_else(|| "-".to_string());
			println!(
				"{:<40} {:<30} {:<10} {:<8} {:<8}",
				w.id, image_display, w.status, age, ttl
			);
		}
	}

	Ok(())
}

async fn run_weaver_delete(
	server_url: &str,
	token: Option<loom_common_secret::SecretString>,
	weaver_id: &str,
) -> Result<()> {
	let mut client = weaver_client::WeaverClient::new(server_url)?;
	if let Some(token) = token {
		client = client.with_token(token);
	}

	println!(
		"{}",
		loom_common_i18n::t_fmt(get_locale(), "client.weaver.deleting", &[("id", weaver_id)])
	);
	client.delete_weaver(weaver_id).await?;
	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.weaver.deleted")
	);

	Ok(())
}

async fn run_weaver_attach(
	server_url: &str,
	token: Option<loom_common_secret::SecretString>,
	weaver_id: &str,
) -> Result<()> {
	let mut client = weaver_client::WeaverClient::new(server_url)?;
	if let Some(token) = token {
		client = client.with_token(token);
	}

	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.attach_prefix",
			&[("id", weaver_id)]
		)
	);
	client.attach_terminal(weaver_id).await?;

	println!(
		"\n{}",
		loom_common_i18n::t_fmt(get_locale(), "client.weaver.detached", &[("id", weaver_id)])
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.weaver.reattach_hint",
			&[("id", weaver_id)]
		)
	);

	Ok(())
}
// test change
