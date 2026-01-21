# Anthropic Max Pool Management Specification

**Status:** Draft  
**Version:** 1.0  
**Last Updated:** 2026-01-01

---

## 1. Overview

### Purpose

Enable loom-server administrators to manage a pool of Claude Max OAuth subscriptions through a web interface. The pool provides automatic failover when accounts hit their 5-hour rolling quota limits, and proactively refreshes OAuth tokens to prevent expiration.

### Goals

- **Web-based account management**: Admins can add/remove Claude Max accounts via loom-web
- **OAuth flow integration**: Adding an account triggers OAuth redirect to `claude.ai`
- **Pool visibility**: Admin UI displays account status (available/cooling/disabled)
- **Proactive token refresh**: Background task refreshes tokens before expiration
- **Hot-reload**: Token updates propagate to active pool without restart

### Non-Goals

- CLI-based account management (may be added later)
- Support for Console OAuth (API key creation flow)
- Multi-tenant pool isolation (all accounts shared server-wide)

---

## 2. Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              loom-web (Admin UI)                            │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  /admin/anthropic-accounts                                          │    │
│  │  - List accounts with status badges                                 │    │
│  │  - "Add Account" button → OAuth redirect                            │    │
│  │  - "Remove" button per account                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              loom-server                                     │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  Admin Routes (SystemRole::Admin only)                               │   │
│  │  GET  /api/admin/anthropic/accounts     → List accounts + status     │   │
│  │  POST /api/admin/anthropic/accounts     → Initiate OAuth flow        │   │
│  │  DELETE /api/admin/anthropic/accounts/{id} → Remove account          │   │
│  │  GET  /api/admin/anthropic/callback     → OAuth callback             │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                      │                                       │
│                                      ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  LlmService                                                          │   │
│  │  - Holds AnthropicPool (when OAuth configured)                       │   │
│  │  - Exposes pool management methods                                   │   │
│  │  - Starts background refresh task                                    │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                      │                                       │
│                                      ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  AnthropicPool                                                       │   │
│  │  - Round-robin account selection                                     │   │
│  │  - Automatic failover on quota exhaustion                            │   │
│  │  - spawn_refresh_task() for proactive token refresh                  │   │
│  │  - add_account() / remove_account() for dynamic management           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                      │                                       │
│                                      ▼                                       │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  FileCredentialStore                                                 │   │
│  │  Path: /var/lib/loom-server/anthropic-credentials.json (configurable)│   │
│  │  Permissions: 0600                                                   │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              claude.ai                                       │
│  OAuth Authorization: https://claude.ai/oauth/authorize                      │
│  Token Exchange: https://console.anthropic.com/v1/oauth/token                │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Credential Storage

### File Format

Credentials are stored in a JSON file keyed by account ID:

```json
{
  "claude-max-1": {
    "type": "oauth",
    "refresh": "rt_abc123...",
    "access": "at_xyz789...",
    "expires": 1735500000000
  },
  "claude-max-2": {
    "type": "oauth",
    "refresh": "rt_def456...",
    "access": "at_uvw000...",
    "expires": 1735500100000
  }
}
```

### File Location

- Default: `/var/lib/loom-server/anthropic-credentials.json`
- Configurable via NixOS module: `services.loom-server.anthropic.oauthCredentialFile`
- Configurable via env var: `LOOM_SERVER_ANTHROPIC_OAUTH_CREDENTIAL_FILE`

### Security

- File permissions: `0600` (owner read/write only)
- Owner: `loom-server` system user
- Tokens are wrapped in `SecretString` in memory (auto-redacts in logs)

---

## 4. OAuth Flow

### Adding an Account

1. Admin clicks "Add Account" in loom-web
2. Frontend calls `POST /api/admin/anthropic/accounts`
3. Server generates PKCE challenge and state, stores in `OAuthStateStore`
4. Server returns redirect URL: `https://claude.ai/oauth/authorize?...`
5. Frontend redirects admin to claude.ai
6. Admin authorizes in claude.ai
7. claude.ai redirects to `GET /api/admin/anthropic/callback?code=...&state=...`
8. Server validates state, exchanges code for tokens
9. Server stores tokens in credential file
10. Server adds account to pool (hot-reload)
11. Server redirects admin back to account list page

