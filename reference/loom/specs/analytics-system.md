<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Product Analytics System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-11

---

## 1. Overview

### Purpose

This specification defines a product analytics and experimentation platform for Loom. Products built on Loom can track user behavior, run A/B experiments, and analyze conversion funnels. The system follows PostHog's patterns for identity management, event tracking, and experiment integration.

### Goals

- **Person profiles** for both anonymous and identified users
- **Event tracking** with flexible properties
- **Identity resolution** linking anonymous sessions to authenticated users
- **Experiment integration** with existing feature flags system (`loom-flags-core`)
- **Multi-tenant** analytics scoped to organizations
- **SDKs** for Rust and TypeScript clients

### Non-Goals

- Real-time dashboards — analytics data is for API consumption/export
- Cohort analysis engine — deferred to future version
- Session replay — out of scope
- Heatmaps — out of scope

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-analytics-core/            # Shared types for analytics
│   ├── src/
│   │   ├── lib.rs
│   │   ├── person.rs               # Person, PersonIdentity
│   │   ├── event.rs                # Event, EventProperty
│   │   ├── identify.rs             # IdentifyPayload, AliasPayload
│   │   ├── group.rs                # Group analytics (optional)
│   │   └── error.rs                # Error types
│   └── Cargo.toml
├── loom-analytics/                 # Rust SDK client
│   ├── src/
│   │   ├── lib.rs
│   │   ├── client.rs               # AnalyticsClient
│   │   ├── batch.rs                # Event batching
│   │   └── error.rs
│   └── Cargo.toml
├── loom-server-analytics/          # Server-side API and storage
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs               # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs          # Event capture endpoint
│   │   │   ├── identify.rs         # Identity resolution
│   │   │   ├── persons.rs          # Person queries
│   │   │   └── events.rs           # Event queries
│   │   ├── repository.rs           # Database repository
│   │   ├── identity_resolution.rs  # Merge logic
│   │   └── api_key.rs              # Analytics API key management
│   └── Cargo.toml

