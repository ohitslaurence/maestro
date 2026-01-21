<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Audit System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-03

---

## 1. Overview

### Purpose

This specification defines a comprehensive audit logging system for Loom with SIEM (Security Information and Event Management) integration capabilities. The system provides centralized, non-blocking audit event capture with pluggable output sinks for enterprise compliance requirements.

### Goals

- **Centralized audit logging** with async, non-blocking event dispatch
- **SIEM integration** via syslog (RFC 5424), HTTP webhooks, CEF, and JSON streams
- **OpenTelemetry compatibility** via tracing integration
- **Event enrichment** with session, organization, and geo-IP context
- **Configurable filtering** by event type and severity level
- **Graceful degradation** when sinks are unreachable
- **Enterprise compliance** support for SOC2, HIPAA, and similar standards
- **Zero-copy fan-out** to multiple sinks without event duplication

### Non-Goals

- Distributed message bus (Kafka/NATS) — deferred to future version
- Guaranteed delivery with disk-backed queues
- Cross-service audit aggregation
- Long-term warehouse storage (use ETL from sinks)
- Real-time alerting (handled by SIEM)

---

## 2. Architecture

### Crate Structure

```
crates/
  loom-server-audit/
    src/
      lib.rs              # Crate exports and prelude
      config.rs           # Configuration types and loading
      event.rs            # Core event model + builders
      enrichment.rs       # Enrichment traits and enriched event wrapper
      filter.rs           # Event filtering by type/severity
      pipeline.rs         # AuditService: queue, dispatch, backpressure
      sink/
        mod.rs            # AuditSink trait, sink registry
        sqlite.rs         # SQLite sink (primary storage)
        file.rs           # JSONL / CEF file sink
        syslog.rs         # RFC 5424 syslog sink
        http.rs           # HTTP(S) webhook sink
        json_stream.rs    # JSON over TCP/UDP sink
        cef.rs            # CEF formatter helpers
        tracing.rs        # tracing/OpenTelemetry sink
      error.rs            # Error types for sinks/pipeline
```

### Migration from Current Implementation

The current implementation is split across:
- `loom-server-auth/src/audit.rs` — `AuditEventType`, `AuditLogEntry`, `AuditLogBuilder`
- `loom-server-db/src/audit.rs` — `AuditRepository` for SQLite persistence

After migration:
- Types move to `loom-server-audit::event`
- `AuditRepository` becomes `SqliteAuditSink`
- Handlers use `AuditService::log()` instead of direct DB calls

### Component Diagram

```
┌────────────────────────────────────────────────────────────────────────┐
│                          Request Handlers                               │
│    (share.rs, api_keys.rs, admin.rs, abac_middleware.rs)               │
└───────────────────────────────┬────────────────────────────────────────┘
                                │ AuditLogEntry
                                ▼
┌────────────────────────────────────────────────────────────────────────┐
│                         AuditService                                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                     │
│  │ mpsc Queue  │→ │  Enricher   │→ │Global Filter│                     │
│  │ (bounded)   │  │ (session,   │  │ (severity,  │                     │
│  │             │  │  org, geo)  │  │  event type)│                     │
│  └─────────────┘  └─────────────┘  └─────────────┘                     │
│                                           │                             │
│                                           ▼                             │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │                      Fan-out to Sinks                             │  │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐     │  │
│  │  │ SQLite  │ │ Syslog  │ │  HTTP   │ │ JSON/   │ │ Tracing │     │  │
│  │  │  Sink   │ │  Sink   │ │  Sink   │ │ TCP/UDP │ │  Sink   │     │  │
│  │  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘     │  │
│  └───────┼──────────┼──────────┼──────────┼──────────┼──────────────┘  │
└──────────┼──────────┼──────────┼──────────┼──────────┼──────────────────┘
           │          │          │          │          │
           ▼          ▼          ▼          ▼          ▼
      ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────────┐
      │ SQLite │ │ Splunk │ │Datadog │ │ Custom │ │OpenTelemetry│
      │   DB   │ │ QRadar │ │ Elastic│ │Collector│ │  Exporter  │
      └────────┘ │ArcSight│ │SumoLogic│└────────┘ └────────────┘
                 └────────┘ └────────┘
```

### Data Flow

