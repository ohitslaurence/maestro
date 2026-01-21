// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Git HTTP Smart Protocol endpoints for clone/fetch/push operations.
//!
//! Implements the Git smart HTTP protocol as specified in:
//! https://git-scm.com/docs/http-protocol
//!
//! # Endpoints
//!
//! - `GET /git/{owner}/{repo}.git/info/refs` - Advertise refs for clone/fetch/push
//! - `POST /git/{owner}/{repo}.git/git-upload-pack` - Serve clone/fetch
//! - `POST /git/{owner}/{repo}.git/git-receive-pack` - Receive push
//!
//! # Authentication
//!
//! - Public repos: anonymous clone allowed
//! - Private repos: authentication required for all operations
//! - Push: always requires authentication

use axum::{
	body::Bytes,
	extract::{Path, Query, State},
	http::{header, HeaderMap, StatusCode},
	response::{IntoResponse, Response},
	routing::{get, post},
};
use base64::Engine;
use loom_server_auth::middleware::{identify_bearer_token, BearerTokenType, CurrentUser};
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::{
	check_push_allowed, OwnerType, ProtectionStore, PushCheck, RepoRole, RepoStore,
	RepoTeamAccessStore, Repository, Visibility,
};
use loom_server_scm_mirror::{CreateExternalMirror, ExternalMirrorStore, Platform};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Stdio};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{info, instrument, warn};

use crate::{api::AppState, auth_middleware::OptionalAuth, error::ServerError, i18n::t};

fn git_unauthorized_response(message: &str) -> Response {
	(
		StatusCode::UNAUTHORIZED,
		[(header::WWW_AUTHENTICATE, "Basic realm=\"git\"")],
		axum::Json(serde_json::json!({
			"error": "unauthorized",
			"message": message
		})),
	)
		.into_response()
}

async fn update_mirror_access_time(state: &AppState, repo_id: uuid::Uuid) {
	if let Some(store) = &state.external_mirror_store {
		if let Ok(Some(mirror)) = store.get_by_repo_id(repo_id).await {
			if let Err(e) = loom_server_scm_mirror::touch_mirror(store.as_ref(), mirror.id).await {
				tracing::warn!(
					mirror_id = %mirror.id,
					error = %e,
					"Failed to update external mirror access time"
				);
			}
		}
	}
}

async fn extract_basic_auth_user(headers: &HeaderMap, state: &AppState) -> Option<CurrentUser> {
	let auth_header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;

	if !auth_header.starts_with("Basic ") {
		return None;
	}

	let encoded = auth_header.strip_prefix("Basic ")?;
	let decoded = base64::engine::general_purpose::STANDARD
		.decode(encoded)
		.ok()?;
	let credentials = String::from_utf8(decoded).ok()?;

	let (_, password) = credentials.split_once(':')?;

	match identify_bearer_token(password) {
		BearerTokenType::AccessToken => {
			let mut hasher = Sha256::new();
			hasher.update(password.as_bytes());
			let token_hash = hex::encode(hasher.finalize());

			let (_token_id, user_id) = state
				.session_repo
				.get_access_token_by_hash(&token_hash)
				.await
				.ok()??;

			let user = state.user_repo.get_user_by_id(&user_id).await.ok()??;
			Some(CurrentUser::from_access_token(user))
		}
		BearerTokenType::ApiKey => {
			let mut hasher = Sha256::new();
			hasher.update(password.as_bytes());
			let key_hash = hex::encode(hasher.finalize());

			let api_key = state
				.api_key_repo
				.get_api_key_by_hash(&key_hash)
				.await
				.ok()??;

			if api_key.revoked_at.is_some() {
				return None;
			}

			let user = state
				.user_repo
				.get_user_by_id(&api_key.created_by)
				.await
				.ok()??;
			Some(CurrentUser::from_api_key(user, api_key.id.into()))
		}
		BearerTokenType::Unknown => None,
		BearerTokenType::WsToken => None,
	}
}

#[derive(Debug, Deserialize)]
pub struct InfoRefsParams {
	service: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitService {
	UploadPack,
	ReceivePack,
}

impl GitService {
	fn from_str(s: &str) -> Option<Self> {
		match s {
			"git-upload-pack" => Some(GitService::UploadPack),
			"git-receive-pack" => Some(GitService::ReceivePack),
			_ => None,
		}
	}

	fn as_str(&self) -> &'static str {
		match self {
			GitService::UploadPack => "git-upload-pack",
			GitService::ReceivePack => "git-receive-pack",
		}
	}

	fn as_git_subcommand(&self) -> &'static str {
		match self {
			GitService::UploadPack => "upload-pack",
			GitService::ReceivePack => "receive-pack",
		}
	}

	fn content_type(&self) -> &'static str {
		match self {
			GitService::UploadPack => "application/x-git-upload-pack-advertisement",
			GitService::ReceivePack => "application/x-git-receive-pack-advertisement",
		}
	}

	fn result_content_type(&self) -> &'static str {
		match self {
			GitService::UploadPack => "application/x-git-upload-pack-result",
			GitService::ReceivePack => "application/x-git-receive-pack-result",
		}
	}
}

