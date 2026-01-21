# Anthropic OAuth Pool Specification

This document specifies the design for pooling multiple Claude Pro/Max OAuth subscriptions with automatic failover when quota is exhausted.

## Overview

Claude Pro/Max subscriptions have a 5-hour rolling usage limit. To maximize availability, Loom supports pooling multiple OAuth accounts and automatically failing over to the next available account when one hits its quota.

### Authentication Modes

| Mode | Use Case | Failover |
|------|----------|----------|
| **API Key** | Pay-per-use, single static key | None (unlimited quota) |
| **OAuth Pool** | 1+ Pro/Max subscriptions | Automatic on quota exhaustion |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            AnthropicPool                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │ Account 1       │  │ Account 2       │  │ Account 3       │              │
│  │ claude-max-1    │  │ claude-max-2    │  │ claude-max-3    │              │
│  │ ✓ Available     │  │ ⏳ Cooling Down │  │ ✓ Available     │              │
│  │                 │  │ (1h 30m left)   │  │                 │              │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘              │
│           │                                         │                        │
│           └──────────────┬──────────────────────────┘                        │
│                          ▼                                                   │
│                 ┌─────────────────┐                                          │
│                 │ Account Selector│                                          │
│                 │ (Round Robin)   │                                          │
│                 └─────────────────┘                                          │
│                          │                                                   │
│                          ▼                                                   │
│                 ┌─────────────────┐                                          │
│                 │  LlmClient      │ ◄── implements trait                     │
│                 │  complete()     │                                          │
│                 └─────────────────┘                                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Credential Storage

Multiple OAuth accounts are stored in a single credential file, keyed by provider ID:

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
  },
  "claude-max-3": {
    "type": "oauth",
    "refresh": "rt_ghi789...",
    "access": "at_rst111...",
    "expires": 1735500200000
  }
}
```

## Configuration

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `LOOM_SERVER_ANTHROPIC_API_KEY` | Static API key (mutually exclusive with OAuth) | `sk-ant-api03-...` |
| `LOOM_SERVER_ANTHROPIC_OAUTH_CREDENTIAL_FILE` | Path to credential JSON file | `~/.config/loom/credentials.json` |
| `LOOM_SERVER_ANTHROPIC_OAUTH_PROVIDERS` | Comma-separated provider IDs | `claude-max-1,claude-max-2,claude-max-3` |
| `LOOM_SERVER_ANTHROPIC_POOL_COOLDOWN_SECS` | Cooldown duration (default: 7200 = 2h) | `3600` |
| `LOOM_SERVER_ANTHROPIC_MODEL` | Model override (optional) | `claude-sonnet-4-20250514` |

### Priority

1. If `LOOM_SERVER_ANTHROPIC_OAUTH_PROVIDERS` is set → **OAuth Pool mode**
2. Else if `LOOM_SERVER_ANTHROPIC_API_KEY` is set → **API Key mode**
3. Else → Anthropic not configured

### Configuration Types

```rust
/// Anthropic authentication configuration
pub enum AnthropicAuthConfig {
    /// Static API key (pay-per-use)
    ApiKey(SecretString),
    
    /// OAuth pool with 1+ Pro/Max subscriptions
    OAuthPool {
        /// Path to credential file
        credential_file: PathBuf,
        /// Provider IDs to load from credential file
        provider_ids: Vec<String>,
        /// Cooldown duration when quota exhausted (default: 2 hours)
        cooldown_secs: u64,
    },
}
```

## Pool Implementation

### Account State

```rust
/// Runtime state for each account in the pool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    /// Account is ready to use
    Available,
    /// Account hit quota, cooling down until specified instant
    CoolingDown { until: Instant },
    /// Account permanently disabled (e.g., invalid credentials)
    Disabled,
}

/// Runtime tracking for an account
struct AccountRuntime {
    status: AccountStatus,
    last_used: Option<Instant>,
    last_error: Option<String>,
}
```

### Error Classification

Errors from Anthropic API are classified to determine failover behavior:

```rust
#[derive(Debug, Clone, Copy)]
pub enum ClientErrorKind {
    /// Transient error, retry on same account (via loom_http)
    Transient,
    /// Quota exhausted, failover to next account
    QuotaExceeded,
    /// Permanent error (bad credentials), disable account
    Permanent,
}
```

#### Detection Logic

| HTTP Status | Error Pattern | Classification |
|-------------|---------------|----------------|
| 429 | Message contains "5-hour", "rolling window", "usage limit" | `QuotaExceeded` |
| 429 | Other rate limit messages | `Transient` (retry with backoff) |
| 401, 403 | Any | `Permanent` |
| 408, 500, 502, 503, 504 | Any | `Transient` |

```rust
fn classify_error(status: u16, message: &str) -> ClientErrorKind {
    if status == 401 || status == 403 {
        return ClientErrorKind::Permanent;
    }
    
    if status == 429 && is_quota_message(message) {
        return ClientErrorKind::QuotaExceeded;
    }
    
    if matches!(status, 408 | 429 | 500 | 502 | 503 | 504) {
        return ClientErrorKind::Transient;
    }
    
    ClientErrorKind::Permanent
}