```
Handler creates AuditLogEntry
    │
    ▼ audit_service.log(entry)
┌───────────────────────────────┐
│ try_send to bounded mpsc queue│
│ (non-blocking, returns bool)  │
└───────────────┬───────────────┘
                │ background task
                ▼
┌───────────────────────────────┐
│ Enricher adds context:        │
│ - Session (id, type, device)  │
│ - Org (id, slug, role)        │
│ - GeoIP (city, country)       │
└───────────────┬───────────────┘
                │
                ▼
┌───────────────────────────────┐
│ Global filter checks:         │
│ - min_severity                │
│ - include/exclude event types │
└───────────────┬───────────────┘
                │ if allowed
                ▼
┌───────────────────────────────┐
│ Arc<EnrichedAuditEvent>       │
│ (zero-copy for fan-out)       │
└───────────────┬───────────────┘
                │
        ┌───────┴───────┐
        ▼               ▼ (for each sink)
┌───────────────┐ ┌───────────────┐
│ Sink filter   │ │ Sink filter   │
└───────┬───────┘ └───────┬───────┘
        │ if allowed      │
        ▼                 ▼
┌───────────────┐ ┌───────────────┐
│sink.publish() │ │sink.publish() │
│  (async)      │ │  (async)      │
└───────────────┘ └───────────────┘
```

---

## 3. Core Types

### AuditEventType

Extended from current implementation with additional categories:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication events
    Login,
    Logout,
    LoginFailed,
    MagicLinkRequested,
    MagicLinkUsed,
    DeviceCodeStarted,
    DeviceCodeCompleted,

    // Session events
    SessionCreated,
    SessionRevoked,
    SessionExpired,

    // API key events
    ApiKeyCreated,
    ApiKeyUsed,
    ApiKeyRevoked,

    // Access control events
    AccessGranted,
    AccessDenied,

    // Organization events
    OrgCreated,
    OrgUpdated,
    OrgDeleted,
    OrgRestored,
    MemberAdded,
    MemberRemoved,
    RoleChanged,

    // Team events
    TeamCreated,
    TeamUpdated,
    TeamDeleted,
    TeamMemberAdded,
    TeamMemberRemoved,

    // Thread events
    ThreadCreated,
    ThreadDeleted,
    ThreadShared,
    ThreadUnshared,
    ThreadVisibilityChanged,

    // Support access events
    SupportAccessRequested,
    SupportAccessApproved,
    SupportAccessRevoked,

    // Admin events
    ImpersonationStarted,
    ImpersonationEnded,
    GlobalRoleChanged,

    // Weaver events
    WeaverCreated,
    WeaverDeleted,
    WeaverAttached,

    // LLM events
    LlmRequestStarted,
    LlmRequestCompleted,
    LlmRequestFailed,

    // SCM events
    RepoCreated,
    RepoDeleted,
    MirrorCreated,
    MirrorSynced,
    WebhookReceived,
}
```

### AuditSeverity

Maps to syslog severity levels:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Debug = 7,      // RFC 5424: Debug
    Info = 6,       // RFC 5424: Informational
    Notice = 5,     // RFC 5424: Notice
    Warning = 4,    // RFC 5424: Warning
    Error = 3,      // RFC 5424: Error
    Critical = 2,   // RFC 5424: Critical
}
```

Default severity mapping:

| Event Category | Default Severity |
|----------------|------------------|
| `Login`, `SessionCreated`, `*Created` | Info |
| `LoginFailed`, `AccessDenied` | Warning |
| `*Deleted`, `*Revoked` | Notice |
| `ImpersonationStarted`, `GlobalRoleChanged` | Notice |
| `LlmRequestFailed` | Error |

### AuditLogEntry

Extended with correlation IDs for OpenTelemetry:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub severity: AuditSeverity,

    // Actor
    pub actor_user_id: Option<UserId>,
    pub impersonating_user_id: Option<UserId>,

    // Resource
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,

    // Context
    pub action: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub details: serde_json::Value,

    // Correlation (OpenTelemetry)
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub request_id: Option<String>,
}
```

### EnrichedAuditEvent

Wrapper with additional context for SIEM:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedAuditEvent {
    pub base: AuditLogEntry,
    pub session: Option<SessionContext>,
    pub org: Option<OrgContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionContext {
    pub session_id: Option<String>,
    pub session_type: Option<String>,  // "web", "cli", "vscode"
    pub device_label: Option<String>,
    pub geo: Option<GeoIpInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoIpInfo {
    pub city: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrgContext {
    pub org_id: Option<String>,
    pub org_slug: Option<String>,
    pub org_role: Option<String>,
    pub team_id: Option<String>,
    pub team_role: Option<String>,
}
```