### OAuth Parameters

| Parameter | Value |
|-----------|-------|
| `client_id` | `9d1c250a-e61b-44d9-88ed-5944d1962f5e` |
| `redirect_uri` | `{base_url}/api/admin/anthropic/callback` |
| `scope` | `user:inference user:profile user:sessions:claude_code` |
| `response_type` | `code` |
| `code_challenge_method` | `S256` |

**Critical:** The `user:sessions:claude_code` scope is required to access Sonnet and Opus models. Without it, only Haiku-tier models are available.

### Account ID Generation

When adding a new account, generate a unique ID:
- Format: `claude-max-{timestamp}` or `claude-max-{uuid-short}`
- Must be unique within the credential file

---

## 5. Proactive Token Refresh

### Background Task

`AnthropicPool::spawn_refresh_task()` spawns a tokio task that:

1. Runs every 5 minutes
2. For each account in the pool:
   - Get current credentials via `OAuthClient::current_credentials()`
   - If `expires` is within 15 minutes of now → refresh
   - Call `OAuthClient::get_access_token()` to trigger refresh
   - If refresh fails → mark account as `Disabled`
3. Log refresh activity at `debug` level

### Token Refresh Flow

```rust
// In OAuthClient::get_access_token() - already implemented
if creds.is_expired() {
    match refresh_token(creds.refresh.expose()).await? {
        ExchangeResult::Success { access, refresh, expires } => {
            // Update in-memory credentials
            // Persist to FileCredentialStore
        }
        ExchangeResult::Failed { error } => {
            return Err(CredentialError::RefreshFailed(error));
        }
    }
}
```

### Configuration

| Parameter | Default | Env Var |
|-----------|---------|---------|
| Refresh interval | 5 minutes | `LOOM_SERVER_ANTHROPIC_REFRESH_INTERVAL_SECS` |
| Refresh threshold | 15 minutes | `LOOM_SERVER_ANTHROPIC_REFRESH_THRESHOLD_SECS` |

---

## 6. API Endpoints

### List Accounts

```
GET /api/admin/anthropic/accounts
Authorization: Bearer {admin_token}

Response 200:
{
  "accounts": [
    {
      "id": "claude-max-1",
      "status": "available",
      "cooldown_remaining_secs": null,
      "last_error": null,
      "expires_at": "2026-01-01T12:00:00Z"
    },
    {
      "id": "claude-max-2", 
      "status": "cooling_down",
      "cooldown_remaining_secs": 3600,
      "last_error": "5-hour usage limit exceeded",
      "expires_at": "2026-01-01T11:30:00Z"
    }
  ],
  "summary": {
    "total": 2,
    "available": 1,
    "cooling_down": 1,
    "disabled": 0
  }
}
```

### Initiate OAuth Flow

```
POST /api/admin/anthropic/accounts
Authorization: Bearer {admin_token}
Content-Type: application/json

{
  "redirect_after": "/admin/anthropic-accounts"
}

Response 200:
{
  "redirect_url": "https://claude.ai/oauth/authorize?client_id=...&state=..."
}
```

### OAuth Callback

```
GET /api/admin/anthropic/callback?code={code}&state={state}

Response 302:
Location: /admin/anthropic-accounts?added=claude-max-1735689600

Response 400 (on error):
{
  "error": "oauth_failed",
  "message": "Token exchange failed: invalid_grant"
}
```

### Remove Account

```
DELETE /api/admin/anthropic/accounts/{account_id}
Authorization: Bearer {admin_token}

Response 200:
{
  "removed": "claude-max-1"
}

Response 404:
{
  "error": "not_found",
  "message": "Account claude-max-1 not found"
}
```

---

## 7. Pool Management Methods

### AnthropicPool API