fn get_repos_base_dir() -> PathBuf {
	std::env::var("LOOM_SERVER_DATA_DIR")
		.map(PathBuf::from)
		.unwrap_or_else(|_| PathBuf::from("/var/lib/loom"))
		.join("repos")
}

fn get_repo_path(repo: &Repository) -> PathBuf {
	let id_str = repo.id.to_string();
	let shard = &id_str[..2];
	get_repos_base_dir().join(shard).join(&id_str).join("git")
}

pub fn get_repo_path_by_id(repo_id: uuid::Uuid) -> PathBuf {
	let id_str = repo_id.to_string();
	let shard = &id_str[..2];
	get_repos_base_dir().join(shard).join(&id_str).join("git")
}

#[derive(Debug, Clone)]
struct MirrorInfo {
	platform: Platform,
	external_owner: String,
	external_repo: String,
}

fn parse_mirror_path(owner: &str, repo_name: &str) -> Option<MirrorInfo> {
	let repo_name = repo_name.strip_suffix(".git").unwrap_or(repo_name);

	let parts: Vec<&str> = owner.split('/').collect();
	if parts.len() != 3 {
		return None;
	}

	if parts[0] != "mirrors" {
		return None;
	}

	let platform = Platform::parse(parts[1])?;
	let external_owner = parts[2].to_string();
	let external_repo = repo_name.to_string();

	Some(MirrorInfo {
		platform,
		external_owner,
		external_repo,
	})
}

fn is_mirror_path(owner: &str) -> bool {
	owner.starts_with("mirrors/")
}

pub async fn resolve_repo(
	owner: &str,
	repo_name: &str,
	state: &AppState,
	locale: &str,
) -> Result<Repository, ServerError> {
	let repo_name = repo_name.strip_suffix(".git").unwrap_or(repo_name);

	let scm_store = state
		.scm_repo_store
		.as_ref()
		.ok_or_else(|| ServerError::Internal(t(locale, "server.api.scm.not_configured").to_string()))?;

	// Try parsing owner as UUID (owner_id) first
	if let Ok(owner_id) = uuid::Uuid::parse_str(owner) {
		// Try as user owner
		if let Some(scm_repo) = scm_store
			.get_by_owner_and_name(loom_server_scm::OwnerType::User, owner_id, repo_name)
			.await
			.map_err(|e| ServerError::Internal(e.to_string()))?
		{
			return Ok(scm_repo);
		}
		// Try as org owner
		if let Some(scm_repo) = scm_store
			.get_by_owner_and_name(loom_server_scm::OwnerType::Org, owner_id, repo_name)
			.await
			.map_err(|e| ServerError::Internal(e.to_string()))?
		{
			return Ok(scm_repo);
		}
	}

	// Try looking up by org slug
	if let Some(org) = state.org_repo.get_org_by_slug(owner).await? {
		if let Some(scm_repo) = scm_store
			.get_by_owner_and_name(loom_server_scm::OwnerType::Org, org.id.into(), repo_name)
			.await
			.map_err(|e| ServerError::Internal(e.to_string()))?
		{
			return Ok(scm_repo);
		}
	}

	// Try looking up by username
	if let Ok(Some(user)) = state.user_repo.get_user_by_username(owner).await {
		if let Some(scm_repo) = scm_store
			.get_by_owner_and_name(
				loom_server_scm::OwnerType::User,
				user.id.into_inner(),
				repo_name,
			)
			.await
			.map_err(|e| ServerError::Internal(e.to_string()))?
		{
			return Ok(scm_repo);
		}
	}

	Err(ServerError::NotFound(
		t(locale, "server.api.scm.repo_not_found").to_string(),
	))
}

fn higher_role(a: Option<RepoRole>, b: Option<RepoRole>) -> Option<RepoRole> {
	match (a, b) {
		(Some(x), Some(y)) => {
			if x.has_permission_of(&y) {
				Some(x)
			} else {
				Some(y)
			}
		}
		(Some(x), None) => Some(x),
		(None, Some(y)) => Some(y),
		(None, None) => None,
	}
}

async fn get_direct_role(
	user: &CurrentUser,
	repo: &Repository,
	state: &AppState,
) -> Option<RepoRole> {
	match repo.owner_type {
		OwnerType::User => {
			if repo.owner_id == user.user.id.into_inner() {
				Some(RepoRole::Admin)
			} else {
				None
			}
		}
		OwnerType::Org => {
			let org_id = OrgId::new(repo.owner_id);
			match state.org_repo.get_membership(&org_id, &user.user.id).await {
				Ok(Some(m)) => {
					if m.role == OrgRole::Owner || m.role == OrgRole::Admin {
						Some(RepoRole::Admin)
					} else {
						Some(RepoRole::Read)
					}
				}
				_ => None,
			}
		}
	}
}

pub async fn get_user_repo_role(
	user: &CurrentUser,
	repo: &Repository,
	state: &AppState,
) -> Option<RepoRole> {
	let direct_role = get_direct_role(user, repo, state).await;

	let team_role = match &state.scm_team_access_store {
		Some(store) => store
			.get_user_role_via_teams(user.user.id.into_inner(), repo.id)
			.await
			.ok()
			.flatten(),
		None => None,
	};

	higher_role(direct_role, team_role)
}

