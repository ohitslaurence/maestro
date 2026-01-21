<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# GitHub App System Specification

**Status:** Draft\
**Version:** 1.1\
**Last Updated:** 2024-12-18

---

## 1. Overview

### Purpose

The GitHub App System enables Loom users to install a GitHub App on their repositories, providing
the Loom client with authenticated access to GitHub APIs for code search, private repository
introspection, and other GitHub operations.

### Primary Use Cases

1. **Code Search**: Search across private repository code using GitHub's search API
2. **Repository Introspection**: Fetch repository metadata (branches, visibility, settings)
3. **File Contents**: Retrieve file contents from private repositories
4. **Installation Management**: Track which repositories have the GitHub App installed

### Goals

- **Secure Authentication**: Use GitHub App authentication (JWT + installation tokens)
- **Automatic Token Refresh**: Cache and refresh installation tokens transparently
- **Webhook-Driven State**: Keep installation state synchronized via webhooks
- **Pattern Consistency**: Follow existing patterns (like `loom-google-cse`) for HTTP client
  structure
- **Minimal Footprint**: Focus on essential operations initially

### Non-Goals

- Full GitHub API coverage (PRs, issues, comments, etc.)
- Multi-GitHub-Enterprise support in initial version
- User-level OAuth flows (App-only authentication)
- Real-time webhook event streaming to clients

---

## 2. Architecture

### Crate Structure

```
crates/loom-github-app/
├── src/
│   ├── lib.rs           # Public API exports
│   ├── config.rs        # Configuration types
│   ├── client.rs        # GitHub App client with auth
│   ├── error.rs         # Error types with RetryableError impl
│   ├── types.rs         # Request/Response DTOs
│   ├── jwt.rs           # JWT generation utilities
│   └── webhook.rs       # Webhook signature verification
├── Cargo.toml
```

### Dependency Graph

```
                ┌─────────────────┐
                │   loom-server   │
                └────────┬────────┘
                         │
          ┌──────────────┴──────────────┐
          │                             │
          ▼                             ▼
┌─────────────────┐           ┌─────────────────┐
│ loom-github-app │           │ loom-google-cse │
└────────┬────────┘           └─────────────────┘
         │
         ▼
┌─────────────────┐
│ loom-http │
└─────────────────┘
```

### Component Interaction

```
┌─────────┐      ┌─────────────┐      ┌─────────────────┐      ┌────────┐
│  Client │◀────▶│ loom-server │◀────▶│ loom-github-app │◀────▶│ GitHub │
│  (CLI)  │      │             │      │     Client      │      │  API   │
└─────────┘      └──────┬──────┘      └─────────────────┘      └────────┘
                        │
                        ▼
                 ┌─────────────┐
                 │   SQLite    │
                 │ (installs)  │
                 └─────────────┘
```

---

## 3. Configuration

### Environment Variables

