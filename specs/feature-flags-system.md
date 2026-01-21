<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Feature Flags & Experiments System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-10

---

## 1. Overview

### Purpose

This specification defines a feature flags and experiments system for Loom. The system enables controlled rollouts, A/B experiments, and emergency kill switches with real-time updates via SSE. It follows patterns from LaunchDarkly and Unleash.

### Goals

- **Per-organization feature flags** with environment-scoped configuration (dev, staging, prod)
- **Platform-level flags** for global features (super admin managed, override org flags)
- **Rollout strategies** with percentage, attribute, geographic, and environment targeting
- **Scheduling support** for gradual rollouts over time
- **Kill switches** for emergency feature disablement with flag linking
- **Multi-variant experiments** with exposure tracking for analytics
- **Real-time updates** via SSE streaming to connected clients
- **SDK libraries** for Rust and TypeScript with offline caching
- **Full audit logging** of all flag, strategy, and kill switch changes

### Non-Goals

- Statistical analysis / experiment metrics computation — exposure data exported for external analytics
- Webhooks for flag changes — deferred to future version
- Rate limiting on evaluation endpoints — deferred to future version
- Code reference scanning — deferred to future version
- Strategy nesting/composition — single strategy per flag for v1

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-flags-core/                # Shared types for flags, strategies, kill switches
│   ├── src/
│   │   ├── lib.rs
│   │   ├── flag.rs                 # Flag, Variant, FlagConfig
│   │   ├── strategy.rs             # Strategy, Condition, Schedule
│   │   ├── kill_switch.rs          # KillSwitch
│   │   ├── environment.rs          # Environment
│   │   ├── sdk_key.rs              # SdkKey, SdkKeyType
│   │   ├── evaluation.rs           # EvaluationContext, EvaluationResult
│   │   └── error.rs                # Error types
│   └── Cargo.toml
├── loom-flags/                     # Rust SDK client
│   ├── src/
│   │   ├── lib.rs
│   │   ├── client.rs               # FlagsClient
│   │   ├── cache.rs                # Local flag cache
│   │   ├── sse.rs                  # SSE connection for updates
│   │   └── evaluation.rs           # Local evaluation helpers
│   └── Cargo.toml
├── loom-server-flags/              # Server-side API and evaluation
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs               # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── flags.rs            # Flag CRUD
│   │   │   ├── strategies.rs       # Strategy CRUD
│   │   │   ├── kill_switches.rs    # Kill switch management
│   │   │   ├── environments.rs     # Environment management
│   │   │   ├── sdk_keys.rs         # SDK key management
│   │   │   └── evaluate.rs         # Flag evaluation endpoint
│   │   ├── repository.rs           # Database repository
│   │   ├── evaluation.rs           # Server-side evaluation engine
│   │   ├── sse.rs                  # SSE streaming
│   │   └── geoip.rs                # GeoIP resolution with proxy support
│   └── Cargo.toml