```rust
impl AnthropicPool {
    /// Create a new pool from credential file.
    pub async fn new(
        credential_file: impl AsRef<Path>,
        provider_ids: Vec<String>,
        model: Option<String>,
        config: AnthropicPoolConfig,
    ) -> Result<Self, LlmError>;

    /// Create an empty pool (for dynamic account addition).
    pub fn empty(
        credential_file: impl AsRef<Path>,
        model: Option<String>,
        config: AnthropicPoolConfig,
    ) -> Self;

    /// Add an account to the pool dynamically.
    pub async fn add_account(
        &self,
        account_id: String,
        credentials: OAuthCredentials,
    ) -> Result<(), LlmError>;

    /// Remove an account from the pool.
    pub async fn remove_account(&self, account_id: &str) -> Result<(), LlmError>;

    /// Get list of account IDs.
    pub async fn account_ids(&self) -> Vec<String>;

    /// Spawn background token refresh task.
    /// Returns a JoinHandle that can be aborted on shutdown.
    pub fn spawn_refresh_task(
        self: Arc<Self>,
        interval: Duration,
        threshold: Duration,
    ) -> tokio::task::JoinHandle<()>;

    /// Get current pool status for health reporting.
    pub async fn pool_status(&self) -> PoolStatus;

    /// Get detailed account info including token expiration.
    pub async fn account_details(&self) -> Vec<AccountDetails>;
}

/// Extended account info for admin API.
pub struct AccountDetails {
    pub id: String,
    pub status: AccountHealthStatus,
    pub cooldown_remaining_secs: Option<u64>,
    pub last_error: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}
```

---

## 8. LlmService Integration

### Changes to LlmService

```rust
impl LlmService {
    /// Create service with pool management capabilities.
    pub async fn new(config: LlmServiceConfig) -> Result<Self, LlmServiceError> {
        // ... existing code ...
        
        // If OAuth pool configured, start refresh task
        if let Some(AnthropicClientWrapper::Pool(ref pool)) = anthropic_client {
            let refresh_handle = Arc::clone(pool).spawn_refresh_task(
                Duration::from_secs(300),  // 5 minutes
                Duration::from_secs(900),  // 15 minutes
            );
            // Store handle for graceful shutdown
        }
    }

    /// Add an Anthropic OAuth account to the pool.
    pub async fn add_anthropic_account(
        &self,
        account_id: String,
        credentials: OAuthCredentials,
    ) -> Result<(), LlmServiceError>;

    /// Remove an Anthropic OAuth account from the pool.
    pub async fn remove_anthropic_account(
        &self,
        account_id: &str,
    ) -> Result<(), LlmServiceError>;

    /// Get detailed account info for admin API.
    pub async fn anthropic_account_details(&self) -> Option<Vec<AccountDetails>>;
}
```

---

## 9. NixOS Module Configuration

### New Options

```nix
services.loom-server.anthropic = {
  # Existing
  enable = mkEnableOption "Anthropic Claude provider";
  apiKeyFile = mkOption { ... };  # For API key mode
  model = mkOption { ... };

  # New OAuth pool options
  oauthCredentialFile = mkOption {
    type = types.path;
    default = "/var/lib/loom-server/anthropic-credentials.json";
    description = "Path to OAuth credential store JSON file.";
  };

  oauthEnabled = mkOption {
    type = types.bool;
    default = false;
    description = ''
      Enable OAuth pool mode for Claude Max subscriptions.
      When enabled, accounts are managed via the admin web UI.
      Mutually exclusive with apiKeyFile.
    '';
  };

  refreshIntervalSecs = mkOption {
    type = types.int;
    default = 300;
    description = "Interval in seconds between proactive token refresh checks.";
  };

  refreshThresholdSecs = mkOption {
    type = types.int;
    default = 900;
    description = "Refresh tokens when they expire within this many seconds.";
  };
};
```

### Environment Variables

| NixOS Option | Environment Variable |
|--------------|---------------------|
| `oauthCredentialFile` | `LOOM_SERVER_ANTHROPIC_OAUTH_CREDENTIAL_FILE` |
| `oauthEnabled` | `LOOM_SERVER_ANTHROPIC_OAUTH_ENABLED` |
| `refreshIntervalSecs` | `LOOM_SERVER_ANTHROPIC_REFRESH_INTERVAL_SECS` |
| `refreshThresholdSecs` | `LOOM_SERVER_ANTHROPIC_REFRESH_THRESHOLD_SECS` |