pub async fn check_read_access(
	repo: &Repository,
	user: Option<&CurrentUser>,
	state: &AppState,
	locale: &str,
) -> Result<(), ServerError> {
	if repo.visibility == Visibility::Public {
		return Ok(());
	}

	let user = user.ok_or_else(|| {
		ServerError::Unauthorized(t(locale, "server.api.scm.git.auth_required_private").to_string())
	})?;

	let role = get_user_repo_role(user, repo, state).await;

	if role
		.map(|r| r.has_permission_of(&RepoRole::Read))
		.unwrap_or(false)
	{
		Ok(())
	} else {
		Err(ServerError::Forbidden(
			t(locale, "server.api.scm.git.access_denied").to_string(),
		))
	}
}

async fn check_write_access(
	repo: &Repository,
	user: Option<&CurrentUser>,
	state: &AppState,
	locale: &str,
) -> Result<(), ServerError> {
	let user = user.ok_or_else(|| {
		ServerError::Unauthorized(t(locale, "server.api.scm.git.auth_required_push").to_string())
	})?;

	let role = get_user_repo_role(user, repo, state).await;

	if role
		.map(|r| r.has_permission_of(&RepoRole::Write))
		.unwrap_or(false)
	{
		Ok(())
	} else {
		Err(ServerError::Forbidden(
			t(locale, "server.api.scm.git.write_access_denied").to_string(),
		))
	}
}

const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

#[derive(Debug)]
struct PushCommand {
	old_sha: String,
	new_sha: String,
	ref_name: String,
}

fn parse_push_commands(body: &[u8]) -> Vec<PushCommand> {
	let mut commands = Vec::new();
	let mut pos = 0;

	while pos + 4 <= body.len() {
		let len_str = std::str::from_utf8(&body[pos..pos + 4]).unwrap_or("0000");
		let len = usize::from_str_radix(len_str, 16).unwrap_or(0);

		if len == 0 {
			break;
		}

		if pos + len > body.len() {
			break;
		}

		let line_end = pos + len;
		let line = &body[pos + 4..line_end];
		pos = line_end;

		if let Ok(line_str) = std::str::from_utf8(line) {
			let line_str = line_str.trim_end_matches('\n');
			let parts: Vec<&str> = line_str.split(' ').collect();
			if parts.len() >= 3 {
				let old_sha = parts[0].to_string();
				let new_sha = parts[1].to_string();
				let ref_with_caps = parts[2..].join(" ");
				let ref_name = ref_with_caps
					.split('\0')
					.next()
					.unwrap_or(&ref_with_caps)
					.to_string();

				commands.push(PushCommand {
					old_sha,
					new_sha,
					ref_name,
				});
			}
		}
	}

	commands
}

fn extract_branch_name(ref_name: &str) -> Option<&str> {
	ref_name.strip_prefix("refs/heads/")
}

async fn check_user_is_repo_admin(repo: &Repository, user: &CurrentUser, state: &AppState) -> bool {
	let role = get_user_repo_role(user, repo, state).await;
	role.map(|r| r == RepoRole::Admin).unwrap_or(false)
}

async fn is_force_push(repo_path: &std::path::Path, old_sha: &str, new_sha: &str) -> bool {
	if old_sha == ZERO_SHA || new_sha == ZERO_SHA {
		return false;
	}

	let path = repo_path.to_path_buf();
	let old = old_sha.to_string();
	let new = new_sha.to_string();

	let result = tokio::task::spawn_blocking(move || {
		let repo = match gix::open(&path) {
			Ok(r) => r,
			Err(_) => return false,
		};

		let old_oid = match gix::ObjectId::from_hex(old.as_bytes()) {
			Ok(oid) => oid,
			Err(_) => return false,
		};

		let new_oid = match gix::ObjectId::from_hex(new.as_bytes()) {
			Ok(oid) => oid,
			Err(_) => return false,
		};

		match repo.merge_base(old_oid, new_oid) {
			Ok(base) => base.detach() != old_oid,
			Err(_) => true,
		}
	})
	.await;

	result.unwrap_or(false)
}

fn pkt_line(data: &str) -> Vec<u8> {
	let len = data.len() + 4;
	format!("{len:04x}{data}").into_bytes()
}

fn pkt_flush() -> Vec<u8> {
	b"0000".to_vec()
}