| Variable                                | Required | Description                                                      |
| --------------------------------------- | -------- | ---------------------------------------------------------------- |
| `LOOM_SERVER_GITHUB_APP_ID`             | Yes      | GitHub App numeric ID                                            |
| `LOOM_SERVER_GITHUB_APP_PRIVATE_KEY`    | Yes      | PEM-encoded RSA private key                                      |
| `LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET` | **Yes**  | Secret for webhook signature verification (enforced)             |
| `LOOM_SERVER_GITHUB_APP_SLUG`           | No       | App slug (defaults to "loom")                                    |
| `LOOM_SERVER_GITHUB_APP_BASE_URL`       | No       | API base URL (defaults to https://api.github.com, must be HTTPS) |

### Configuration Type

```rust
#[derive(Clone)]
pub struct GithubAppConfig {
	/// GitHub App numeric ID (private, use accessor)
	app_id: u64,

	/// PEM-encoded RSA private key for JWT signing (private, never exposed)
	private_key_pem: String,

	/// Secret for webhook signature verification (required for webhooks)
	webhook_secret: Option<String>,

	/// App slug for installation URL generation
	app_slug: String,

	/// Base URL for GitHub API (validated HTTPS, normalized)
	base_url: Url,

	/// HTTP retry configuration
	pub retry_config: RetryConfig,
}

impl GithubAppConfig {
	/// Create configuration from environment variables
	/// Validates base_url is HTTPS and has a host
	fn from_env() -> Result<Self, GithubAppError>;

	/// Builder method for custom base URL (validates HTTPS)
	fn with_base_url(self, url: impl Into<String>) -> Self;

	/// Builder method for custom retry config
	fn with_retry_config(self, config: RetryConfig) -> Self;

	/// Accessors for private fields
	fn app_id(&self) -> u64;
	fn base_url(&self) -> &Url;
	fn webhook_secret(&self) -> Option<&str>;
}
```

### Base URL Validation

The `base_url` is validated and normalized at construction time:

1. Must be a valid URL
2. Must use HTTPS scheme (security requirement)
3. Must have a host (no localhost or bare IPs in production)
4. Trailing slashes are normalized

```rust
fn validate_and_normalize_base_url(raw: &str) -> Result<Url, GithubAppError> {
	let url = Url::parse(raw)?;

	if url.scheme() != "https" {
		return Err(GithubAppError::Config("GitHub base URL must use https"));
	}

	if url.host_str().is_none() {
		return Err(GithubAppError::Config(
			"GitHub base URL must include a host",
		));
	}

	Ok(url)
}
```

---

## 4. Authentication Flow

### GitHub App Authentication Model

GitHub Apps use a two-tier authentication system:

1. **App-Level JWT**: Short-lived JWT signed with the app's private key
2. **Installation Access Token**: Per-installation token obtained using the JWT

### JWT Generation

```
┌──────────────────────────────────────────────────────────────────┐
│                        JWT Claims                                 │
├──────────────────────────────────────────────────────────────────┤
│  iss: app_id                    (Issuer = App ID)                │
│  iat: now - 60 seconds          (Issued at, with clock skew)     │
│  exp: now + 9 minutes           (Expiration, max 10 min)         │
│  alg: RS256                     (Algorithm)                       │
└──────────────────────────────────────────────────────────────────┘
```

### Token Lifecycle

```
┌─────────────────┐
│ Request Arrives │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────────┐
│ Check cached installation token │
└────────────────┬────────────────┘
                 │
         ┌───────┴───────┐
         │               │
    Cache Hit       Cache Miss/Expired
    (expires_at     (expires_at <= now + 2min)
     > now + 2min)       │
         │               ▼
         │  ┌─────────────────────────────┐
         │  │ Get/refresh App JWT         │
         │  │ (cached if expires > 30s)   │
         │  └──────────────┬──────────────┘
         │                 │
         │                 ▼
         │  ┌─────────────────────────────┐
         │  │ POST /app/installations/    │
         │  │   {id}/access_tokens        │
         │  └──────────────┬──────────────┘
         │                 │
         │                 ▼
         │  ┌─────────────────────────────┐
         │  │ Cache new token with expiry │
         │  └──────────────┬──────────────┘
         │                 │
         └────────┬────────┘
                  │
                  ▼
         ┌────────────────┐
         │ Return Token   │
         └────────────────┘
```

### Token Caching Strategy

```rust
struct CachedToken {
	token: String,
	expires_at: Instant,
}

/// In-memory token cache
pub struct TokenCache {
	/// App JWT (single, short-lived)
	app_jwt: Arc<Mutex<Option<CachedToken>>>,

	/// Installation tokens keyed by installation_id
	installation_tokens: Arc<Mutex<HashMap<i64, CachedToken>>>,
}
```

**Refresh margins:**

- App JWT: Refresh if `expires_at <= now + 30 seconds`
- Installation Token: Refresh if `expires_at <= now + 2 minutes`

### Token Refresh on 401

If GitHub responds with 401 for an installation-scoped request, the token is considered stale:

1. Invalidate the cached token for that installation
2. Fetch a new installation token
3. Retry the request once with the fresh token

This is handled at the client layer, separate from the generic retry logic:

```rust
async fn search_code_with_refresh(
	&self,
	installation_id: i64,
	query: &str,
) -> Result<CodeSearchResponse, GithubAppError> {
	let token = self.get_installation_token(installation_id).await?;

	match self.search_code_inner(&token, query).await {
		Ok(resp) => Ok(resp),
		Err(GithubAppError::Unauthorized) => {
			// Stale token: invalidate and retry once
			self.invalidate_installation_token(installation_id).await;
			let fresh = self.get_installation_token(installation_id).await?;
			self.search_code_inner(&fresh, query).await
		}
		Err(e) => Err(e),
	}
}
```

### Token Request Deduplication

To prevent thundering-herd when multiple concurrent requests need tokens:

```rust
pub struct GithubAppClient {
	// Token caches
	app_jwt_cache: Arc<Mutex<Option<CachedToken>>>,
	installation_token_cache: Arc<Mutex<HashMap<i64, CachedToken>>>,

	// Deduplication locks
	app_jwt_lock: Arc<Mutex<()>>,
	installation_locks: Arc<Mutex<HashMap<i64, Arc<Mutex<()>>>>>,
}
```

Pattern:

1. Check cache (fast path)
2. If miss, acquire per-installation lock
3. Double-check cache under lock
4. If still miss, fetch and cache

This ensures only one token fetch per installation is in-flight at a time.

---

## 5. Error Handling

### Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum GithubAppError {
	/// Network-level error during HTTP communication
	#[error("Network error: {0}")]
	Network(#[from] reqwest::Error),

	/// Request timed out
	#[error("Request timed out")]
	Timeout,

	/// Invalid API key or app configuration
	#[error("Unauthorized or invalid app configuration")]
	Unauthorized,

	/// Forbidden - insufficient permissions
	#[error("Forbidden or insufficient permissions")]
	Forbidden,

	/// Rate limit exceeded
	#[error("Rate limit exceeded")]
	RateLimited,

	/// GitHub API returned an error
	#[error("GitHub API error: {status} - {message}")]
	ApiError { status: u16, message: String },

	/// Invalid or unparseable response
	#[error("Invalid response from GitHub: {0}")]
	InvalidResponse(String),

	/// Configuration error
	#[error("Configuration error: {0}")]
	Config(String),

	/// JWT signing/encoding error
	#[error("JWT error: {0}")]
	Jwt(String),

	/// Installation not found for repository
	#[error("GitHub App not installed for {owner}/{repo}")]
	InstallationNotFound { owner: String, repo: String },

	/// Webhook signature verification failed
	#[error("Invalid webhook signature")]
	InvalidWebhookSignature,
}
```

### Retry Behavior

```rust
impl RetryableError for GithubAppError {
	fn is_retryable(&self) -> bool {
		match self {
			GithubAppError::Network(e) => e.is_retryable(),
			GithubAppError::Timeout => true,
			GithubAppError::RateLimited => true,
			GithubAppError::ApiError { status, .. } => *status >= 500,
			_ => false,
		}
	}
}
```

---

## 6. Client API

### GithubAppClient

```rust
#[derive(Clone)]
pub struct GithubAppClient {
	http_client: Client,
	config: GithubAppConfig,
	token_cache: TokenCache,
}

impl GithubAppClient {
	/// Create a new GitHub App client
	fn new(config: GithubAppConfig) -> Result<Self, GithubAppError>;

	/// Search code within a repository
	async fn search_code(
		&self,
		installation_id: i64,
		request: CodeSearchRequest,
	) -> Result<CodeSearchResponse, GithubAppError>;

	/// Get repository metadata
	async fn get_repository(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
	) -> Result<Repository, GithubAppError>;

	/// Get file contents
	async fn get_file_contents(
		&self,
		installation_id: i64,
		owner: &str,
		repo: &str,
		path: &str,
		git_ref: Option<&str>,
	) -> Result<FileContents, GithubAppError>;

	/// List installations for the app (uses app JWT, not installation token)
	async fn list_installations(&self) -> Result<Vec<Installation>, GithubAppError>;

	/// Get installation for a specific repository
	async fn get_repo_installation(
		&self,
		owner: &str,
		repo: &str,
	) -> Result<Installation, GithubAppError>;
}
```

---

## 7. Server API Endpoints

### Webhook Endpoint

**POST /v1/github/webhook**

Receives webhook events from GitHub.

**Headers:**

- `X-Hub-Signature-256`: HMAC-SHA256 signature of request body
- `X-GitHub-Event`: Event type (e.g., `installation`, `installation_repositories`)
- `X-GitHub-Delivery`: Unique delivery ID

**Supported Events:**

| Event                       | Action        | Description                            |
| --------------------------- | ------------- | -------------------------------------- |
| `installation`              | `created`     | App installed on account               |
| `installation`              | `deleted`     | App uninstalled                        |
| `installation`              | `suspended`   | App suspended                          |
| `installation`              | `unsuspended` | App unsuspended                        |
| `installation_repositories` | `added`       | Repositories added to installation     |
| `installation_repositories` | `removed`     | Repositories removed from installation |

**Response:**

- `200 OK`: Event processed
- `401 Unauthorized`: Invalid signature
- `400 Bad Request`: Invalid payload

### App Info Endpoint

**GET /v1/github/app**

Returns information about the configured GitHub App.

**Response:**

```json
{
	"configured": true,
	"app_slug": "loom",
	"installation_url": "https://github.com/apps/loom/installations/new"
}
```

**Response (not configured):**

```json
{
	"configured": false,
	"app_slug": null,
	"installation_url": null
}
```

### Installation Status Endpoint

**GET /v1/github/installations/by-repo**

Check if the GitHub App is installed for a specific repository.

**Query Parameters:**

- `owner` (required): Repository owner
- `repo` (required): Repository name

**Response (installed):**

```json
{
	"installed": true,
	"installation_id": 12345678,
	"account_login": "my-org",
	"account_type": "Organization",
	"repositories_selection": "selected"
}
```

**Response (not installed):**

```json
{
	"installed": false,
	"installation_id": null,
	"account_login": null,
	"account_type": null,
	"repositories_selection": null
}
```

### Code Search Proxy Endpoint

**POST /proxy/github/search-code**

Proxy code search requests to GitHub API.

**Request:**

```json
{
	"owner": "my-org",
	"repo": "my-repo",
	"query": "struct Config language:rust",
	"per_page": 20,
	"page": 1
}
```

**Response:**

```json
{
	"total_count": 42,
	"incomplete_results": false,
	"items": [
		{
			"name": "config.rs",
			"path": "src/config.rs",
			"sha": "abc123...",
			"html_url": "https://github.com/my-org/my-repo/blob/main/src/config.rs",
			"repository_full_name": "my-org/my-repo",
			"score": 12.345
		}
	]
}
```

**Error Responses:**

| Status | Condition                               |
| ------ | --------------------------------------- |
| `400`  | Missing or invalid parameters           |
| `404`  | GitHub App not installed for repository |
| `429`  | GitHub rate limit exceeded              |
| `500`  | Internal server error                   |
| `502`  | GitHub API error                        |

### Repository Info Proxy Endpoint

**POST /proxy/github/repo-info**

Get repository metadata.

**Request:**

```json
{
	"owner": "my-org",
	"repo": "my-repo"
}
```

**Response:**

```json
{
	"id": 123456789,
	"full_name": "my-org/my-repo",
	"description": "My awesome repository",
	"private": true,
	"default_branch": "main",
	"language": "Rust",
	"stargazers_count": 42,
	"html_url": "https://github.com/my-org/my-repo"
}
```

### File Contents Proxy Endpoint

**POST /proxy/github/file-contents**

Get file contents from a repository.

**Request:**

```json
{
	"owner": "my-org",
	"repo": "my-repo",
	"path": "src/main.rs",
	"ref": "main"
}
```

**Response:**

```json
{
	"name": "main.rs",
	"path": "src/main.rs",
	"sha": "abc123...",
	"size": 1234,
	"encoding": "base64",
	"content": "Zm4gbWFpbigpIHsKICAgIHByaW50bG4hKCJIZWxsbyIpOwp9Cg=="
}
```

---

## 8. Database Schema

### Migration: 006_github_app.sql

```sql
-- GitHub App installations table
CREATE TABLE IF NOT EXISTS github_installations (
    installation_id      INTEGER PRIMARY KEY,  -- GitHub installation ID
    account_id           INTEGER NOT NULL,     -- GitHub account ID (user/org)
    account_login        TEXT NOT NULL,        -- Account login name
    account_type         TEXT NOT NULL,        -- "User" or "Organization"
    app_slug             TEXT,                 -- App slug (for multi-app setups)
    repositories_selection TEXT NOT NULL,      -- "all" or "selected"
    suspended_at         TEXT,                 -- ISO8601 timestamp or NULL
    created_at           TEXT NOT NULL,        -- ISO8601 timestamp
    updated_at           TEXT NOT NULL         -- ISO8601 timestamp
);

-- Repository to installation mapping
CREATE TABLE IF NOT EXISTS github_installation_repos (
    repository_id        INTEGER PRIMARY KEY,  -- GitHub repository ID
    installation_id      INTEGER NOT NULL,     -- FK to github_installations
    owner                TEXT NOT NULL,        -- Repository owner
    name                 TEXT NOT NULL,        -- Repository name
    full_name            TEXT NOT NULL,        -- "owner/name"
    private              INTEGER NOT NULL,     -- 0 = public, 1 = private
    default_branch       TEXT,                 -- Default branch name
    created_at           TEXT NOT NULL,        -- ISO8601 timestamp
    updated_at           TEXT NOT NULL,        -- ISO8601 timestamp
    FOREIGN KEY (installation_id) REFERENCES github_installations(installation_id)
        ON DELETE CASCADE
);

-- Index for fast owner/name lookups
CREATE INDEX IF NOT EXISTS idx_github_installation_repos_owner_name
    ON github_installation_repos (owner, name);

-- Index for installation_id lookups
CREATE INDEX IF NOT EXISTS idx_github_installation_repos_installation
    ON github_installation_repos (installation_id);
```

### Repository Methods

```rust
impl ThreadRepository {
	/// Upsert a GitHub installation from webhook data
	async fn upsert_github_installation(
		&self,
		installation: &GithubInstallation,
	) -> Result<(), ServerError>;

	/// Delete a GitHub installation (cascades to repos)
	async fn delete_github_installation(&self, installation_id: i64) -> Result<bool, ServerError>;

	/// Suspend/unsuspend an installation
	async fn update_github_installation_suspension(
		&self,
		installation_id: i64,
		suspended_at: Option<&str>,
	) -> Result<bool, ServerError>;

	/// Add repositories to an installation
	async fn add_github_installation_repos(
		&self,
		installation_id: i64,
		repos: &[GithubRepo],
	) -> Result<(), ServerError>;

	/// Remove repositories from an installation
	async fn remove_github_installation_repos(
		&self,
		repository_ids: &[i64],
	) -> Result<(), ServerError>;

	/// Get installation ID for a repository by owner/name
	async fn get_github_installation_for_repo(
		&self,
		owner: &str,
		name: &str,
	) -> Result<Option<GithubInstallationInfo>, ServerError>;

	/// List all installations
	async fn list_github_installations(&self) -> Result<Vec<GithubInstallation>, ServerError>;
}
```

---

## 9. Types (DTOs)

### Request Types

```rust
/// Code search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchRequest {
	pub query: String,
	pub owner: String,
	pub repo: String,
	#[serde(default = "default_per_page")]
	pub per_page: u32,
	#[serde(default = "default_page")]
	pub page: u32,
}