web/
├── packages/
│   ├── http/                       # @loom/http - TypeScript HTTP client
│   │   ├── src/
│   │   │   ├── index.ts
│   │   │   ├── client.ts           # HTTP client with retry
│   │   │   └── headers.ts          # Standard headers
│   │   └── package.json
│   └── flags/                      # @loom/flags - TypeScript SDK
│       ├── src/
│       │   ├── index.ts
│       │   ├── client.ts           # FlagsClient
│       │   ├── cache.ts            # Local cache
│       │   └── sse.ts              # SSE connection
│       └── package.json
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Clients                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│    loom-cli              loom-web (Browser)          Other Services          │
│    (loom-flags)          (@loom/flags)               (loom-flags)            │
│    Server-side key       Client-side key             Server-side key         │
└────────┬─────────────────────────┬──────────────────────────┬───────────────┘
         │                         │                          │
         └─────────────────────────┼──────────────────────────┘
                                   │
                                   ▼ SSE + REST
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │  Flag Routes    │  │ SDK Key Auth    │  │ Evaluation      │              │
│  │  /api/flags/*   │  │ Middleware      │  │ Engine          │              │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘              │
│           │                    │                    │                        │
│           └────────────────────┼────────────────────┘                        │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                         SSE Broadcaster                               │   │
│  │    Real-time flag updates to all connected clients per environment   │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                │                                             │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                         GeoIP Resolver                                │   │
│  │    Resolves client IP (with CF-Connecting-IP, X-Forwarded-For, etc.) │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  flags    flag_configs    strategies    kill_switches    sdk_keys           │
│  environments    flag_prerequisites    exposure_logs    audit_logs          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Entities

### 3.1 Flag

A feature flag with multi-variant support and per-environment configuration.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
    pub id: FlagId,
    pub org_id: Option<OrgId>,       // None = platform-level flag
    pub key: String,                  // Structured key: "checkout.new_flow"
    pub name: String,                 // Human-readable name
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub maintainer_user_id: Option<UserId>,
    pub variants: Vec<Variant>,
    pub default_variant: String,      // Variant name for fallback
    pub prerequisites: Vec<FlagPrerequisite>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub name: String,                 // e.g., "control", "treatment_a"
    pub value: VariantValue,
    pub weight: u32,                  // For percentage-based distribution
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum VariantValue {
    Boolean(bool),
    String(String),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagPrerequisite {
    pub flag_key: String,
    pub required_variant: String,     // Prerequisite flag must be this variant
}
```

### 3.2 FlagConfig

Per-environment configuration for a flag.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagConfig {
    pub id: FlagConfigId,
    pub flag_id: FlagId,
    pub environment_id: EnvironmentId,
    pub enabled: bool,
    pub strategy_id: Option<StrategyId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 3.3 Strategy

Reusable rollout strategy with targeting conditions.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: StrategyId,
    pub org_id: Option<OrgId>,        // None = platform-level strategy
    pub name: String,
    pub description: Option<String>,
    pub conditions: Vec<Condition>,   // All conditions must match (AND)
    pub percentage: Option<u32>,      // 0-100, applied after conditions
    pub percentage_key: PercentageKey,
    pub schedule: Option<Schedule>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Condition {
    Attribute {
        attribute: String,            // e.g., "plan", "created_at"
        operator: AttributeOperator,
        value: serde_json::Value,
    },
    Geographic {
        field: GeoField,              // Country, Region, City
        operator: GeoOperator,
        values: Vec<String>,          // e.g., ["US", "CA", "GB"]
    },
    Environment {
        environments: Vec<String>,    // e.g., ["prod", "staging"]
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttributeOperator {
    Equals,
    NotEquals,
    Contains,
    StartsWith,
    EndsWith,
    GreaterThan,
    LessThan,
    GreaterThanOrEquals,
    LessThanOrEquals,
    In,
    NotIn,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GeoField {
    Country,
    Region,
    City,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum GeoOperator {
    In,
    NotIn,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PercentageKey {
    UserId,
    OrgId,
    SessionId,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub steps: Vec<ScheduleStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleStep {
    pub percentage: u32,
    pub start_at: DateTime<Utc>,
}
```

### 3.4 Kill Switch

Emergency shutoff mechanism that overrides linked flags.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitch {
    pub id: KillSwitchId,
    pub org_id: Option<OrgId>,        // None = platform-level
    pub key: String,                  // e.g., "disable_checkout"
    pub name: String,
    pub description: Option<String>,
    pub linked_flag_keys: Vec<String>,
    pub is_active: bool,
    pub activated_at: Option<DateTime<Utc>>,
    pub activated_by: Option<UserId>,
    pub activation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 3.5 Environment

Deployment environment with its own SDK keys and flag configs.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: EnvironmentId,
    pub org_id: OrgId,
    pub name: String,                 // e.g., "dev", "staging", "prod"
    pub color: Option<String>,        // For UI display
    pub created_at: DateTime<Utc>,
}
```

### 3.6 SDK Key

Authentication key for SDK clients.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkKey {
    pub id: SdkKeyId,
    pub environment_id: EnvironmentId,
    pub key_type: SdkKeyType,
    pub name: String,
    pub key_hash: String,             // Argon2 hash
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SdkKeyType {
    ClientSide,   // Safe for browser, single user context
    ServerSide,   // Secret, backend only, any user context
}
```

### 3.7 Evaluation Context

Context passed by SDK for flag evaluation.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationContext {
    pub user_id: Option<String>,
    pub org_id: Option<String>,
    pub environment: String,
    pub attributes: HashMap<String, serde_json::Value>,
    // GeoIP resolved server-side from request IP
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    pub flag_key: String,
    pub variant: String,
    pub value: VariantValue,
    pub reason: EvaluationReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvaluationReason {
    Default,                          // No strategy, default variant
    Strategy { strategy_id: StrategyId },
    KillSwitch { kill_switch_id: KillSwitchId },
    Prerequisite { missing_flag: String },
    Disabled,                         // Flag disabled in this environment
    Error { message: String },
}
```

---

## 4. Scoping & Precedence

### 4.1 Two-Tier Flag System

| Scope | Managed By | Use Case |
|-------|------------|----------|
| **Platform** | Super admins | Global features, maintenance mode, platform-wide rollouts |
| **Organization** | Org admins | Org-specific features, team experiments |

### 4.2 Precedence Rules

1. **Platform flags override org flags** — If a platform flag exists for a key, org config is ignored
2. **Kill switches override flags** — Active kill switch forces linked flags to off/default
3. **Platform kill switches override org flags** — Platform kill switch affects all orgs
4. **Prerequisites evaluated first** — Missing prerequisite returns default variant

### 4.3 Flag Key Format

Structured dot-notation: `{domain}.{feature}[.{sub}]`

Examples:
- `checkout.new_flow`
- `billing.subscription.annual`
- `ai.model.gpt4`

Validation:
- Lowercase alphanumeric with dots and underscores
- 3-100 characters
- Cannot start or end with dot
- Pattern: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`

---

## 5. Environments

### 5.1 Auto-Created Environments

When an organization is created, two environments are automatically provisioned:

| Environment | Description |
|-------------|-------------|
| `dev` | Development environment |
| `prod` | Production environment |

Org admins can create additional environments (e.g., `staging`, `qa`).

### 5.2 Environment-Scoped Configuration

- Each flag has separate config per environment
- When a flag is created, config is auto-created for all existing environments
- SDK keys are scoped to a single environment
- SSE streams are environment-specific

---

## 6. SDK Keys

### 6.1 Key Types

| Type | Prefix | Use Case | Capabilities |
|------|--------|----------|--------------|
| Client-side | `loom_sdk_client_` | Browser, mobile apps | Evaluate for single user context |
| Server-side | `loom_sdk_server_` | Backend services | Evaluate for any user context |

### 6.2 Key Format

```
loom_sdk_{type}_{env}_{random}

Example:
loom_sdk_server_prod_7a3b9f2e1c4d8a5b6e0f3c2d1a4b5c6d7e8f9a0b
```

### 6.3 Key Management

- Org admins create/revoke SDK keys
- Keys are stored hashed (Argon2)
- Shown once at creation
- One or more keys per environment allowed
- Last used timestamp tracked

---

## 7. Kill Switches

### 7.1 Design Principles

| Aspect | Behavior |
|--------|----------|
| **Targeting** | Global on/off only — no strategies |
| **Linked flags** | When activated, forces all linked flags to off/default |
| **Activation reason** | Required field when activating |
| **Reset** | Manual only — no auto-reset |
| **Permissions** | Separate `killswitch:activate` permission |
| **Priority** | Evaluated before flag strategies |

### 7.2 Activation Flow

```
1. User with killswitch:activate permission activates kill switch
2. Required: activation_reason field
3. Kill switch marked active with timestamp and user
4. All linked flags immediately evaluate to default/off variant
5. SSE broadcast to all connected clients
6. Audit log entry created
```

### 7.3 Deactivation Flow

```
1. User with killswitch:activate permission deactivates kill switch
2. Kill switch marked inactive
3. Linked flags resume normal evaluation
4. SSE broadcast to all connected clients
5. Audit log entry created
```

---

## 8. Evaluation Engine

### 8.1 Evaluation Order

```
1. Check if flag exists
   → Not found: return error or SDK-provided default

2. Check environment config
   → Disabled: return default variant with reason=Disabled

3. Check kill switches (platform first, then org)
   → Active kill switch links this flag: return default with reason=KillSwitch

4. Check prerequisites
   → Prerequisite flag not enabled/wrong variant: return default with reason=Prerequisite

5. Check strategy (if configured)
   → Evaluate conditions (all must match)
   → Apply percentage (if configured)
   → Apply schedule (if configured)
   → Return variant based on strategy

6. No strategy or conditions not met
   → Return default variant with reason=Default
```

### 8.2 Percentage Hashing

Consistent hashing for sticky assignment:

```rust
fn evaluate_percentage(key: &str, flag_key: &str, percentage: u32) -> bool {
    let input = format!("{}.{}", flag_key, key);
    let hash = murmur3_32(&input, 0);
    let bucket = hash % 100;
    bucket < percentage
}
```

### 8.3 Schedule Evaluation

```rust
fn evaluate_schedule(schedule: &Schedule, now: DateTime<Utc>) -> u32 {
    let mut current_percentage = 0;
    for step in &schedule.steps {
        if now >= step.start_at {
            current_percentage = step.percentage;
        }
    }
    current_percentage
}
```

### 8.4 GeoIP Resolution

Server resolves GeoIP from client IP with proxy header support:

```rust
fn resolve_client_ip(headers: &HeaderMap, remote_addr: IpAddr) -> IpAddr {
    // Priority order:
    // 1. CF-Connecting-IP (Cloudflare)
    // 2. X-Real-IP
    // 3. X-Forwarded-For (first IP)
    // 4. Remote address
}
```

---

## 9. Exposure Tracking

### 9.1 Purpose

Track when users are exposed to flag variants for experiment analysis.

### 9.2 Exposure Log Entry

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureLog {
    pub id: ExposureLogId,
    pub flag_id: FlagId,
    pub flag_key: String,
    pub environment_id: EnvironmentId,
    pub user_id: Option<String>,
    pub org_id: Option<String>,
    pub variant: String,
    pub reason: EvaluationReason,
    pub context_hash: String,         // Hash of evaluation context for dedup
    pub timestamp: DateTime<Utc>,
}
```

### 9.3 Deduplication

- Hash evaluation context to avoid duplicate logs
- Only log first exposure per context hash per time window (1 hour)
- Configurable per flag (enable/disable exposure tracking)

### 9.4 Export

Exposure logs can be exported for external analytics:
- REST API: `GET /api/flags/exposures?flag_key=X&start=&end=`
- Bulk export: `POST /api/flags/exposures/export`

---

## 10. Stale Flag Detection

### 10.1 Staleness Criteria

A flag is considered stale if:
- Not evaluated in the last 30 days
- No strategy changes in the last 90 days

### 10.2 Tracking

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagStats {
    pub flag_id: FlagId,
    pub last_evaluated_at: Option<DateTime<Utc>>,
    pub evaluation_count_24h: u64,
    pub evaluation_count_7d: u64,
    pub evaluation_count_30d: u64,
}
```

### 10.3 API

`GET /api/flags/stale` — Returns list of stale flags with last evaluation timestamps.

---

## 11. API Endpoints

### 11.1 Base Path

All feature flag endpoints are under `/api/flags/*`.

### 11.2 Flag Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags` | List flags for org |
| POST | `/api/flags` | Create flag |
| GET | `/api/flags/{key}` | Get flag by key |
| PATCH | `/api/flags/{key}` | Update flag |
| DELETE | `/api/flags/{key}` | Archive flag |
| POST | `/api/flags/{key}/restore` | Restore archived flag |
| GET | `/api/flags/{key}/configs` | Get all environment configs |
| PATCH | `/api/flags/{key}/configs/{env}` | Update environment config |

### 11.3 Strategy Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags/strategies` | List strategies |
| POST | `/api/flags/strategies` | Create strategy |
| GET | `/api/flags/strategies/{id}` | Get strategy |
| PATCH | `/api/flags/strategies/{id}` | Update strategy |
| DELETE | `/api/flags/strategies/{id}` | Delete strategy |

### 11.4 Kill Switch Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags/kill-switches` | List kill switches |
| POST | `/api/flags/kill-switches` | Create kill switch |
| GET | `/api/flags/kill-switches/{key}` | Get kill switch |
| PATCH | `/api/flags/kill-switches/{key}` | Update kill switch |
| DELETE | `/api/flags/kill-switches/{key}` | Delete kill switch |
| POST | `/api/flags/kill-switches/{key}/activate` | Activate (requires reason) |
| POST | `/api/flags/kill-switches/{key}/deactivate` | Deactivate |

### 11.5 Environment Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags/environments` | List environments |
| POST | `/api/flags/environments` | Create environment |
| PATCH | `/api/flags/environments/{id}` | Update environment |
| DELETE | `/api/flags/environments/{id}` | Delete environment |

### 11.6 SDK Key Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags/sdk-keys` | List SDK keys |
| POST | `/api/flags/sdk-keys` | Create SDK key |
| DELETE | `/api/flags/sdk-keys/{id}` | Revoke SDK key |

### 11.7 Evaluation (Requires SDK Key Auth)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/flags/evaluate` | Evaluate all flags for context |
| POST | `/api/flags/evaluate/{key}` | Evaluate single flag |
| GET | `/api/flags/stream` | SSE stream for flag updates |

### 11.8 Exposure & Stats (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/flags/exposures` | Query exposure logs |
| POST | `/api/flags/exposures/export` | Bulk export exposures |
| GET | `/api/flags/stale` | List stale flags |
| GET | `/api/flags/{key}/stats` | Get flag statistics |

### 11.9 Platform Flags (Requires Super Admin)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/admin/flags` | List platform flags |
| POST | `/api/admin/flags` | Create platform flag |
| PATCH | `/api/admin/flags/{key}` | Update platform flag |
| DELETE | `/api/admin/flags/{key}` | Archive platform flag |
| GET | `/api/admin/flags/kill-switches` | List platform kill switches |
| POST | `/api/admin/flags/kill-switches` | Create platform kill switch |

---

## 12. SSE Streaming

### 12.1 Connection

```
GET /api/flags/stream
Authorization: Bearer loom_sdk_server_prod_xxx
```

### 12.2 Events

| Event | Description |
|-------|-------------|
| `init` | Full state of all flags on connect |
| `flag.updated` | Flag or config changed |
| `flag.archived` | Flag archived |
| `killswitch.activated` | Kill switch activated |
| `killswitch.deactivated` | Kill switch deactivated |
| `heartbeat` | Keep-alive (every 30s) |

### 12.3 Event Format

```json
{
  "event": "flag.updated",
  "data": {
    "flag_key": "checkout.new_flow",
    "environment": "prod",
    "enabled": true,
    "timestamp": "2026-01-10T12:00:00Z"
  }
}
```

### 12.4 Reconnection

- SDK should reconnect with exponential backoff
- On reconnect, server sends `init` event with full state

---

## 13. SDK Design

### 13.1 Rust SDK (`loom-flags`)

```rust
use loom_flags::{FlagsClient, EvaluationContext};

// Initialize client with SDK key
let client = FlagsClient::builder()
    .sdk_key("loom_sdk_server_prod_xxx")
    .base_url("https://loom.example.com")
    .build()
    .await?;

// Evaluate flag with caller-provided default
let enabled = client.get_bool(
    "checkout.new_flow",
    &context,
    Some(false),  // Override default if flag missing/error
).await?;

// Evaluate with configured default
let variant = client.get_string(
    "checkout.theme",
    &context,
    None,  // Use flag's configured default
).await?;

// Get all flags
let all_flags = client.get_all(&context).await?;
```

### 13.2 TypeScript SDK (`@loom/flags`)

```typescript
import { FlagsClient } from '@loom/flags';

const client = new FlagsClient({
  sdkKey: 'loom_sdk_client_prod_xxx',
  baseUrl: 'https://loom.example.com',
});

await client.initialize();

// Evaluate flag
const enabled = await client.getBool('checkout.new_flow', context, false);
const variant = await client.getString('checkout.theme', context);

// React to updates
client.on('flag.updated', (event) => {
  console.log(`Flag ${event.flagKey} updated`);
});
```

### 13.3 SDK Behavior

| Aspect | Behavior |
|--------|----------|
| **Initialization** | Fetch all flags, start SSE connection |
| **Caching** | All flags cached locally |
| **Updates** | SSE pushes updates, cache refreshed |
| **Offline** | Use last cached values |
| **Defaults** | Flag config default, caller can override |
| **Reconnection** | Exponential backoff on disconnect |

### 13.4 HTTP Client Requirements

Both SDKs use shared HTTP client libraries:
- Rust: `loom-http` with retry, User-Agent
- TypeScript: `@loom/http` with retry, User-Agent

---

## 14. Configuration

### 14.1 Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_FLAGS_ENABLED` | boolean | Enable feature flags system | `true` |
| `LOOM_FLAGS_SSE_HEARTBEAT_INTERVAL` | duration | SSE heartbeat interval | `30s` |
| `LOOM_FLAGS_EXPOSURE_DEDUP_WINDOW` | duration | Exposure log dedup window | `1h` |
| `LOOM_FLAGS_STALE_THRESHOLD_DAYS` | integer | Days without evaluation for stale | `30` |

---

## 15. Database Schema

### 15.1 Migration: `XXX_feature_flags.sql`

```sql
-- Environments
CREATE TABLE flag_environments (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    color TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(org_id, name)
);

-- Flags
CREATE TABLE flags (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform flag
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    tags TEXT,  -- JSON array
    maintainer_user_id TEXT REFERENCES users(id),
    variants TEXT NOT NULL,  -- JSON array
    default_variant TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived_at TEXT,
    UNIQUE(org_id, key)
);

CREATE INDEX idx_flags_org_id ON flags(org_id);
CREATE INDEX idx_flags_key ON flags(key);

-- Flag prerequisites
CREATE TABLE flag_prerequisites (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id) ON DELETE CASCADE,
    prerequisite_flag_key TEXT NOT NULL,
    required_variant TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_flag_prerequisites_flag_id ON flag_prerequisites(flag_id);

-- Flag configs (per environment)
CREATE TABLE flag_configs (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id) ON DELETE CASCADE,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL DEFAULT 0,
    strategy_id TEXT REFERENCES flag_strategies(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(flag_id, environment_id)
);

CREATE INDEX idx_flag_configs_flag_id ON flag_configs(flag_id);
CREATE INDEX idx_flag_configs_environment_id ON flag_configs(environment_id);

-- Strategies
CREATE TABLE flag_strategies (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform strategy
    name TEXT NOT NULL,
    description TEXT,
    conditions TEXT NOT NULL,  -- JSON array
    percentage INTEGER,
    percentage_key TEXT NOT NULL DEFAULT 'user_id',
    schedule TEXT,  -- JSON
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_flag_strategies_org_id ON flag_strategies(org_id);

-- Kill switches
CREATE TABLE kill_switches (
    id TEXT PRIMARY KEY,
    org_id TEXT REFERENCES organizations(id),  -- NULL = platform
    key TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    linked_flag_keys TEXT NOT NULL,  -- JSON array
    is_active INTEGER NOT NULL DEFAULT 0,
    activated_at TEXT,
    activated_by TEXT REFERENCES users(id),
    activation_reason TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(org_id, key)
);

CREATE INDEX idx_kill_switches_org_id ON kill_switches(org_id);
CREATE INDEX idx_kill_switches_is_active ON kill_switches(is_active);

-- SDK keys
CREATE TABLE sdk_keys (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id) ON DELETE CASCADE,
    key_type TEXT NOT NULL,  -- 'client_side', 'server_side'
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX idx_sdk_keys_environment_id ON sdk_keys(environment_id);
CREATE INDEX idx_sdk_keys_key_hash ON sdk_keys(key_hash);

-- Exposure logs
CREATE TABLE exposure_logs (
    id TEXT PRIMARY KEY,
    flag_id TEXT NOT NULL REFERENCES flags(id),
    flag_key TEXT NOT NULL,
    environment_id TEXT NOT NULL REFERENCES flag_environments(id),
    user_id TEXT,
    org_id TEXT,
    variant TEXT NOT NULL,
    reason TEXT NOT NULL,
    context_hash TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE INDEX idx_exposure_logs_flag_id ON exposure_logs(flag_id);
CREATE INDEX idx_exposure_logs_timestamp ON exposure_logs(timestamp);
CREATE INDEX idx_exposure_logs_context_hash ON exposure_logs(context_hash, timestamp);

-- Flag statistics
CREATE TABLE flag_stats (
    flag_id TEXT PRIMARY KEY REFERENCES flags(id) ON DELETE CASCADE,
    last_evaluated_at TEXT,
    evaluation_count_24h INTEGER NOT NULL DEFAULT 0,
    evaluation_count_7d INTEGER NOT NULL DEFAULT 0,
    evaluation_count_30d INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
```

---

## 16. Audit Events

All flag operations are logged via `loom-server-audit`:

| Event | Description |
|-------|-------------|
| `FlagCreated` | New flag created |
| `FlagUpdated` | Flag metadata/variants updated |
| `FlagArchived` | Flag archived |
| `FlagRestored` | Flag restored from archive |
| `FlagConfigUpdated` | Environment config changed |
| `StrategyCreated` | New strategy created |
| `StrategyUpdated` | Strategy updated |
| `StrategyDeleted` | Strategy deleted |
| `KillSwitchCreated` | Kill switch created |
| `KillSwitchActivated` | Kill switch activated (includes reason) |
| `KillSwitchDeactivated` | Kill switch deactivated |
| `KillSwitchDeleted` | Kill switch deleted |
| `SdkKeyCreated` | SDK key created |
| `SdkKeyRevoked` | SDK key revoked |
| `EnvironmentCreated` | Environment created |
| `EnvironmentDeleted` | Environment deleted |

---

## 17. Permissions

### 17.1 Flag Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List flags | ✓ | ✓ (read) | ✓ (all orgs) |
| Create flag | ✓ | ✗ | ✓ (platform) |
| Update flag | ✓ | ✗ | ✓ (platform) |
| Archive flag | ✓ | ✗ | ✓ (platform) |
| Update config | ✓ | ✗ | ✓ (platform) |

### 17.2 Kill Switch

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List kill switches | ✓ | ✓ (read) | ✓ (all) |
| Create kill switch | ✓ | ✗ | ✓ (platform) |
| Activate/Deactivate | ✓ | ✗ | ✓ (platform) |

### 17.3 SDK Keys

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List SDK keys | ✓ | ✗ | ✓ (all) |
| Create SDK key | ✓ | ✗ | ✓ |
| Revoke SDK key | ✓ | ✗ | ✓ |

---

## 18. Implementation Phases

### Phase 1: Core Types & Database (2-3 hours)

- [ ] Create `loom-flags-core` crate
- [ ] Define Flag, Variant, Strategy, KillSwitch types
- [ ] Define EvaluationContext and EvaluationResult
- [ ] Add database migration for all tables
- [ ] Create repository layer in `loom-server-flags`

### Phase 2: Environment & SDK Keys (2-3 hours)

- [ ] Environment CRUD handlers
- [ ] Auto-create dev/prod on org creation
- [ ] SDK key generation with Argon2 hashing
- [ ] SDK key authentication middleware
- [ ] SDK key CRUD handlers

### Phase 3: Flag Management (3-4 hours)

- [ ] Flag CRUD handlers
- [ ] FlagConfig per-environment handlers
- [ ] Auto-create configs for all environments on flag creation
- [ ] Flag key validation
- [ ] Prerequisites handling

### Phase 4: Strategy System (3-4 hours)

- [ ] Strategy CRUD handlers
- [ ] Condition evaluation engine
- [ ] Percentage hashing with murmur3
- [ ] Schedule evaluation
- [ ] GeoIP integration with proxy header support

### Phase 5: Kill Switches (2-3 hours)

- [ ] Kill switch CRUD handlers
- [ ] Activation/deactivation with reason
- [ ] Link to flags
- [ ] `killswitch:activate` permission

### Phase 6: Evaluation Engine (3-4 hours)

- [ ] Full evaluation flow implementation
- [ ] Platform vs org precedence
- [ ] Kill switch override logic
- [ ] Prerequisite checking
- [ ] POST `/api/flags/evaluate` endpoint
- [ ] POST `/api/flags/evaluate/{key}` endpoint

### Phase 7: SSE Streaming (2-3 hours)

- [ ] SSE endpoint with SDK key auth
- [ ] Initial full state on connect
- [ ] Flag update broadcasts
- [ ] Kill switch broadcasts
- [ ] Heartbeat mechanism

### Phase 8: Exposure Tracking (2-3 hours)

- [ ] Exposure log table and repository
- [ ] Deduplication logic
- [ ] Per-flag exposure toggle
- [ ] Query and export endpoints

### Phase 9: Stale Detection & Stats (1-2 hours)

- [ ] Flag stats tracking
- [ ] Stale flag detection endpoint
- [ ] Evaluation count rollups

### Phase 10: Rust SDK (3-4 hours)

- [ ] `loom-flags` crate structure
- [ ] FlagsClient with initialization
- [ ] Local caching
- [ ] SSE connection handling
- [ ] Offline mode with cached values
- [ ] Evaluation methods (get_bool, get_string, get_json)

### Phase 11: TypeScript Packages (4-5 hours)

- [ ] `@loom/http` package with retry, headers
- [ ] `@loom/flags` client implementation
- [ ] Local caching
- [ ] SSE connection handling
- [ ] Event emitter for updates

### Phase 12: Audit Integration (1-2 hours)

- [ ] Add all audit event types
- [ ] Integrate with handlers
- [ ] Test audit logging

### Phase 13: Platform Flags (2-3 hours)

- [ ] Super admin routes
- [ ] Platform flag/kill switch management
- [ ] Precedence in evaluation engine

---

## 19. Rust Dependencies

```toml
# loom-flags-core
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }

# loom-flags (SDK)
[dependencies]
loom-flags-core = { path = "../loom-flags-core" }
loom-http = { path = "../loom-http" }
async-trait = "0.1"
futures = "0.3"
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"
eventsource-stream = "0.2"

# loom-server-flags
[dependencies]
loom-flags-core = { path = "../loom-flags-core" }
loom-db = { path = "../loom-db" }
loom-server-audit = { path = "../loom-server-audit" }
loom-geoip = { path = "../loom-geoip" }
axum = "0.8"
sqlx = { version = "0.8", features = ["sqlite"] }
murmur3 = "0.5"
```

---

## 20. Security Considerations

### 20.1 SDK Key Security

- SDK keys stored as Argon2 hashes
- Keys shown once at creation
- Client-side keys safe for browser (limited to single user context)
- Server-side keys must be kept secret

### 20.2 Kill Switch Permissions

- Separate permission for activation
- Reason required for audit trail
- No auto-reset to prevent accidental re-enablement

### 20.3 Flag Evaluation

- All evaluation server-side (no flag data in client-side SDK)
- Client-side SDK only receives evaluated results
- GeoIP resolved from trusted headers only

---

## Appendix A: SDK Key Header Examples

### Rust SDK

```rust
let client = reqwest::Client::new();
client
    .post("https://loom.example.com/api/flags/evaluate")
    .header("Authorization", "Bearer loom_sdk_server_prod_xxx")
    .header("User-Agent", "loom-flags-sdk/1.0.0")
    .json(&context)
    .send()
    .await?;
```

### TypeScript SDK

```typescript
fetch('https://loom.example.com/api/flags/evaluate', {
  method: 'POST',
  headers: {
    'Authorization': 'Bearer loom_sdk_client_prod_xxx',
    'User-Agent': '@loom/flags/1.0.0',
    'Content-Type': 'application/json',
  },
  body: JSON.stringify(context),
});
```

---

## Appendix B: Future Considerations

| Feature | Description |
|---------|-------------|
| Webhooks | Notify external systems on flag changes |
| Rate limiting | Per-SDK-key rate limits on evaluation |
| Code references | Scan codebase for flag usage |
| Strategy composition | Combine multiple strategies with AND/OR |
| Metrics integration | Built-in experiment metrics and analysis |
| Flag scheduling | Enable/disable flags at specific times |
| Bulk operations | Import/export flags as JSON/YAML |
| Flag templates | Reusable flag configurations |