fn is_quota_message(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("5-hour") ||
    lower.contains("5 hour") ||
    lower.contains("rolling window") ||
    lower.contains("usage limit for your plan") ||
    lower.contains("subscription usage limit")
}
```

### Account Selection

```rust
/// Strategy for selecting which account to use
#[derive(Debug, Clone, Copy, Default)]
pub enum AccountSelectionStrategy {
    /// Round-robin through available accounts
    #[default]
    RoundRobin,
    /// Always try first available account
    FirstAvailable,
}
```

**Round-Robin Algorithm:**
1. Start from `next_index`
2. Scan up to N accounts looking for `Available` status
3. First `CoolingDown` account with expired cooldown is changed to `Available`
4. Select first `Available` account found, increment `next_index`
5. If none found, return error (all exhausted)

### Pool Interface

```rust
/// Configuration for the account pool
pub struct AnthropicPoolConfig {
    /// How long to cool down an account after quota exhaustion
    pub cooldown: Duration,
    /// Account selection strategy
    pub strategy: AccountSelectionStrategy,
}

impl Default for AnthropicPoolConfig {
    fn default() -> Self {
        Self {
            cooldown: Duration::from_secs(2 * 60 * 60), // 2 hours
            strategy: AccountSelectionStrategy::RoundRobin,
        }
    }
}

/// Pool of Anthropic OAuth accounts with automatic failover
pub struct AnthropicPool {
    accounts: Vec<AccountEntry>,
    state: Mutex<PoolState>,
    config: AnthropicPoolConfig,
}

impl AnthropicPool {
    /// Create a new pool from OAuth credentials
    pub async fn new(
        credential_file: impl AsRef<Path>,
        provider_ids: Vec<String>,
        model: Option<String>,
        config: AnthropicPoolConfig,
    ) -> Result<Self, LlmError>;
    
    /// Get current pool status for health reporting
    pub async fn pool_status(&self) -> PoolStatus;
}

#[async_trait]
impl LlmClient for AnthropicPool {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
    async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
}
```

### Failover Flow

```
┌──────────────────┐
│ Incoming Request │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐     No accounts
│ Select Account   │────────────────────► Return Error
│ (Round Robin)    │                      "Pool exhausted"
└────────┬─────────┘
         │ Found available
         ▼
┌──────────────────┐
│ Send Request to  │
│ Selected Account │
└────────┬─────────┘
         │
    ┌────┴────┐
    │         │
Success    Error
    │         │
    ▼         ▼
┌────────┐ ┌──────────────────┐
│ Return │ │ Classify Error   │
│ Result │ └────────┬─────────┘
└────────┘          │
           ┌────────┼────────┐
           │        │        │
      Transient  Quota   Permanent
           │     Exceeded    │
           │        │        │
           ▼        ▼        ▼
      ┌────────┐ ┌────────┐ ┌────────┐
      │ Retry  │ │ Mark   │ │ Mark   │
      │ (same) │ │Cooling │ │Disabled│
      └────────┘ └───┬────┘ └────────┘
                     │
                     ▼
              Return Error
              (caller may retry
               with new account)