---

## 4. Sink Trait

```rust
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Unique name for this sink (used in logs/metrics).
    fn name(&self) -> &str;

    /// Per-sink filter configuration.
    fn filter(&self) -> &AuditFilterConfig;

    /// Publish an event to the sink.
    async fn publish(
        &self,
        event: Arc<EnrichedAuditEvent>,
    ) -> Result<(), AuditSinkError>;

    /// Health check (optional, default: Ok).
    async fn health_check(&self) -> Result<(), AuditSinkError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditSinkError {
    #[error("transient error: {0}")]
    Transient(String),  // Network, timeout — will retry
    #[error("permanent error: {0}")]
    Permanent(String),  // Config, serialization — won't retry
}
```

### Sink Implementations

| Sink | Protocol | Target SIEMs |
|------|----------|--------------|
| `SqliteAuditSink` | SQLite | Local DB (primary storage) |
| `TracingAuditSink` | tracing events | OpenTelemetry exporters |
| `SyslogAuditSink` | RFC 5424 UDP/TCP/TLS | Splunk, QRadar, ArcSight, Graylog |
| `HttpAuditSink` | HTTPS POST | Datadog, Sumo Logic, Elastic, custom |
| `JsonStreamSink` | JSON over TCP/UDP | Custom collectors, Logstash |
| `FileAuditSink` | JSONL or CEF files | Offline analysis, compliance archives |

---

## 5. Configuration