/// File contents request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentsRequest {
	pub owner: String,
	pub repo: String,
	pub path: String,
	#[serde(rename = "ref")]
	pub git_ref: Option<String>,
}

/// Repository info request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfoRequest {
	pub owner: String,
	pub repo: String,
}
```

### Response Types

```rust
/// Code search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchResponse {
	pub total_count: u32,
	pub incomplete_results: bool,
	pub items: Vec<CodeSearchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSearchItem {
	pub name: String,
	pub path: String,
	pub sha: String,
	pub html_url: String,
	pub repository_full_name: String,
	pub score: f64,
}

/// Repository info response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
	pub id: i64,
	pub full_name: String,
	pub description: Option<String>,
	pub private: bool,
	pub default_branch: String,
	pub language: Option<String>,
	pub stargazers_count: u32,
	pub html_url: String,
}

/// File contents response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContents {
	pub name: String,
	pub path: String,
	pub sha: String,
	pub size: u64,
	pub encoding: String,
	pub content: String,
}

/// Installation info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installation {
	pub id: i64,
	pub account: InstallationAccount,
	pub repository_selection: String,
	pub suspended_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationAccount {
	pub id: i64,
	pub login: String,
	#[serde(rename = "type")]
	pub account_type: String,
}
```

---

## 10. Webhook Handling

### Signature Verification

```rust
/// Verify GitHub webhook signature
pub fn verify_webhook_signature(
	secret: &str,
	signature_header: &str,
	body: &[u8],
) -> Result<(), GithubAppError> {
	// signature_header format: "sha256=<hex>"
	let expected_prefix = "sha256=";
	if !signature_header.starts_with(expected_prefix) {
		return Err(GithubAppError::InvalidWebhookSignature);
	}

	let expected_signature = &signature_header[expected_prefix.len()..];
	let expected_bytes =
		hex::decode(expected_signature).map_err(|_| GithubAppError::InvalidWebhookSignature)?;

	let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
		.map_err(|_| GithubAppError::InvalidWebhookSignature)?;
	mac.update(body);

	mac
		.verify_slice(&expected_bytes)
		.map_err(|_| GithubAppError::InvalidWebhookSignature)
}
```

### Webhook Event Processing

```rust
/// Process webhook events
pub async fn handle_webhook_event(
	event_type: &str,
	payload: &WebhookPayload,
	repo: &ThreadRepository,
) -> Result<(), ServerError> {
	match event_type {
		"installation" => handle_installation_event(payload, repo).await,
		"installation_repositories" => handle_installation_repos_event(payload, repo).await,
		_ => {
			tracing::debug!(event_type = %event_type, "Ignoring unhandled webhook event");
			Ok(())
		}
	}
}
```

---

## 11. Testing Strategy

### Property-Based Tests

```rust
proptest! {
		/// **Property: JWT tokens are valid for expected duration**
		///
		/// Why: Ensures JWT generation creates tokens with correct expiry
		/// and follows GitHub's maximum 10-minute lifetime constraint
		#[test]
		fn prop_jwt_claims_are_valid(app_id in 1u64..=u64::MAX) {
				let token = generate_app_jwt(app_id, TEST_RSA_PRIVATE_KEY).unwrap();
				let claims = decode_claims(&token);

				// Issuer matches app_id
				prop_assert_eq!(claims.iss, app_id.to_string());

				// exp > iat
				prop_assert!(claims.exp > claims.iat);

				// Lifetime <= 10 minutes (GitHub max)
				let lifetime = claims.exp - claims.iat;
				prop_assert!(lifetime <= 10 * 60);

				// Token is not already expired
				let now = current_unix_timestamp();
				prop_assert!(claims.exp > now);
		}

		/// **Property: Token cache returns same token within validity window**
		///
		/// Why: Ensures caching works correctly and avoids unnecessary token refresh
		#[test]
		fn cached_token_reuse(installation_id in 1i64..1000000i64) {
				// Get token twice within validity window, assert same token returned
		}

		/// **Property: Webhook signature verification rejects tampered payloads**
		///
		/// Why: Security critical - ensures webhook verification is correct
		#[test]
		fn prop_tampered_body_fails_verification(
				secret in "[a-zA-Z0-9]{8,64}",
				body in proptest::collection::vec(any::<u8>(), 2..500),
				tamper_index in 0usize..500usize
		) {
				let signature = compute_webhook_signature(&secret, &body);

				let mut tampered = body.clone();
				let idx = tamper_index % tampered.len();
				tampered[idx] = tampered[idx].wrapping_add(1);

				if tampered != body {
						let result = verify_webhook_signature(&secret, &signature, &tampered);
						prop_assert!(result.is_err());
				}
		}

		/// **Property: Signature format is always sha256= followed by 64 hex chars**
		///
		/// Why: Ensures signature output format matches GitHub's expected format
		#[test]
		fn prop_signature_format_is_correct(
				secret in "[a-zA-Z0-9]{1,100}",
				body in proptest::collection::vec(any::<u8>(), 0..1000)
		) {
				let signature = compute_webhook_signature(&secret, &body);
				prop_assert!(signature.starts_with("sha256="));
				prop_assert_eq!(signature.len(), "sha256=".len() + 64);
		}
}
```

### Database Cascade Tests

```rust
#[tokio::test]
async fn test_deleting_installation_cascades_repos() {
	let repo = ThreadRepository::for_tests().await?;

	// Insert installation + repos
	let installation_id = 123_i64;
	repo.upsert_github_installation(&installation).await?;
	repo
		.add_github_installation_repos(installation_id, &repos)
		.await?;

	// Verify mapping exists
	let found = repo
		.get_github_installation_for_repo("owner", "repo")
		.await?;
	assert!(found.is_some());

	// Delete installation
	repo.delete_github_installation(installation_id).await?;

	// Mapping should be gone (cascade delete)
	let found_after = repo
		.get_github_installation_for_repo("owner", "repo")
		.await?;
	assert!(found_after.is_none());
}
```

### Webhook Security Tests

```rust
#[tokio::test]
async fn test_webhook_rejects_missing_secret_config() {
	// Server with no webhook secret configured
	// Send valid webhook → 500 Internal
}