### Priority

1. If `oauthEnabled = true` → OAuth pool mode (credential file may be empty initially)
2. Else if `apiKeyFile` is set → API key mode
3. Else → Anthropic not configured

---

## 10. Web UI

### Admin Page: `/admin/anthropic-accounts`

```
┌─────────────────────────────────────────────────────────────────┐
│  Claude Max Accounts                         [+ Add Account]    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 🟢 claude-max-1                                          │   │
│  │    Status: Available                                     │   │
│  │    Token expires: in 45 minutes                          │   │
│  │                                            [Remove]      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 🟡 claude-max-2                                          │   │
│  │    Status: Cooling down (1h 30m remaining)               │   │
│  │    Last error: 5-hour usage limit exceeded               │   │
│  │    Token expires: in 2 hours                             │   │
│  │                                            [Remove]      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 🔴 claude-max-3                                          │   │
│  │    Status: Disabled                                      │   │
│  │    Last error: Token refresh failed: invalid_grant       │   │
│  │                                            [Remove]      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  Summary: 1 available, 1 cooling, 1 disabled (3 total)         │
└─────────────────────────────────────────────────────────────────┘
```

### Status Indicators

| Status | Icon | Color |
|--------|------|-------|
| Available | 🟢 | Green |
| Cooling Down | 🟡 | Yellow/Warning |
| Disabled | 🔴 | Red/Error |

---

## 11. Implementation Checklist

### loom-llm-anthropic

- [ ] Add `AnthropicPool::empty()` constructor
- [ ] Add `AnthropicPool::add_account()` method
- [ ] Add `AnthropicPool::remove_account()` method
- [ ] Add `AnthropicPool::account_ids()` method
- [ ] Add `AnthropicPool::spawn_refresh_task()` method
- [ ] Add `AnthropicPool::account_details()` method
- [ ] Add `AccountDetails` struct with `expires_at` field
- [ ] Update `OAuthClient` to expose credentials for inspection

### loom-llm-service

- [ ] Update `LlmService::new()` to start refresh task
- [ ] Add `LlmService::add_anthropic_account()` method
- [ ] Add `LlmService::remove_anthropic_account()` method
- [ ] Add `LlmService::anthropic_account_details()` method
- [ ] Store refresh task handle for graceful shutdown

### loom-server

- [ ] Add `routes/admin_anthropic.rs` module
- [ ] Implement `GET /api/admin/anthropic/accounts`
- [ ] Implement `POST /api/admin/anthropic/accounts`
- [ ] Implement `DELETE /api/admin/anthropic/accounts/{id}`
- [ ] Implement `GET /api/admin/anthropic/callback`
- [ ] Add OAuth state storage for PKCE flow
- [ ] Register routes with `SystemRole::Admin` requirement
- [ ] Add OpenAPI documentation

### loom-web

- [ ] Add `/admin/anthropic-accounts` page
- [ ] Implement account list component with status badges
- [ ] Implement "Add Account" button with OAuth redirect
- [ ] Implement "Remove" button with confirmation
- [ ] Add to admin navigation

### infra/nixos-modules

- [ ] Add `oauthCredentialFile` option
- [ ] Add `oauthEnabled` option
- [ ] Add `refreshIntervalSecs` option
- [ ] Add `refreshThresholdSecs` option
- [ ] Update environment variable generation
- [ ] Add assertion for mutual exclusivity with `apiKeyFile`

---

## 12. Security Considerations

1. **Admin-only access**: All pool management endpoints require `SystemRole::Admin`
2. **OAuth state validation**: PKCE + state parameter prevents CSRF
3. **Token storage**: Credentials file has 0600 permissions
4. **No token exposure**: API never returns refresh/access tokens, only status
5. **Audit logging**: Log account add/remove operations at `info` level

---

## 13. Required HTTP Headers for OAuth

OAuth tokens from Claude Max subscriptions require specific headers to authenticate with the Anthropic API. These were identified by sniffing Claude CLI traffic with mitmproxy.

### Working Request Example