web/
├── packages/
│   └── analytics/                  # @loom/analytics - TypeScript SDK
│       ├── src/
│       │   ├── index.ts
│       │   ├── client.ts           # AnalyticsClient
│       │   ├── storage.ts          # LocalStorage/cookie for distinct_id
│       │   ├── batch.ts            # Event batching
│       │   └── identify.ts         # Identity helpers
│       └── package.json
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Clients                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│    Product App (Web)           Product Backend           Other Services      │
│    (@loom/analytics)           (loom-analytics)          (loom-analytics)    │
│    Generates distinct_id       Server-side tracking      Server-side         │
└────────┬─────────────────────────┬──────────────────────────┬───────────────┘
         │                         │                          │
         └─────────────────────────┼──────────────────────────┘
                                   │
                                   ▼ REST API
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │ Analytics Routes│  │ API Key Auth    │  │ Identity        │              │
│  │ /api/analytics/*│  │ Middleware      │  │ Resolution      │              │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘              │
│           │                    │                    │                        │
│           └────────────────────┼────────────────────┘                        │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                    Experiment Integration                             │   │
│  │    Links analytics events to feature flag exposures (loom-flags)     │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  analytics_persons    analytics_person_identities    analytics_events       │
│  analytics_api_keys   analytics_person_properties                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Entities

### 3.1 Person

A person represents a user (anonymous or identified) being tracked.

```rust
use loom_secret::Secret;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub org_id: OrgId,
    pub properties: serde_json::Value,    // JSON object of person properties
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonWithIdentities {
    pub person: Person,
    pub identities: Vec<PersonIdentity>,
}
```

### 3.2 PersonIdentity

Links distinct_ids to a person. Multiple distinct_ids can point to the same person.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonIdentity {
    pub id: PersonIdentityId,
    pub person_id: PersonId,
    pub distinct_id: String,              // SDK-generated UUID or user's real ID
    pub identity_type: IdentityType,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IdentityType {
    Anonymous,    // SDK-generated UUIDv7
    Identified,   // User's real ID (email, user_id, etc.)
}
```

### 3.3 Event

A tracked user action or occurrence.

```rust
use loom_secret::Secret;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub org_id: OrgId,
    pub person_id: Option<PersonId>,      // Resolved after ingestion
    pub distinct_id: String,
    pub event_name: String,               // e.g., "button_clicked", "$pageview"
    pub properties: serde_json::Value,    // Event-specific properties
    pub timestamp: DateTime<Utc>,
    pub ip_address: Option<Secret<String>>,  // Wrapped in Secret for auto-redaction
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

### 3.4 Analytics API Key

Authentication key for SDK clients (separate from feature flag SDK keys).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsApiKey {
    pub id: AnalyticsApiKeyId,
    pub org_id: OrgId,
    pub name: String,
    pub key_type: AnalyticsKeyType,
    pub key_hash: String,                 // Argon2 hash
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnalyticsKeyType {
    Write,    // Can only capture events (safe for client-side)
    ReadWrite, // Can capture and query (server-side only)
}
```

---

## 4. Identity Resolution

### 4.1 PostHog-Style Identity Model

Following PostHog's approach:

1. **Anonymous user arrives**: SDK generates UUIDv7, stored in localStorage/cookie
2. **Events captured**: All events tagged with this `distinct_id`
3. **User identifies**: SDK calls `identify(anonymous_id, user_id)`
4. **Merge occurs**: Both distinct_ids linked to same Person, properties merged
5. **Future events**: Can use either distinct_id, resolve to same Person

### 4.2 Identify Operation

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyPayload {
    pub distinct_id: String,              // Current distinct_id (often anonymous)
    pub user_id: String,                  // The "real" user identifier
    #[serde(default)]
    pub properties: serde_json::Value,    // Person properties to set/update
}
```

**Server-side logic:**

```
1. Find or create Person for distinct_id
2. Find or create PersonIdentity for user_id
3. If user_id already has a Person:
   - If same Person: no-op, just update properties
   - If different Person: merge Persons (see 4.3)
4. If user_id has no Person:
   - Link user_id identity to distinct_id's Person
5. Update Person properties (user_id properties take precedence)
```

### 4.3 Person Merge

When two distinct_ids that map to different Persons are identified as the same user:

```rust
pub struct PersonMerge {
    pub winner_id: PersonId,    // Person that survives
    pub loser_id: PersonId,     // Person that gets merged into winner
    pub reason: MergeReason,
    pub merged_at: DateTime<Utc>,
}

pub enum MergeReason {
    Identify { distinct_id: String, user_id: String },
    Alias { alias: String, distinct_id: String },
    Manual { by_user_id: UserId },
}
```

**Merge rules:**

1. Person with `Identified` identity type wins (user_id > anonymous)
2. If both identified, older Person wins
3. All events from loser reassigned to winner
4. All identities from loser moved to winner
5. Properties merged (winner takes precedence on conflicts)
6. Loser Person marked as merged (soft delete)

### 4.4 Alias Operation

Alternative to identify for linking two distinct_ids:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasPayload {
    pub distinct_id: String,    // The primary identity
    pub alias: String,          // The secondary identity to link
}
```

### 4.5 Reset Operation

When a user logs out, the SDK should call `reset()`:

1. Generate new anonymous distinct_id
2. Store in localStorage/cookie
3. Future events use new distinct_id
4. Previous distinct_id remains linked to original Person

---

## 5. Event Tracking

### 5.1 Capture Endpoint

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturePayload {
    pub distinct_id: String,
    pub event: String,
    #[serde(default)]
    pub properties: serde_json::Value,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,  // Defaults to server time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCapturePayload {
    pub batch: Vec<CapturePayload>,
}
```

### 5.2 Automatic Properties

Server adds these to every event:

| Property | Description |
|----------|-------------|
| `$ip` | Client IP (from headers, wrapped in `Secret`) |
| `$user_agent` | User-Agent header |
| `$timestamp` | Server receive time |
| `$lib` | SDK name (e.g., "loom-analytics", "@loom/analytics") |
| `$lib_version` | SDK version |

### 5.3 Special Events

Following PostHog conventions:

| Event | Description |
|-------|-------------|
| `$pageview` | Page view (web SDK auto-captures) |
| `$pageleave` | Page leave |
| `$identify` | Logged when identify() called |
| `$feature_flag_called` | When feature flag evaluated (integration with loom-flags) |

### 5.4 Person Properties vs Event Properties

| Type | Scope | Example |
|------|-------|---------|
| **Person properties** | Persistent on the Person | `email`, `plan`, `company` |
| **Event properties** | Single event only | `button_name`, `page_url` |

Set person properties via:

```rust
// In identify call
identify(distinct_id, user_id, properties: { "plan": "pro" })

// Or dedicated set call
set_person_properties(distinct_id, { "plan": "pro" })
```

### 5.5 Property Operations

Following PostHog:

| Operation | Description |
|-----------|-------------|
| `$set` | Set properties (overwrites) |
| `$set_once` | Set only if not already set |
| `$unset` | Remove properties |

---

## 6. Experiment Integration

### 6.1 Feature Flag Exposure Tracking

When a feature flag is evaluated, automatically track:

```rust
// loom-flags SDK calls this internally
analytics.capture({
    event: "$feature_flag_called",
    distinct_id: context.user_id,
    properties: {
        "$feature_flag": "checkout.new_flow",
        "$feature_flag_response": "treatment_a",
    }
})
```

### 6.2 Linking Experiments to Analytics

The existing `exposure_logs` table (from `loom-server-flags`) connects to analytics:

```
exposure_logs.user_id  ←→  analytics_person_identities.distinct_id
```

Experiment analysis queries can join:
- Feature flag exposures (which variant a user saw)
- Analytics events (what actions they took)

### 6.3 Experiment Metrics

Products define success metrics as analytics events:

```typescript
// Track conversion after seeing experiment
analytics.capture("checkout_completed", {
  order_value: 99.00,
  currency: "USD"
});
```

Query experiment results via API:

```sql
SELECT
  el.variant,
  COUNT(DISTINCT ae.person_id) as conversions,
  COUNT(DISTINCT el.user_id) as exposures,
  CAST(COUNT(DISTINCT ae.person_id) AS REAL) / COUNT(DISTINCT el.user_id) as conversion_rate
FROM exposure_logs el
LEFT JOIN analytics_person_identities api ON api.distinct_id = el.user_id
LEFT JOIN analytics_events ae ON ae.person_id = api.person_id
  AND ae.event_name = 'checkout_completed'
  AND ae.timestamp > el.timestamp
WHERE el.flag_key = 'checkout.new_flow'
GROUP BY el.variant;
```

---

## 7. API Endpoints

### 7.1 Base Path

All analytics endpoints are under `/api/analytics/*`.

### 7.2 Capture (Requires Write API Key)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/analytics/capture` | Capture single event |
| POST | `/api/analytics/batch` | Capture batch of events |
| POST | `/api/analytics/identify` | Identify user |
| POST | `/api/analytics/alias` | Alias two distinct_ids |
| POST | `/api/analytics/set` | Set person properties |

### 7.3 Query (Requires ReadWrite API Key)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/analytics/persons` | List persons |
| GET | `/api/analytics/persons/{id}` | Get person by ID |
| GET | `/api/analytics/persons/by-distinct-id/{distinct_id}` | Get person by distinct_id |
| GET | `/api/analytics/events` | Query events |
| GET | `/api/analytics/events/count` | Count events by filters |
| POST | `/api/analytics/events/export` | Bulk export events |

### 7.4 Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/analytics/api-keys` | List API keys |
| POST | `/api/analytics/api-keys` | Create API key |
| DELETE | `/api/analytics/api-keys/{id}` | Revoke API key |

---

## 8. SDK Design

### 8.1 Rust SDK (`loom-analytics`)

```rust
use loom_analytics::{AnalyticsClient, Properties};

// Initialize
let client = AnalyticsClient::builder()
    .api_key("loom_analytics_write_xxx")
    .base_url("https://loom.example.com")
    .flush_interval(Duration::from_secs(10))
    .build()?;

// Capture event
client.capture("button_clicked", "user_123", Properties::new()
    .insert("button_name", "checkout")
    .insert("page", "/cart")
).await?;

// Identify user
client.identify("anon_abc123", "user@example.com", Properties::new()
    .insert("plan", "pro")
    .insert("company", "Acme Inc")
).await?;

// Set person properties
client.set("user@example.com", Properties::new()
    .insert("last_login", Utc::now().to_rfc3339())
).await?;

// Shutdown (flushes pending events)
client.shutdown().await?;
```

### 8.2 TypeScript SDK (`@loom/analytics`)

```typescript
import { AnalyticsClient } from '@loom/analytics';

// Initialize (browser)
const analytics = new AnalyticsClient({
  apiKey: 'loom_analytics_write_xxx',
  baseUrl: 'https://loom.example.com',
  persistence: 'localStorage+cookie', // or 'memory'
  autocapture: true, // Auto-capture $pageview
});

// Capture event
analytics.capture('button_clicked', {
  button_name: 'checkout',
  page: '/cart',
});

// Identify user (links anonymous to authenticated)
analytics.identify('user@example.com', {
  plan: 'pro',
  company: 'Acme Inc',
});

// Get current distinct_id
const distinctId = analytics.getDistinctId();

// Reset on logout
analytics.reset();
```

### 8.3 SDK Behavior

| Aspect | Behavior |
|--------|----------|
| **Distinct ID** | UUIDv7 generated client-side, stored in localStorage+cookie |
| **Batching** | Events queued, flushed every 10s or 10 events |
| **Retry** | Exponential backoff on failure (via `loom-http`) |
| **Offline** | Events queued in memory, flushed when online |
| **Persistence** | Cookie for cross-subdomain, localStorage for properties |
| **Reset** | Generate new distinct_id, clear stored properties |

### 8.4 HTTP Client Requirements

Both SDKs use shared HTTP client libraries:
- Rust: `loom-http` with retry, User-Agent
- TypeScript: `@loom/http` with retry, User-Agent

---

## 9. Database Schema

### 9.1 Migration: `XXX_analytics.sql`

```sql
-- Persons (users being tracked)
CREATE TABLE analytics_persons (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    properties TEXT NOT NULL DEFAULT '{}',  -- JSON object
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    merged_into_id TEXT REFERENCES analytics_persons(id),  -- For merges
    merged_at TEXT
);

CREATE INDEX idx_analytics_persons_org_id ON analytics_persons(org_id);
CREATE INDEX idx_analytics_persons_merged_into ON analytics_persons(merged_into_id);

-- Person identities (distinct_ids linked to persons)
CREATE TABLE analytics_person_identities (
    id TEXT PRIMARY KEY,
    person_id TEXT NOT NULL REFERENCES analytics_persons(id) ON DELETE CASCADE,
    distinct_id TEXT NOT NULL,
    identity_type TEXT NOT NULL,  -- 'anonymous', 'identified'
    created_at TEXT NOT NULL,
    UNIQUE(person_id, distinct_id)
);

-- Index for fast lookup by distinct_id within an org
-- We need org_id here, get it via join or denormalize
CREATE INDEX idx_analytics_person_identities_distinct_id ON analytics_person_identities(distinct_id);
CREATE INDEX idx_analytics_person_identities_person_id ON analytics_person_identities(person_id);

-- Events
CREATE TABLE analytics_events (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    person_id TEXT REFERENCES analytics_persons(id),
    distinct_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',  -- JSON object
    timestamp TEXT NOT NULL,
    ip_address TEXT,  -- Stored encrypted or hashed
    user_agent TEXT,
    lib TEXT,
    lib_version TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_analytics_events_org_id ON analytics_events(org_id);
CREATE INDEX idx_analytics_events_person_id ON analytics_events(person_id);
CREATE INDEX idx_analytics_events_distinct_id ON analytics_events(distinct_id);
CREATE INDEX idx_analytics_events_event_name ON analytics_events(event_name);
CREATE INDEX idx_analytics_events_timestamp ON analytics_events(timestamp);

-- Person merges (audit trail)
CREATE TABLE analytics_person_merges (
    id TEXT PRIMARY KEY,
    winner_id TEXT NOT NULL REFERENCES analytics_persons(id),
    loser_id TEXT NOT NULL REFERENCES analytics_persons(id),
    reason TEXT NOT NULL,  -- JSON: { type: "identify", distinct_id: "...", user_id: "..." }
    merged_at TEXT NOT NULL
);

CREATE INDEX idx_analytics_person_merges_winner ON analytics_person_merges(winner_id);
CREATE INDEX idx_analytics_person_merges_loser ON analytics_person_merges(loser_id);

-- Analytics API keys
CREATE TABLE analytics_api_keys (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_type TEXT NOT NULL,  -- 'write', 'read_write'
    key_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX idx_analytics_api_keys_org_id ON analytics_api_keys(org_id);
CREATE INDEX idx_analytics_api_keys_key_hash ON analytics_api_keys(key_hash);
```

---

## 10. API Key Management

### 10.1 Key Types

| Type | Prefix | Use Case | Capabilities |
|------|--------|----------|--------------|
| Write | `loom_analytics_write_` | Client-side, public | Capture, identify, alias |
| ReadWrite | `loom_analytics_rw_` | Server-side, secret | All write + query/export |

### 10.2 Key Format

```
loom_analytics_{type}_{random}

Example:
loom_analytics_write_7a3b9f2e1c4d8a5b6e0f3c2d1a4b5c6d7e8f9a0b
loom_analytics_rw_8b4c0g3f2d5e9a6c7f1g4d3e2b5a6c7d8e9f0a1b
```

### 10.3 Authentication

SDK requests include:

```
Authorization: Bearer loom_analytics_write_xxx
```

Server validates:
1. Parse key type from prefix
2. Hash key, lookup in `analytics_api_keys`
3. Check not revoked
4. Update `last_used_at`
5. Inject `org_id` into request context

---

## 11. Configuration

### 11.1 Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_ANALYTICS_ENABLED` | boolean | Enable analytics system | `true` |
| `LOOM_ANALYTICS_BATCH_SIZE` | integer | Max events per batch | `100` |
| `LOOM_ANALYTICS_FLUSH_INTERVAL_SECS` | integer | SDK flush interval | `10` |
| `LOOM_ANALYTICS_EVENT_RETENTION_DAYS` | integer | Event retention period | `365` |

---

## 12. Audit Events

Analytics operations logged via `loom-server-audit`:

| Event | Description |
|-------|-------------|
| `AnalyticsApiKeyCreated` | API key created |
| `AnalyticsApiKeyRevoked` | API key revoked |
| `AnalyticsPersonMerged` | Two persons merged |
| `AnalyticsEventsExported` | Bulk export performed |

---

## 13. Permissions

### 13.1 API Key Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List API keys | ✓ | ✗ | ✓ (all) |
| Create API key | ✓ | ✗ | ✓ |
| Revoke API key | ✓ | ✗ | ✓ |

### 13.2 Query Access

| Action | Write Key | ReadWrite Key |
|--------|-----------|---------------|
| Capture events | ✓ | ✓ |
| Identify/alias | ✓ | ✓ |
| Query persons | ✗ | ✓ |
| Query events | ✗ | ✓ |
| Export events | ✗ | ✓ |

---

## 14. Security Considerations

### 14.1 IP Address Handling

IP addresses are sensitive data. Use `loom-secret::Secret`:

```rust
use loom_secret::Secret;

pub struct Event {
    // ...
    pub ip_address: Option<Secret<String>>,
}
```

This ensures:
- Auto-redaction in logs and Debug output
- Explicit `.expose()` required to access
- Serialization can be controlled

### 14.2 Write Key Safety

Write-only keys are safe for client-side because:
- Cannot read any data back
- Cannot query other users
- Scoped to single org

### 14.3 Event Validation

Validate incoming events:
- `event_name`: Max 200 chars, alphanumeric + underscore + `$` prefix
- `properties`: Max 1MB JSON
- `distinct_id`: Max 200 chars

---

## 15. Rust Dependencies

```toml
# loom-analytics-core
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }
loom-secret = { path = "../loom-secret" }

# loom-analytics (SDK)
[dependencies]
loom-analytics-core = { path = "../loom-analytics-core" }
loom-http = { path = "../loom-http" }
async-trait = "0.1"
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"

# loom-server-analytics
[dependencies]
loom-analytics-core = { path = "../loom-analytics-core" }
loom-db = { path = "../loom-db" }
loom-server-audit = { path = "../loom-server-audit" }
loom-secret = { path = "../loom-secret" }
axum = "0.8"
sqlx = { version = "0.8", features = ["sqlite"] }
argon2 = "0.5"
```

---

## Appendix A: PostHog Compatibility

This system follows PostHog patterns but is not a drop-in replacement:

| PostHog | Loom Analytics | Notes |
|---------|----------------|-------|
| `posthog.capture()` | `analytics.capture()` | Same semantics |
| `posthog.identify()` | `analytics.identify()` | Same semantics |
| `posthog.alias()` | `analytics.alias()` | Same semantics |
| `posthog.reset()` | `analytics.reset()` | Same semantics |
| `posthog.group()` | Not in v1 | Deferred |
| Project API Key | Org API Key | Scoped to org, not project |

---

## Appendix B: Future Considerations

| Feature | Description |
|---------|-------------|
| Group analytics | Track companies/teams as first-class entities |
| Funnels API | Built-in funnel analysis |
| Cohorts | User segmentation |
| Data warehouse export | Stream to BigQuery/Snowflake |
| Webhooks | Real-time event notifications |
| Session tracking | Group events into sessions |