#[tokio::test]
async fn test_webhook_rejects_missing_signature() {
	// Send webhook without X-Hub-Signature-256 header → 400 BadRequest
}

#[tokio::test]
async fn test_webhook_rejects_invalid_signature() {
	// Send webhook with wrong signature → 401 Unauthorized
}

#[tokio::test]
async fn test_webhook_accepts_valid_signature() {
	// Send webhook with correct signature → 200 OK
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_installation_webhook_creates_record() {
	// Send installation.created webhook with valid signature
	// Verify database record created
}

#[tokio::test]
async fn test_installation_deleted_removes_records() {
	// Create installation and repos
	// Send installation.deleted webhook
	// Verify all records removed
}

#[tokio::test]
async fn test_proxy_search_code_returns_results() {
	// Mock GitHub API
	// Call /proxy/github/search-code
	// Verify response structure
}

#[tokio::test]
async fn test_proxy_returns_404_for_uninstalled_repo() {
	// Call proxy endpoint for repo without installation
	// Verify 404 response with helpful message
}

#[tokio::test]
async fn test_401_triggers_token_refresh() {
	// Mock GitHub to return 401 on first call, 200 on second
	// Call proxy endpoint
	// Verify request succeeds after token refresh
}
```

---

## 12. Health Check Integration

Add GitHub App status to health check response (per `health-check.md` spec):

### Health Model

```rust
/// GitHub App component health.
#[derive(Debug, Serialize)]
pub struct GithubAppHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthComponents {
	pub database: DatabaseHealth,
	pub bin_dir: BinDirHealth,
	pub llm_providers: LlmProvidersHealth,
	pub google_cse: GoogleCseHealth,
	pub github_app: GithubAppHealth, // New
}
```

### Health Check Logic

```rust
const GITHUB_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

pub async fn check_github_app(client: Option<Arc<GithubAppClient>>) -> GithubAppHealth {
	let start = Instant::now();

	let (configured, status, error) = match client {
		None => (
			false,
			HealthStatus::Degraded,
			Some("GitHub App not configured".to_string()),
		),
		Some(client) => {
			// Lightweight check: list installations (validates JWT + API connectivity)
			match timeout(GITHUB_CHECK_TIMEOUT, client.list_installations()).await {
				Ok(Ok(_)) => (true, HealthStatus::Healthy, None),
				Ok(Err(e)) => {
					let status = match e {
						GithubAppError::Unauthorized | GithubAppError::Config(_) | GithubAppError::Jwt(_) => {
							HealthStatus::Unhealthy
						}
						_ => HealthStatus::Degraded,
					};
					(true, status, Some(e.to_string()))
				}
				Err(_) => (
					true,
					HealthStatus::Degraded,
					Some("GitHub health check timed out".to_string()),
				),
			}
		}
	};

	GithubAppHealth {
		status,
		latency_ms: start.elapsed().as_millis() as u64,
		configured,
		error,
	}
}
```

### Status Mapping

| Condition                 | Status                          |
| ------------------------- | ------------------------------- |
| Not configured            | `degraded` (optional component) |
| Configured and responsive | `healthy`                       |
| Auth/config error         | `unhealthy`                     |
| Timeout/network error     | `degraded`                      |
| Rate limited              | `degraded`                      |
| 5xx from GitHub           | `degraded`                      |

---

## 13. Security Considerations

### Credential Handling

- **Private Key**: Stored in environment variable, never logged, never exposed via API
- **Webhook Secret**: Required for webhook processing, used only for HMAC verification
- **Installation Tokens**: Short-lived (1 hour), cached in memory only
- **App JWT**: Very short-lived (10 minutes max), not persisted
- **Base URL**: Validated as HTTPS at config time to prevent SSRF

### Webhook Security

Webhook processing requires:

1. **Webhook secret must be configured** - server returns 500 if missing
2. **X-Hub-Signature-256 header required** - request rejected with 400 if missing
3. **Signature verification** - request rejected with 401 if invalid

```rust
// In webhook handler
let secret = client.webhook_secret().ok_or_else(|| {
    ServerError::Internal("GitHub webhook secret is not configured")
})?;

let sig_header = headers
    .get("X-Hub-Signature-256")
    .ok_or_else(|| ServerError::BadRequest("Missing signature header"))?;

verify_webhook_signature(secret, sig_header, &body)?;
```

### Input Validation

- **Base URL**: Must be HTTPS, must have host, no localhost/IPs
- **per_page**: Clamped to 1-100 range
- **page**: Must be >= 1
- **Repository access**: Scoped by installation in database

### Access Control

- **Initial Version**: All clients can query any installed repository
- **Future**: Add user authentication and per-user access controls

### Logging

```rust
// DO: Log operation metadata
tracing::info!(
		installation_id = %installation_id,
		owner = %owner,
		repo = %repo,
		"Performing code search"
);

// DON'T: Log tokens or secrets
// Never log: private_key_pem, access_token, webhook_secret
```

---

## 14. Future Considerations

### 14.1 Extended GitHub Operations

- Pull request listing and review
- Issue management
- Commit and branch operations
- GitHub Actions integration

### 14.2 Multi-User Support

- User authentication with the server
- Per-user installation visibility
- Rate limit accounting per user

### 14.3 GitHub Enterprise

- Multiple base URLs
- Per-installation base URL support
- Custom CA certificates

### 14.4 Enhanced Caching

- Redis-backed token cache for multi-instance deployments
- Background token refresh before expiry
- Circuit breaker for GitHub API failures

---

## Appendix A: GitHub App Setup Instructions

### Creating the GitHub App

1. Go to GitHub Settings → Developer settings → GitHub Apps
2. Click "New GitHub App"
3. Configure:
   - **Name**: `loom` (or your preferred name)
   - **Homepage URL**: Your Loom server URL
   - **Webhook URL**: `https://your-server/v1/github/webhook`
   - **Webhook secret**: Generate a secure random string
4. Permissions required:
   - **Repository permissions**:
     - Contents: Read
     - Metadata: Read
   - **Account permissions**: None
5. Subscribe to events:
   - Installation
   - Installation repositories
6. Generate and download private key
7. Note the App ID

### Environment Setup

```bash
export LOOM_SERVER_GITHUB_APP_ID="123456"
export LOOM_SERVER_GITHUB_APP_PRIVATE_KEY="$(cat path/to/private-key.pem)"
export LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET="your-webhook-secret"
export LOOM_SERVER_GITHUB_APP_SLUG="loom"
```

---

## Appendix B: API Error Responses

### Standard Error Format

```json
{
	"error": {
		"code": "INSTALLATION_NOT_FOUND",
		"message": "GitHub App not installed for my-org/my-repo",
		"details": {
			"owner": "my-org",
			"repo": "my-repo",
			"installation_url": "https://github.com/apps/loom/installations/new"
		}
	}
}
```

### Error Codes

| Code                     | HTTP Status | Description                         |
| ------------------------ | ----------- | ----------------------------------- |
| `INSTALLATION_NOT_FOUND` | 404         | App not installed for repo          |
| `RATE_LIMITED`           | 429         | GitHub rate limit exceeded          |
| `GITHUB_ERROR`           | 502         | GitHub API returned error           |
| `NOT_CONFIGURED`         | 503         | GitHub App not configured on server |
| `INVALID_SIGNATURE`      | 401         | Webhook signature invalid           |