```bash
curl -X POST "https://api.anthropic.com/v1/messages" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "anthropic-version: 2023-06-01" \
  -H "anthropic-beta: oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27" \
  -H "anthropic-dangerous-direct-browser-access: true" \
  -H "user-agent: claude-cli/2.0.76 (external, sdk-cli)" \
  -H "content-type: application/json" \
  -d '{"model":"claude-haiku-4-5-20251001","max_tokens":10,"messages":[{"role":"user","content":"hi"}]}'
```

### Required Headers

| Header | Value | Notes |
|--------|-------|-------|
| `Authorization` | `Bearer {access_token}` | OAuth access token |
| `anthropic-version` | `2023-06-01` | API version |
| `anthropic-beta` | `oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27` | **Critical**: Must include `oauth-2025-04-20`. Do NOT include `claude-code-20250219` for `/v1/messages`. |
| `anthropic-dangerous-direct-browser-access` | `true` | Required for OAuth |
| `user-agent` | `claude-cli/2.0.76 (external, sdk-cli)` | Must match Claude CLI format |
| `content-type` | `application/json` | Standard JSON content type |

### Optional Headers (from Claude CLI)

These are sent by Claude CLI but not required for authentication:

| Header | Example Value |
|--------|---------------|
| `x-app` | `cli` |
| `x-stainless-arch` | `x64` |
| `x-stainless-lang` | `js` |
| `x-stainless-os` | `Linux` |
| `x-stainless-package-version` | `0.70.0` |
| `x-stainless-runtime` | `node` |
| `x-stainless-runtime-version` | `v24.3.0` |

### Common Errors

| Error Message | Cause |
|---------------|-------|
| `This credential is only authorized for use with Claude Code` | Using an OAuth-incompatible model (see model restrictions below) |
| `invalid_grant` | Refresh token expired or revoked |

### Model Access with OAuth Tokens

Model access depends on the OAuth scopes requested during authorization:

| Scope | Model Access |
|-------|--------------|
| `user:inference user:profile` | Haiku only |
| `user:inference user:profile user:sessions:claude_code` | All models (Haiku, Sonnet, Opus) |

**CRITICAL:** The `user:sessions:claude_code` scope is required to access Sonnet and Opus models. Without it, you'll get the error: "This credential is only authorized for use with Claude Code".

| Model | Requires `user:sessions:claude_code` |
|-------|-------------------------------------|
| `claude-haiku-4-5-20251001` | No |
| `claude-haiku-4-5` (alias) | No |
| `claude-3-5-haiku-20241022` | No |
| `claude-3-haiku-20240307` | No |
| `claude-sonnet-4-5-20250929` | Yes |
| `claude-sonnet-4-20250514` | Yes |
| `claude-3-7-sonnet-20250219` | Yes |
| `claude-3-5-sonnet-20241022` | Yes |
| `claude-opus-4-5-20251101` | Yes |
| `claude-opus-4-20250514` | Yes |
| `claude-3-opus-20240229` | Yes |

**How Claude CLI accesses Sonnet/Opus:**
The Claude CLI uses the OAuth scope `user:sessions:claude_code` which grants access to all model tiers. This scope was discovered by extracting strings from the Claude CLI binary.

**Default model for OAuth pool:** `claude-sonnet-4-20250514` (with correct scopes)

### Header Discovery via mitmproxy

To sniff Claude CLI traffic and verify headers:

```bash
# Start mitmproxy
sudo mitmdump --set flow_detail=4 -p 8888 > /tmp/mitm-output.txt 2>&1 &

# Run Claude CLI through proxy (with TLS bypass for Node)
NODE_TLS_REJECT_UNAUTHORIZED=0 \
HTTPS_PROXY=http://127.0.0.1:8888 \
HTTP_PROXY=http://127.0.0.1:8888 \
claude --print "hello"

# Check captured traffic
grep -A50 "POST.*v1/messages" /tmp/mitm-output.txt
```

---

## 14. Future Enhancements

- CLI command for account management (backup to web UI)
- Per-account usage metrics
- Prometheus metrics for pool health
- Automatic re-authentication prompt when account is disabled
- Import/export credential file functionality