// NOTE: run_git_command uses git subprocess because gitoxide doesn't yet support
// server-side git protocol (upload-pack/receive-pack). The client-side protocol is
// supported but serving git HTTP requires additional server-side implementation.
// See: https://github.com/GitoxideLabs/gitoxide/discussions/362
// Track progress at: https://github.com/GitoxideLabs/gitoxide/issues/307
async fn run_git_command(
	repo_path: &std::path::Path,
	service: GitService,
	input: &[u8],
	advertise: bool,
) -> Result<Vec<u8>, ServerError> {
	let mut cmd = Command::new("git");
	cmd.arg(service.as_git_subcommand());

	if advertise {
		cmd.arg("--advertise-refs");
	}

	cmd.arg("--stateless-rpc");
	cmd.arg(repo_path);
	cmd.stdin(Stdio::piped());
	cmd.stdout(Stdio::piped());
	cmd.stderr(Stdio::piped());

	let mut child = cmd
		.spawn()
		.map_err(|e| ServerError::Internal(format!("Failed to spawn git: {e}")))?;

	if let Some(mut stdin) = child.stdin.take() {
		stdin
			.write_all(input)
			.await
			.map_err(|e| ServerError::Internal(format!("Failed to write to git stdin: {e}")))?;
	}

	let output = child
		.wait_with_output()
		.await
		.map_err(|e| ServerError::Internal(format!("Failed to wait for git: {e}")))?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		tracing::error!(stderr = %stderr, "git command failed");
		return Err(ServerError::Internal(format!(
			"Git command failed: {stderr}"
		)));
	}

	Ok(output.stdout)
}

async fn create_on_demand_mirror(
	mirror_info: &MirrorInfo,
	state: &AppState,
	locale: &str,
) -> Result<Repository, ServerError> {
	let external_mirror_store = state
		.external_mirror_store
		.as_ref()
		.ok_or_else(|| ServerError::Internal(t(locale, "server.api.scm.not_configured").to_string()))?;

	let scm_store = state
		.scm_repo_store
		.as_ref()
		.ok_or_else(|| ServerError::Internal(t(locale, "server.api.scm.not_configured").to_string()))?;

	if let Ok(Some(existing)) = external_mirror_store
		.get_by_external(
			mirror_info.platform,
			&mirror_info.external_owner,
			&mirror_info.external_repo,
		)
		.await
	{
		if let Ok(Some(repo)) = scm_store.get_by_id(existing.repo_id).await {
			return Ok(repo);
		}
	}

	info!(
		platform = ?mirror_info.platform,
		owner = %mirror_info.external_owner,
		repo = %mirror_info.external_repo,
		"Checking if remote repository exists"
	);

	if !loom_server_scm_mirror::check_repo_exists(
		mirror_info.platform,
		&mirror_info.external_owner,
		&mirror_info.external_repo,
	)
	.await
	.map_err(|e| ServerError::Internal(format!("Failed to check remote: {e}")))?
	{
		return Err(ServerError::NotFound(
			t(locale, "server.api.scm.mirror.remote_not_found").to_string(),
		));
	}

	let mirrors_org = state
		.org_repo
		.get_org_by_slug("mirrors")
		.await?
		.ok_or_else(|| {
			ServerError::Internal(t(locale, "server.api.scm.mirror.mirrors_org_not_found").to_string())
		})?;

	let repo_name = format!(
		"{}-{}-{}",
		mirror_info.platform.as_str(),
		mirror_info.external_owner,
		mirror_info.external_repo
	);

	let repo = Repository::new(
		OwnerType::Org,
		mirrors_org.id.into_inner(),
		repo_name,
		Visibility::Public,
	);

	let repo = scm_store
		.create(&repo)
		.await
		.map_err(|e| ServerError::Internal(format!("Failed to create repo: {e}")))?;

	info!(
		repo_id = %repo.id,
		platform = ?mirror_info.platform,
		owner = %mirror_info.external_owner,
		repo_name = %mirror_info.external_repo,
		"Created repository for on-demand mirror"
	);

	let create_mirror = CreateExternalMirror {
		platform: mirror_info.platform,
		external_owner: mirror_info.external_owner.clone(),
		external_repo: mirror_info.external_repo.clone(),
		repo_id: repo.id,
	};

	external_mirror_store
		.create(&create_mirror)
		.await
		.map_err(|e| ServerError::Internal(format!("Failed to create mirror entry: {e}")))?;

	info!(
		repo_id = %repo.id,
		platform = ?mirror_info.platform,
		owner = %mirror_info.external_owner,
		repo_name = %mirror_info.external_repo,
		"Created external mirror entry"
	);

	let repo_path = get_repo_path_by_id(repo.id);

	info!(
		repo_id = %repo.id,
		path = ?repo_path,
		platform = ?mirror_info.platform,
		owner = %mirror_info.external_owner,
		repo_name = %mirror_info.external_repo,
		"Starting on-demand mirror clone"
	);

	loom_server_scm_mirror::pull_mirror(
		mirror_info.platform,
		&mirror_info.external_owner,
		&mirror_info.external_repo,
		&repo_path,
	)
	.await
	.map_err(|e| {
		warn!(
			repo_id = %repo.id,
			error = %e,
			"On-demand mirror clone failed"
		);
		ServerError::Internal(t(locale, "server.api.scm.mirror.clone_failed").to_string())
	})?;

	if let Some(store) = &state.external_mirror_store {
		if let Ok(Some(mirror)) = store.get_by_repo_id(repo.id).await {
			let _ = store
				.update_last_synced(mirror.id, chrono::Utc::now())
				.await;
		}
	}

	info!(
		repo_id = %repo.id,
		platform = ?mirror_info.platform,
		owner = %mirror_info.external_owner,
		repo_name = %mirror_info.external_repo,
		"On-demand mirror clone completed successfully"
	);

	Ok(repo)
}