### AuditConfig

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct AuditConfig {
    pub enabled: bool,
    pub retention_days: Option<i64>,  // Default: 90
    pub global_filter: AuditFilterConfig,
    pub queue: QueueConfig,
    pub enrichment: EnrichmentConfig,
    pub sinks: SinksConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuditFilterConfig {
    pub min_severity: AuditSeverity,  // Default: Info
    pub include_events: Option<Vec<AuditEventType>>,
    pub exclude_events: Option<Vec<AuditEventType>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    pub capacity: usize,  // Default: 10000
    pub overflow_policy: QueueOverflowPolicy,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum QueueOverflowPolicy {
    DropNewest,  // Default: drop new events when full
    DropOldest,  // Pop oldest, insert new
    Block,       // Backpressure (risk: latency)
}
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LOOM_AUDIT_ENABLED` | Enable audit system | `true` |
| `LOOM_AUDIT_RETENTION_DAYS` | Log retention period | `90` |
| `LOOM_AUDIT_QUEUE_CAPACITY` | Event queue size | `10000` |
| `LOOM_AUDIT_MIN_SEVERITY` | Minimum severity to log | `info` |

### Syslog Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SyslogSinkConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub protocol: SyslogProtocol,  // Udp, Tcp, Tls
    pub facility: String,          // "local0" - "local7", "auth", etc.
    pub app_name: String,          // "loom"
    pub use_cef: bool,             // Format as CEF instead of plain syslog
    pub filter: Option<AuditFilterConfig>,
}
```

| Variable | Description |
|----------|-------------|
| `LOOM_AUDIT_SYSLOG_ENABLED` | Enable syslog sink |
| `LOOM_AUDIT_SYSLOG_HOST` | Syslog server hostname |
| `LOOM_AUDIT_SYSLOG_PORT` | Syslog server port (514) |
| `LOOM_AUDIT_SYSLOG_PROTOCOL` | `udp`, `tcp`, or `tls` |
| `LOOM_AUDIT_SYSLOG_FACILITY` | Syslog facility |
| `LOOM_AUDIT_SYSLOG_APP_NAME` | Application name |

### HTTP Webhook Configuration

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct HttpSinkConfig {
    pub name: String,              // e.g., "datadog"
    pub url: String,               // HTTPS endpoint
    pub method: String,            // "POST"
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,           // Default: 5000
    pub retry_max_attempts: u32,   // Default: 3
    pub batch_size: usize,         // Default: 1 (no batching)
    pub batch_timeout_ms: u64,     // Default: 1000
    pub filter: Option<AuditFilterConfig>,
}
```

| Variable | Description |
|----------|-------------|
| `LOOM_AUDIT_HTTP_<NAME>_URL` | Webhook URL |
| `LOOM_AUDIT_HTTP_<NAME>_HEADERS` | Comma-separated `key:value` pairs |
| `LOOM_AUDIT_HTTP_<NAME>_TIMEOUT_MS` | Request timeout |

---

## 6. SIEM Formats

### RFC 5424 Syslog

```
<PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID [STRUCTURED-DATA] MSG

Example:
<134>1 2026-01-03T12:34:56.789Z loom.example.com loom 12345 LOGIN_FAILED
[audit@loom event_type="login_failed" actor="user-123" ip="192.168.1.1"
resource_type="session" severity="warning"] User login failed: invalid credentials
```

### CEF (Common Event Format)

```
CEF:0|Loom|Loom Server|1.0|LOGIN_FAILED|User login failed|5|
src=192.168.1.1 suser=user-123 msg=Invalid credentials
rt=Jan 03 2026 12:34:56 cs1Label=event_type cs1=login_failed
```

CEF severity mapping:

| AuditSeverity | CEF Severity |
|---------------|--------------|
| Critical | 10 |
| Error | 7 |
| Warning | 5 |
| Notice | 3 |
| Info | 1 |
| Debug | 0 |

### JSON (HTTP/TCP/UDP)

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-01-03T12:34:56.789Z",
  "event_type": "login_failed",
  "severity": "warning",
  "actor_user_id": "user-123",
  "resource_type": "session",
  "action": "User login failed: invalid credentials",
  "ip_address": "192.168.1.1",
  "user_agent": "Mozilla/5.0...",
  "trace_id": "abc123",
  "session": {
    "session_type": "web",
    "geo": {
      "city": "San Francisco",
      "country": "United States",
      "country_code": "US"
    }
  },
  "org": {
    "org_id": "org-456",
    "org_slug": "acme-corp",
    "org_role": "member"
  }
}
```

---

## 7. Pipeline

### AuditService

```rust
pub struct AuditService {
    tx: mpsc::Sender<AuditLogEntry>,
    metrics: AuditMetrics,
}

impl AuditService {
    pub fn new(
        enricher: Arc<dyn AuditEnricher>,
        global_filter: AuditFilterConfig,
        queue_cfg: QueueConfig,
        sinks: Vec<Arc<dyn AuditSink>>,
    ) -> Self;

    /// Non-blocking best-effort logging.
    /// Returns false if queue is full (based on overflow policy).
    pub fn log(&self, entry: AuditLogEntry) -> bool;

    /// Blocking logging for background jobs.
    pub async fn log_blocking(
        &self,
        entry: AuditLogEntry,
    ) -> Result<(), mpsc::error::SendError<AuditLogEntry>>;
}
```

### Enrichment

```rust
#[async_trait]
pub trait AuditEnricher: Send + Sync {
    async fn enrich(&self, event: AuditLogEntry) -> EnrichedAuditEvent;
}

pub struct DefaultEnricher {
    geo_provider: Option<Arc<dyn GeoIpProvider>>,
    session_provider: Option<Arc<dyn SessionContextProvider>>,
    org_provider: Option<Arc<dyn OrgContextProvider>>,
}

#[async_trait]
pub trait GeoIpProvider: Send + Sync {
    async fn lookup(&self, ip: &str) -> Option<GeoIpInfo>;
}

pub struct MaxMindGeoIpProvider {
    reader: maxminddb::Reader<Vec<u8>>,
}
```

---

## 8. Graceful Degradation

### Error Handling

| Error Type | Behavior |
|------------|----------|
| Transient (network, timeout) | Log warning, continue processing |
| Permanent (config, serialization) | Log error, mark sink degraded |
| Queue overflow | Increment metric, drop based on policy |

### Metrics

| Metric | Type | Labels |
|--------|------|--------|
| `audit_events_total` | Counter | `event_type`, `severity` |
| `audit_events_queued` | Gauge | — |
| `audit_events_dropped` | Counter | `reason` |
| `audit_sink_publish_total` | Counter | `sink`, `status` |
| `audit_sink_publish_duration_seconds` | Histogram | `sink` |
| `audit_sink_errors_total` | Counter | `sink`, `error_type` |
| `audit_enrichment_duration_seconds` | Histogram | — |

### Health Checks

Each sink implements optional `health_check()`:
- SQLite: `SELECT 1`
- Syslog: UDP send test or TCP connect
- HTTP: HEAD request to endpoint
- Exposed via `/health` with `audit_sinks` status

---

## 9. Usage

### Handler Usage

```rust
// In route handler
let entry = AuditLogEntry::builder(AuditEventType::AccessDenied)
    .actor(user_id)
    .resource("thread", thread_id.to_string())
    .action("User attempted to access private thread")
    .ip_address(&client_ip)
    .user_agent(&ua)
    .severity(AuditSeverity::Warning)
    .trace_id(tracing::Span::current().context().span().span_context().trace_id().to_string())
    .build();

// Non-blocking, fire-and-forget
state.audit.log(entry);
```

### Server Initialization

```rust
// Build sinks from config
let mut sinks: Vec<Arc<dyn AuditSink>> = Vec::new();

if config.sinks.sqlite.enabled {
    sinks.push(Arc::new(SqliteAuditSink::new(pool.clone(), filter)));
}

if let Some(syslog) = &config.sinks.syslog {
    if syslog.enabled {
        sinks.push(Arc::new(SyslogAuditSink::new(syslog.clone())?));
    }
}

// Build enricher
let enricher = Arc::new(DefaultEnricher::new(
    geo_provider,
    session_provider,
    org_provider,
));

// Create service
let audit_service = AuditService::new(
    enricher,
    config.global_filter.clone(),
    config.queue.clone(),
    sinks,
);

// Add to app state
app_state.audit = Arc::new(audit_service);
```

---

## 10. Database Schema

### Migration: `XXX_audit_enrichment.sql`

```sql
-- Add enrichment columns to audit_logs table
ALTER TABLE audit_logs ADD COLUMN severity TEXT DEFAULT 'info';
ALTER TABLE audit_logs ADD COLUMN trace_id TEXT;
ALTER TABLE audit_logs ADD COLUMN span_id TEXT;
ALTER TABLE audit_logs ADD COLUMN request_id TEXT;
ALTER TABLE audit_logs ADD COLUMN session_context TEXT;  -- JSON
ALTER TABLE audit_logs ADD COLUMN org_context TEXT;      -- JSON

-- Index for severity filtering
CREATE INDEX IF NOT EXISTS idx_audit_logs_severity ON audit_logs(severity);

-- Index for trace correlation
CREATE INDEX IF NOT EXISTS idx_audit_logs_trace_id ON audit_logs(trace_id);
```

---

## 11. Implementation Phases

### Phase 1: Extract Core Types (1-2 hours)

- [ ] Create `loom-server-audit` crate
- [ ] Move `AuditEventType`, `AuditLogEntry`, `AuditLogBuilder` from `loom-server-auth`
- [ ] Add `AuditSeverity` with default mappings
- [ ] Add correlation fields (`trace_id`, `span_id`, `request_id`)
- [ ] Update `loom-server-auth` to re-export from `loom-server-audit`

### Phase 2: Pipeline & SQLite Sink (2-3 hours)

- [ ] Implement `AuditSink` trait
- [ ] Implement `AuditService` with mpsc queue
- [ ] Implement `SqliteAuditSink` (migrate from `AuditRepository`)
- [ ] Implement `AuditFilterConfig`
- [ ] Wire into `loom-server` app state
- [ ] Migrate handlers to use `AuditService::log()`

### Phase 3: Tracing Sink (1 hour)

- [ ] Implement `TracingAuditSink`
- [ ] Map `AuditSeverity` to `tracing::Level`
- [ ] Emit structured events for OpenTelemetry

### Phase 4: Enrichment (2-3 hours)

- [ ] Define `AuditEnricher` trait
- [ ] Implement `DefaultEnricher`
- [ ] Add `SessionContext` and `OrgContext` extraction
- [ ] Integrate `MaxMindGeoIpProvider` (optional feature)
- [ ] Add enrichment columns to database schema

### Phase 5: Syslog Sink (2-3 hours)

- [ ] Implement RFC 5424 message formatting
- [ ] Implement UDP transport
- [ ] Implement TCP transport
- [ ] Implement TLS transport (optional feature)
- [ ] Add CEF formatting option

### Phase 6: HTTP Webhook Sink (2-3 hours)

- [ ] Implement `HttpAuditSink`
- [ ] Add retry with exponential backoff
- [ ] Add configurable headers
- [ ] Add optional batching

### Phase 7: JSON Stream Sink (1-2 hours)

- [ ] Implement TCP transport
- [ ] Implement UDP transport
- [ ] Add reconnection logic for TCP

### Phase 8: File Sink (1-2 hours)

- [ ] Implement JSONL file output
- [ ] Implement CEF file output
- [ ] Add daily rotation

### Phase 9: Metrics & Health (1-2 hours)

- [ ] Add Prometheus metrics
- [ ] Add per-sink health checks
- [ ] Integrate with `/health` endpoint

### Phase 10: Configuration & Documentation (1-2 hours)

- [ ] Implement environment variable loading
- [ ] Add TOML configuration support
- [ ] Document SIEM integration guides

---

## 12. Rust Dependencies

```toml
[dependencies]
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["sync", "net", "io-util"] }
tracing = "0.1"
uuid = { version = "1", features = ["v4", "serde"] }

[dependencies.loom-common-http]
path = "../loom-common-http"

[dependencies.loom-common-secret]
path = "../loom-common-secret"

[dev-dependencies]
proptest = "1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

[features]
default = ["sink-sqlite", "sink-tracing"]
sink-sqlite = ["sqlx"]
sink-tracing = []
sink-syslog = []
sink-http = []
sink-json-stream = []
sink-file = []
geo-ip = ["maxminddb"]
```

---

## 13. Security Considerations

### Secret Handling

- Audit events should never contain plaintext secrets or tokens
- Use `loom-common-secret::SecretString` for any sensitive fields
- Redact API keys, session tokens, and passwords in `details` JSON

### Transport Security

- HTTP sinks should use HTTPS only in production
- Syslog TLS should verify certificates
- TCP streams should support TLS upgrade

### Access Control

- Audit log query API requires `SystemAdmin` or `Auditor` role
- Per-org audit logs filtered by org membership
- Audit logs themselves are write-only (no update/delete API)

### Retention & Compliance

- Default 90-day retention (configurable)
- Cleanup job runs daily to purge old logs
- Support for legal hold (disable cleanup for specific events)

---

## Appendix A: SIEM Integration Guides

### Splunk

```toml
[sinks.syslog]
enabled = true
host = "splunk-hec.example.com"
port = 514
protocol = "tcp"
facility = "local0"
app_name = "loom"
```

Or use HTTP Event Collector:

```toml
[[sinks.http]]
name = "splunk"
url = "https://splunk-hec.example.com:8088/services/collector/event"
method = "POST"
headers = [["Authorization", "Splunk <HEC-TOKEN>"]]
```

### Datadog

```toml
[[sinks.http]]
name = "datadog"
url = "https://http-intake.logs.datadoghq.com/api/v2/logs"
method = "POST"
headers = [["DD-API-KEY", "<API-KEY>"], ["Content-Type", "application/json"]]
```

### Elastic / Logstash

```toml
[[sinks.json_stream]]
name = "logstash"
host = "logstash.example.com"
port = 5044
protocol = "tcp"
```

### QRadar / ArcSight

Use syslog with CEF format:

```toml
[sinks.syslog]
enabled = true
host = "qradar.example.com"
port = 514
protocol = "udp"
facility = "auth"
use_cef = true
```

---

## Appendix B: Future Considerations

| Feature | Description |
|---------|-------------|
| Kafka/NATS integration | For high-volume, distributed audit collection |
| Transactional outbox | Guaranteed delivery via database CDC |
| Real-time alerting | Integration with PagerDuty, Opsgenie |
| Audit log search UI | Web interface for querying logs |
| Legal hold | Prevent deletion of specific events |
| Audit log export | Bulk export for compliance audits |