```

## Health Integration

### Pool Status Types

```rust
/// Status of an individual account for health reporting
#[derive(Debug, Clone, Serialize)]
pub struct AccountHealthInfo {
    pub id: String,
    pub status: AccountHealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_remaining_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountHealthStatus {
    Available,
    CoolingDown,
    Disabled,
}

/// Overall pool status for health reporting
#[derive(Debug, Clone, Serialize)]
pub struct PoolStatus {
    pub accounts_total: usize,
    pub accounts_available: usize,
    pub accounts_cooling: usize,
    pub accounts_disabled: usize,
    pub accounts: Vec<AccountHealthInfo>,
}
```

### LlmService Health Method

```rust
impl LlmService {
    /// Get Anthropic health status
    pub async fn anthropic_health(&self) -> Option<AnthropicHealthInfo> {
        match &self.anthropic_client {
            Some(AnthropicClientWrapper::ApiKey(_)) => {
                Some(AnthropicHealthInfo::ApiKey { configured: true })
            }
            Some(AnthropicClientWrapper::Pool(pool)) => {
                Some(AnthropicHealthInfo::Pool(pool.pool_status().await))
            }
            None => None,
        }
    }
}

pub enum AnthropicHealthInfo {
    ApiKey { configured: bool },
    Pool(PoolStatus),
}
```

### Health Endpoint Response

**API Key mode:**
```json
{
  "components": {
    "llm_providers": {
      "status": "healthy",
      "providers": [{
        "name": "anthropic",
        "status": "healthy",
        "mode": "api_key"
      }]
    }
  }
}
```

**Pool mode (healthy):**
```json
{
  "components": {
    "llm_providers": {
      "status": "healthy",
      "providers": [{
        "name": "anthropic",
        "status": "healthy",
        "mode": "oauth_pool",
        "pool": {
          "accounts_total": 3,
          "accounts_available": 3,
          "accounts_cooling": 0,
          "accounts_disabled": 0,
          "accounts": [
            { "id": "claude-max-1", "status": "available" },
            { "id": "claude-max-2", "status": "available" },
            { "id": "claude-max-3", "status": "available" }
          ]
        }
      }]
    }
  }
}
```

**Pool mode (degraded):**
```json
{
  "components": {
    "llm_providers": {
      "status": "degraded",
      "providers": [{
        "name": "anthropic",
        "status": "degraded",
        "mode": "oauth_pool",
        "pool": {
          "accounts_total": 3,
          "accounts_available": 1,
          "accounts_cooling": 2,
          "accounts_disabled": 0,
          "accounts": [
            { "id": "claude-max-1", "status": "available" },
            { "id": "claude-max-2", "status": "cooling_down", "cooldown_remaining_secs": 5400 },
            { "id": "claude-max-3", "status": "cooling_down", "cooldown_remaining_secs": 3600, "last_error": "Usage limit exceeded" }
          ]
        }
      }]
    }
  }
}
```

### Health Status Mapping

| Pool State | Provider Status | Overall LLM Status |
|------------|-----------------|-------------------|
| All available | `Healthy` | `Healthy` |
| ≥1 available, some cooling/disabled | `Degraded` | `Degraded` |
| All cooling or disabled | `Unhealthy` | `Unhealthy` |

## Implementation Checklist

### loom-llm-anthropic

- [ ] Add `ClientErrorKind` enum to `client.rs`
- [ ] Update `send_request` to classify errors and detect quota exhaustion
- [ ] Create `pool.rs` with `AnthropicPool` struct
- [ ] Implement `LlmClient` trait for `AnthropicPool`
- [ ] Add `pool_status()` method for health reporting
- [ ] Export pool types from `lib.rs`

### loom-llm-service

- [ ] Update `AnthropicAuthConfig` to `ApiKey | OAuthPool`
- [ ] Update `AnthropicClientWrapper` to `ApiKey | Pool`
- [ ] Add env var parsing for `OAUTH_PROVIDERS` and `POOL_COOLDOWN_SECS`
- [ ] Update `LlmService::new()` to instantiate pool when configured
- [ ] Add `anthropic_health()` method

### loom-server

- [ ] Add `AnthropicPoolHealth` struct to `health.rs`
- [ ] Update `LlmProviderHealth` to include optional `pool` field and `mode`
- [ ] Update `check_llm_providers()` to call `anthropic_health()`
- [ ] Register new types in `api_docs.rs`

## Security Considerations

1. **Credential isolation**: Each account's OAuth tokens are independently managed
2. **No credential leakage**: Health endpoint shows account IDs but never tokens
3. **Graceful degradation**: Service continues with reduced capacity when accounts are cooling
4. **Audit trail**: Log which account was used for each request (at debug level)

## Troubleshooting

### "This credential is only authorized for use with Claude Code" Error

This error occurs when using OAuth tokens with premium models (Opus, Sonnet) without the required system prompt prefix.

#### Root Cause

Anthropic validates OAuth requests server-side to ensure they come from legitimate coding assistant tools. The validation is **content-based**, checking the system prompt in the request body.

**Key Discovery**: The restriction is NOT based on headers, User-Agent, or network fingerprinting. It's based on the **system prompt content**.

Reference: [Anthropic MAX Plan Implementation Guide](https://raw.githubusercontent.com/nsxdavid/anthropic-max-router/main/ANTHROPIC-MAX-PLAN-IMPLEMENTATION-GUIDE.md)

#### Required System Prompt Prefix

The system prompt **MUST** start with this exact phrase (case-sensitive, punctuation-sensitive):

```
You are Claude Code, Anthropic's official CLI for Claude.
```

**Requirements:**
- Must be the **FIRST** content in the system prompt
- Exact capitalization (case-sensitive)
- Include the period at the end
- Additional system content can be appended AFTER this phrase

**What works:**
```json
{
  "system": "You are Claude Code, Anthropic's official CLI for Claude. You are also a helpful coding assistant."
}
```

**What fails:**
- ❌ No system prompt at all
- ❌ Phrase appears after other content: `"You are a helpful assistant. You are Claude Code..."`
- ❌ Case variations: `"you are claude code..."`
- ❌ Missing period: `"You are Claude Code, Anthropic's official CLI for Claude"`
- ❌ Shortened version: `"You are Claude Code."`

#### Model-Specific Behavior

| Model | System Prompt Required? |
|-------|------------------------|
| `claude-opus-4-5-*` | ✅ Yes |
| `claude-sonnet-4-*` | ✅ Yes |
| `claude-haiku-*` | ❌ No (works without prefix) |

#### Implementation

Loom automatically prepends the required system prompt when using OAuth authentication. See `AnthropicRequest::with_oauth_system_prompt()` in `crates/loom-server-llm-anthropic/src/types.rs`.

#### Required Headers for OAuth

In addition to the system prompt, the following headers must be sent:

```
Authorization: Bearer <access_token>
anthropic-beta: oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27
anthropic-dangerous-direct-browser-access: true
User-Agent: claude-cli/2.0.76 (external, sdk-cli)
```

**Note**: The `claude-code-20250219` beta header is NOT required for `/v1/messages` requests. It's only used for internal telemetry endpoints.

#### Sniffing Claude CLI Traffic with mitmproxy

To identify what headers the official Claude CLI sends:

1. Install mitmproxy:
   ```bash
   nix-env -iA nixos.mitmproxy
   ```

2. Start mitmproxy in dump mode:
   ```bash
   mitmdump --set flow_detail=4 -p 8888 2>&1 &
   ```

3. Configure environment for Claude CLI to use the proxy:
   ```bash
   export HTTPS_PROXY=http://127.0.0.1:8888
   export HTTP_PROXY=http://127.0.0.1:8888
   export SSL_CERT_FILE=~/.mitmproxy/mitmproxy-ca-cert.pem
   export REQUESTS_CA_BUNDLE=~/.mitmproxy/mitmproxy-ca-cert.pem
   export NODE_EXTRA_CA_CERTS=~/.mitmproxy/mitmproxy-ca-cert.pem
   ```

4. Run a Claude CLI command:
   ```bash
   claude --print "hello"
   ```

5. Observe the headers in mitmproxy output, particularly for `POST https://api.anthropic.com/v1/messages`

#### Key Observations from Traffic Analysis

The Claude CLI (v2.0.76) sends these specific headers for `/v1/messages`:

| Header | Value |
|--------|-------|
| `anthropic-beta` | `oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27` |
| `anthropic-dangerous-direct-browser-access` | `true` |
| `user-agent` | `claude-cli/2.0.76 (external, sdk-cli)` |
| `x-app` | `cli` |

Note: The `claude-code-20250219` beta header is used for internal telemetry (`/api/event_logging/batch`) but **NOT** for `/v1/messages` requests.

### Debugging Header Mismatches

If you encounter authentication errors:

1. Check the actual headers being sent by adding debug logging
2. Compare against sniffed Claude CLI traffic
3. Ensure `OAUTH_COMBINED_BETA_HEADERS` constant matches current Claude CLI behavior
4. Verify User-Agent matches the format: `claude-cli/<version> (external, sdk-cli)`

### Updating Headers When Claude CLI Changes

When Anthropic updates Claude CLI, the beta headers may change:

1. Install the new Claude CLI version
2. Sniff traffic as described above
3. Update constants in `crates/loom-server-llm-anthropic/src/auth/scheme.rs`:
   - `OAUTH_COMBINED_BETA_HEADERS`
   - `ANTHROPIC_USER_AGENT`
   - `OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX`
4. Run tests to verify no regressions

## Future Enhancements

- Parse `retry-after` header for more precise cooldown timing
- Add metrics (Prometheus) for per-account usage and failover events
- Support mixed API key + OAuth pool configuration
- Dynamic pool management (add/remove accounts at runtime)
- Per-account rate limiting to prevent one account from being exhausted first