#[instrument(skip(state, headers), fields(owner = %owner, repo = %repo))]
pub async fn info_refs(
	Path((owner, repo)): Path<(String, String)>,
	Query(params): Query<InfoRefsParams>,
	OptionalAuth(auth): OptionalAuth,
	State(state): State<AppState>,
	headers: HeaderMap,
) -> Result<Response, ServerError> {
	let effective_user = match auth {
		Some(user) => Some(user),
		None => extract_basic_auth_user(&headers, &state).await,
	};

	let locale = effective_user
		.as_ref()
		.and_then(|u| u.user.locale.as_deref())
		.unwrap_or(&state.default_locale);

	let service = GitService::from_str(&params.service)
		.ok_or_else(|| ServerError::BadRequest(format!("Invalid service: {}", params.service)))?;

	let scm_repo = match resolve_repo(&owner, &repo, &state, locale).await {
		Ok(repo) => repo,
		Err(_) if is_mirror_path(&owner) => {
			let mirror_info = parse_mirror_path(&owner, &repo).ok_or_else(|| {
				ServerError::BadRequest(t(locale, "server.api.scm.mirror.invalid_path").to_string())
			})?;

			if service == GitService::ReceivePack {
				return Err(ServerError::Forbidden(
					t(locale, "server.api.scm.mirror.push_not_allowed").to_string(),
				));
			}

			info!(
				platform = ?mirror_info.platform,
				owner = %mirror_info.external_owner,
				repo = %mirror_info.external_repo,
				"On-demand mirror requested"
			);

			create_on_demand_mirror(&mirror_info, &state, locale).await?
		}
		Err(e) => return Err(e),
	};

	let repo_path = get_repo_path(&scm_repo);

	if !repo_path.exists() {
		return Err(ServerError::NotFound(
			t(locale, "server.api.scm.repo_not_found").to_string(),
		));
	}

	match service {
		GitService::UploadPack => {
			if let Err(e) = check_read_access(&scm_repo, effective_user.as_ref(), &state, locale).await {
				return match e {
					ServerError::Unauthorized(msg) => Ok(git_unauthorized_response(&msg)),
					other => Err(other),
				};
			}
		}
		GitService::ReceivePack => {
			if let Err(e) = check_write_access(&scm_repo, effective_user.as_ref(), &state, locale).await {
				return match e {
					ServerError::Unauthorized(msg) => Ok(git_unauthorized_response(&msg)),
					other => Err(other),
				};
			}
		}
	}

	update_mirror_access_time(&state, scm_repo.id).await;

	let git_output = run_git_command(&repo_path, service, &[], true).await?;

	let mut response_body = Vec::new();
	response_body.extend(pkt_line(&format!("# service={}\n", service.as_str())));
	response_body.extend(pkt_flush());
	response_body.extend(git_output);

	Ok(
		(
			StatusCode::OK,
			[
				(header::CONTENT_TYPE, service.content_type()),
				(header::CACHE_CONTROL, "no-cache"),
			],
			response_body,
		)
			.into_response(),
	)
}

#[instrument(skip(state, body, headers), fields(owner = %owner, repo = %repo))]
pub async fn upload_pack(
	Path((owner, repo)): Path<(String, String)>,
	OptionalAuth(auth): OptionalAuth,
	State(state): State<AppState>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<Response, ServerError> {
	let effective_user = match auth {
		Some(user) => Some(user),
		None => extract_basic_auth_user(&headers, &state).await,
	};

	let locale = effective_user
		.as_ref()
		.and_then(|u| u.user.locale.as_deref())
		.unwrap_or(&state.default_locale);

	let content_type = headers
		.get(header::CONTENT_TYPE)
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");

	if content_type != "application/x-git-upload-pack-request" {
		return Err(ServerError::BadRequest(format!(
			"Invalid content type: {content_type}"
		)));
	}

	let scm_repo = match resolve_repo(&owner, &repo, &state, locale).await {
		Ok(repo) => repo,
		Err(_) if is_mirror_path(&owner) => {
			let mirror_info = parse_mirror_path(&owner, &repo).ok_or_else(|| {
				ServerError::BadRequest(t(locale, "server.api.scm.mirror.invalid_path").to_string())
			})?;

			create_on_demand_mirror(&mirror_info, &state, locale).await?
		}
		Err(e) => return Err(e),
	};

	let repo_path = get_repo_path(&scm_repo);

	if !repo_path.exists() {
		return Err(ServerError::NotFound(
			t(locale, "server.api.scm.repo_not_found").to_string(),
		));
	}

	if let Err(e) = check_read_access(&scm_repo, effective_user.as_ref(), &state, locale).await {
		return match e {
			ServerError::Unauthorized(msg) => Ok(git_unauthorized_response(&msg)),
			other => Err(other),
		};
	}

	update_mirror_access_time(&state, scm_repo.id).await;

	let output = run_git_command(&repo_path, GitService::UploadPack, &body, false).await?;

	Ok(
		(
			StatusCode::OK,
			[
				(
					header::CONTENT_TYPE,
					GitService::UploadPack.result_content_type(),
				),
				(header::CACHE_CONTROL, "no-cache"),
			],
			output,
		)
			.into_response(),
	)
}

#[instrument(skip(state, body, headers), fields(owner = %owner, repo = %repo))]
pub async fn receive_pack(
	Path((owner, repo)): Path<(String, String)>,
	OptionalAuth(auth): OptionalAuth,
	State(state): State<AppState>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<Response, ServerError> {
	let effective_user = match auth {
		Some(user) => Some(user),
		None => extract_basic_auth_user(&headers, &state).await,
	};

	let locale = effective_user
		.as_ref()
		.and_then(|u| u.user.locale.as_deref())
		.unwrap_or(&state.default_locale);

	let content_type = headers
		.get(header::CONTENT_TYPE)
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");

	if content_type != "application/x-git-receive-pack-request" {
		return Err(ServerError::BadRequest(format!(
			"Invalid content type: {content_type}"
		)));
	}

	let scm_repo = resolve_repo(&owner, &repo, &state, locale).await?;
	let repo_path = get_repo_path(&scm_repo);

	if !repo_path.exists() {
		return Err(ServerError::NotFound(
			t(locale, "server.api.scm.repo_not_found").to_string(),
		));
	}

	if let Err(e) = check_write_access(&scm_repo, effective_user.as_ref(), &state, locale).await {
		return match e {
			ServerError::Unauthorized(msg) => Ok(git_unauthorized_response(&msg)),
			other => Err(other),
		};
	}

	let user = match effective_user.as_ref() {
		Some(u) => u,
		None => {
			return Ok(git_unauthorized_response(&t(
				locale,
				"server.api.scm.git.auth_required",
			)))
		}
	};

	if let Some(protection_store) = state.scm_protection_store.as_ref() {
		let rules = protection_store
			.list_by_repo(scm_repo.id)
			.await
			.map_err(|e| {
				ServerError::Internal(format!(
					"{}: {e}",
					t(locale, "server.api.scm.protection.failed_to_load")
				))
			})?;

		if !rules.is_empty() {
			let user_is_admin = check_user_is_repo_admin(&scm_repo, user, &state).await;
			let commands = parse_push_commands(&body);

			for cmd in commands {
				if let Some(branch) = extract_branch_name(&cmd.ref_name) {
					let is_deletion = cmd.new_sha == ZERO_SHA;
					let force_push = is_force_push(&repo_path, &cmd.old_sha, &cmd.new_sha).await;

					let check = PushCheck {
						branch: branch.to_string(),
						is_force_push: force_push,
						is_deletion,
						user_is_admin,
					};

					if let Err(violation) = check_push_allowed(&rules, &check) {
						tracing::warn!(
							repo_id = %scm_repo.id,
							branch = %branch,
							user_id = %user.user.id,
							violation = %violation,
							"Push blocked by branch protection"
						);
						return Err(ServerError::Forbidden(violation.to_string()));
					}
				}
			}
		}
	}

	let output = run_git_command(&repo_path, GitService::ReceivePack, &body, false).await?;

	Ok(
		(
			StatusCode::OK,
			[
				(
					header::CONTENT_TYPE,
					GitService::ReceivePack.result_content_type(),
				),
				(header::CACHE_CONTROL, "no-cache"),
			],
			output,
		)
			.into_response(),
	)
}

fn parse_mirror_git_path(path: &str) -> Option<(String, String)> {
	let path = path.strip_prefix('/').unwrap_or(path);

	let suffix_patterns = ["/info/refs", "/git-upload-pack", "/git-receive-pack"];
	for suffix in suffix_patterns {
		if let Some(repo_path) = path.strip_suffix(suffix) {
			if let Some(last_slash) = repo_path.rfind('/') {
				let platform_and_owner = &repo_path[..last_slash];
				let repo = &repo_path[last_slash + 1..];
				if !platform_and_owner.is_empty() && !repo.is_empty() {
					let owner = format!("mirrors/{}", platform_and_owner);
					return Some((owner, repo.to_string()));
				}
			}
		}
	}
	None
}

async fn git_wildcard_handler(
	axum::extract::Path(path): axum::extract::Path<String>,
	_method: axum::http::Method,
	query: Option<Query<InfoRefsParams>>,
	auth: OptionalAuth,
	state: State<AppState>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<Response, ServerError> {
	let (owner, repo) = parse_mirror_git_path(&path)
		.ok_or_else(|| ServerError::BadRequest("Invalid git path".to_string()))?;

	if path.ends_with("/info/refs") {
		let query =
			query.ok_or_else(|| ServerError::BadRequest("Missing service parameter".to_string()))?;
		info_refs(Path((owner, repo)), query, auth, state, headers).await
	} else if path.ends_with("/git-upload-pack") {
		upload_pack(Path((owner, repo)), auth, state, headers, body).await
	} else if path.ends_with("/git-receive-pack") {
		receive_pack(Path((owner, repo)), auth, state, headers, body).await
	} else {
		Err(ServerError::NotFound("Unknown git endpoint".to_string()))
	}
}

pub fn router() -> crate::OptionalAuthRouter {
	crate::OptionalAuthRouter::new()
		.route("/git/{owner}/{repo}/info/refs", get(info_refs))
		.route("/git/{owner}/{repo}/git-upload-pack", post(upload_pack))
		.route("/git/{owner}/{repo}/git-receive-pack", post(receive_pack))
		.route(
			"/git/mirrors/{*path}",
			get(|path, query, auth, state, headers, body| {
				git_wildcard_handler(
					path,
					axum::http::Method::GET,
					Some(query),
					auth,
					state,
					headers,
					body,
				)
			})
			.post(|path, auth, state, headers, body: Bytes| async move {
				git_wildcard_handler(
					path,
					axum::http::Method::POST,
					None,
					auth,
					state,
					headers,
					body,
				)
				.await
			}),
		)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_pkt_line() {
		let line = pkt_line("# service=git-upload-pack\n");
		assert_eq!(line, b"001e# service=git-upload-pack\n");
	}

	#[test]
	fn test_pkt_flush() {
		assert_eq!(pkt_flush(), b"0000");
	}

	#[test]
	fn test_git_service_from_str() {
		assert_eq!(
			GitService::from_str("git-upload-pack"),
			Some(GitService::UploadPack)
		);
		assert_eq!(
			GitService::from_str("git-receive-pack"),
			Some(GitService::ReceivePack)
		);
		assert_eq!(GitService::from_str("invalid"), None);
	}

	#[test]
	fn test_git_service_content_types() {
		assert_eq!(
			GitService::UploadPack.content_type(),
			"application/x-git-upload-pack-advertisement"
		);
		assert_eq!(
			GitService::ReceivePack.content_type(),
			"application/x-git-receive-pack-advertisement"
		);
		assert_eq!(
			GitService::UploadPack.result_content_type(),
			"application/x-git-upload-pack-result"
		);
		assert_eq!(
			GitService::ReceivePack.result_content_type(),
			"application/x-git-receive-pack-result"
		);
	}

	#[test]
	fn test_get_repos_base_dir() {
		let path = get_repos_base_dir();
		assert!(path.to_string_lossy().contains("repos"));
	}

	#[test]
	fn test_parse_push_commands_empty() {
		let commands = parse_push_commands(b"");
		assert!(commands.is_empty());
	}

	#[test]
	fn test_parse_push_commands_flush() {
		let commands = parse_push_commands(b"0000");
		assert!(commands.is_empty());
	}

	#[test]
	fn test_parse_push_commands_single() {
		let old = "0000000000000000000000000000000000000000";
		let new = "1234567890abcdef1234567890abcdef12345678";
		let refname = "refs/heads/main";
		let line = format!("{} {} {}\n", old, new, refname);
		let pkt = format!("{:04x}{}", line.len() + 4, line);
		let data = format!("{}0000", pkt);

		let commands = parse_push_commands(data.as_bytes());
		assert_eq!(commands.len(), 1);
		assert_eq!(commands[0].old_sha, old);
		assert_eq!(commands[0].new_sha, new);
		assert_eq!(commands[0].ref_name, refname);
	}

	#[test]
	fn test_extract_branch_name() {
		assert_eq!(extract_branch_name("refs/heads/main"), Some("main"));
		assert_eq!(
			extract_branch_name("refs/heads/feature/test"),
			Some("feature/test")
		);
		assert_eq!(extract_branch_name("refs/tags/v1.0"), None);
		assert_eq!(extract_branch_name("main"), None);
	}

	#[test]
	fn test_zero_sha_constant() {
		assert_eq!(ZERO_SHA.len(), 40);
		assert!(ZERO_SHA.chars().all(|c| c == '0'));
	}

	#[test]
	fn test_parse_mirror_path_github() {
		let result = parse_mirror_path("mirrors/github/torvalds", "linux.git");
		assert!(result.is_some());
		let info = result.unwrap();
		assert_eq!(info.platform, Platform::GitHub);
		assert_eq!(info.external_owner, "torvalds");
		assert_eq!(info.external_repo, "linux");
	}

	#[test]
	fn test_parse_mirror_path_gitlab() {
		let result = parse_mirror_path("mirrors/gitlab/gitlab-org", "gitlab");
		assert!(result.is_some());
		let info = result.unwrap();
		assert_eq!(info.platform, Platform::GitLab);
		assert_eq!(info.external_owner, "gitlab-org");
		assert_eq!(info.external_repo, "gitlab");
	}

	#[test]
	fn test_parse_mirror_path_invalid_prefix() {
		let result = parse_mirror_path("repos/github/owner", "repo");
		assert!(result.is_none());
	}

	#[test]
	fn test_parse_mirror_path_invalid_platform() {
		let result = parse_mirror_path("mirrors/bitbucket/owner", "repo");
		assert!(result.is_none());
	}

	#[test]
	fn test_parse_mirror_path_missing_parts() {
		let result = parse_mirror_path("mirrors/github", "repo");
		assert!(result.is_none());
	}

	#[test]
	fn test_is_mirror_path() {
		assert!(is_mirror_path("mirrors/github/owner"));
		assert!(is_mirror_path("mirrors/gitlab/owner"));
		assert!(!is_mirror_path("org/owner"));
		assert!(!is_mirror_path("user"));
	}

	#[test]
	fn test_parse_mirror_git_path_info_refs() {
		let result = parse_mirror_git_path("github/torvalds/linux.git/info/refs");
		assert!(result.is_some());
		let (owner, repo) = result.unwrap();
		assert_eq!(owner, "mirrors/github/torvalds");
		assert_eq!(repo, "linux.git");
	}

	#[test]
	fn test_parse_mirror_git_path_upload_pack() {
		let result = parse_mirror_git_path("github/torvalds/linux.git/git-upload-pack");
		assert!(result.is_some());
		let (owner, repo) = result.unwrap();
		assert_eq!(owner, "mirrors/github/torvalds");
		assert_eq!(repo, "linux.git");
	}

	#[test]
	fn test_parse_mirror_git_path_receive_pack() {
		let result = parse_mirror_git_path("gitlab/gitlab-org/gitlab/git-receive-pack");
		assert!(result.is_some());
		let (owner, repo) = result.unwrap();
		assert_eq!(owner, "mirrors/gitlab/gitlab-org");
		assert_eq!(repo, "gitlab");
	}

	#[test]
	fn test_parse_mirror_git_path_with_leading_slash() {
		let result = parse_mirror_git_path("/github/rust-lang/rust.git/info/refs");
		assert!(result.is_some());
		let (owner, repo) = result.unwrap();
		assert_eq!(owner, "mirrors/github/rust-lang");
		assert_eq!(repo, "rust.git");
	}

	#[test]
	fn test_parse_mirror_git_path_invalid() {
		assert!(parse_mirror_git_path("github/torvalds").is_none());
		assert!(parse_mirror_git_path("").is_none());
		assert!(parse_mirror_git_path("github/torvalds/linux.git").is_none());
	}

	#[test]
	fn test_get_repo_path_by_id() {
		let id = uuid::Uuid::parse_str("12345678-1234-1234-1234-123456789012").unwrap();
		let path = get_repo_path_by_id(id);
		assert!(path.to_string_lossy().contains("12"));
		assert!(path
			.to_string_lossy()
			.contains("12345678-1234-1234-1234-123456789012"));
		assert!(path.to_string_lossy().ends_with("git"));
	}

	use proptest::prelude::*;

	proptest! {
		/// **Property: Valid mirror paths parse successfully**
		/// A properly constructed mirror path should always parse correctly.
		#[test]
		fn prop_valid_mirror_path_parses(
			platform in prop_oneof!["github", "gitlab"],
			owner in "[a-zA-Z][a-zA-Z0-9_-]{2,20}",
			repo in "[a-zA-Z][a-zA-Z0-9_-]{2,20}"
		) {
			let owner_path = format!("mirrors/{}/{}", platform, owner);
			let repo_name = format!("{}.git", repo);

			let result = parse_mirror_path(&owner_path, &repo_name);
			prop_assert!(result.is_some(), "Valid path should parse: {}/{}", owner_path, repo_name);

			let info = result.unwrap();
			prop_assert_eq!(info.external_owner, owner);
			prop_assert_eq!(info.external_repo, repo);
		}

		/// **Property: Invalid prefix never parses as mirror**
		/// Paths not starting with "mirrors/" should return None.
		#[test]
		fn prop_non_mirror_prefix_fails(
			prefix in "[a-z]{3,10}",
			platform in "[a-z]{3,10}",
			owner in "[a-zA-Z0-9]{3,10}"
		) {
			prop_assume!(prefix != "mirrors");
			let owner_path = format!("{}/{}/{}", prefix, platform, owner);
			let result = parse_mirror_path(&owner_path, "repo.git");
			prop_assert!(result.is_none());
		}

		/// **Property: is_mirror_path consistent with parse_mirror_path**
		/// If is_mirror_path returns true, parse should have a chance to succeed.
		#[test]
		fn prop_is_mirror_path_consistency(
			platform in prop_oneof!["github", "gitlab"],
			owner in "[a-zA-Z][a-zA-Z0-9]{2,15}"
		) {
			let owner_path = format!("mirrors/{}/{}", platform, owner);
			prop_assert!(is_mirror_path(&owner_path));
		}

		/// **Property: parse_mirror_git_path extracts owner and repo**
		/// Valid git paths should parse to correct owner and repo components.
		#[test]
		fn prop_mirror_git_path_roundtrip(
			platform in prop_oneof!["github", "gitlab"],
			owner in "[a-zA-Z][a-zA-Z0-9]{2,15}",
			repo in "[a-zA-Z][a-zA-Z0-9]{2,15}",
			suffix in prop_oneof!["/info/refs", "/git-upload-pack", "/git-receive-pack"]
		) {
			let path = format!("{}/{}/{}.git{}", platform, owner, repo, suffix);
			let result = parse_mirror_git_path(&path);
			prop_assert!(result.is_some(), "Path should parse: {}", path);

			let (parsed_owner, parsed_repo) = result.unwrap();
			prop_assert!(parsed_owner.contains(&owner));
			prop_assert!(parsed_repo.contains(&repo));
		}
	}
}
