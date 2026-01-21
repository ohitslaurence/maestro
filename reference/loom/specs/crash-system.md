<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Crash Analytics System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-18

---

## 1. Overview

### Purpose

This specification defines a crash analytics system for Loom. Products built on Loom can capture errors and crashes, track issues across releases, detect regressions, and view source context via uploaded source maps. The system integrates with existing analytics (identity resolution) and feature flags (active flags at crash time).

### Goals

- **Crash capture** for TypeScript (browser + Node) and Rust applications
- **Issue grouping** via fingerprinting to deduplicate similar crashes
- **Issue lifecycle** with unresolved/resolved/ignored/regressed states
- **Regression detection** when resolved issues reappear in new releases
- **Source map uploads** for TypeScript with source context extraction
- **Rust panic capture** with backtrace symbolication
- **Integration** with `loom-analytics` (person identity) and `loom-flags` (active flags)
- **Real-time updates** via SSE streaming for new crashes and regressions
- **Web UI** with Storybook component library

### Non-Goals (v1)

- iOS dSYM symbolication — deferred to future version
- Native C/C++ crash capture — deferred
- Session replay integration — out of scope
- Alerting (email/Slack/webhooks) — deferred to future version
- Performance monitoring (transactions/spans) — separate system
- Custom fingerprinting rules UI — API only for v1

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-crash-core/                  # Shared types for crash analytics
│   ├── src/
│   │   ├── lib.rs
│   │   ├── event.rs                  # CrashEvent, Stacktrace, Frame
│   │   ├── issue.rs                  # Issue, IssueStatus, fingerprinting
│   │   ├── symbol.rs                 # SymbolArtifact, ArtifactType
│   │   ├── release.rs                # Release tracking
│   │   └── error.rs                  # Error types
│   └── Cargo.toml
├── loom-crash/                       # Rust SDK client
│   ├── src/
│   │   ├── lib.rs
│   │   ├── client.rs                 # CrashClient
│   │   ├── panic_hook.rs             # std::panic::set_hook integration
│   │   ├── backtrace.rs              # Backtrace capture and parsing
│   │   ├── context.rs                # Crash context (tags, extra, flags)
│   │   └── transport.rs              # HTTP transport with batching
│   └── Cargo.toml
├── loom-crash-symbolicate/           # Symbolication engine
│   ├── src/
│   │   ├── lib.rs
│   │   ├── sourcemap.rs              # JavaScript/TypeScript source maps
│   │   ├── vlq.rs                    # VLQ decoder for source maps
│   │   ├── rust.rs                   # Rust symbol demangling
│   │   └── cache.rs                  # Symbolication cache
│   └── Cargo.toml
├── loom-server-crash/                # Server-side API and storage
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs                 # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── capture.rs            # Crash ingestion endpoint
│   │   │   ├── symbols.rs            # Symbol artifact upload
│   │   │   ├── issues.rs             # Issue CRUD and state transitions
│   │   │   ├── events.rs             # Crash event queries
│   │   │   └── releases.rs           # Release management
│   │   ├── repository.rs             # Database repository
│   │   ├── fingerprint.rs            # Issue fingerprinting logic
│   │   ├── symbolicate.rs            # Symbolication pipeline
│   │   ├── sse.rs                    # SSE streaming for updates
│   │   └── api_key.rs                # Crash API key management
│   └── Cargo.toml

web/
├── packages/
│   └── crash/                        # @loom/crash - TypeScript SDK
│       ├── src/
│       │   ├── index.ts
│       │   ├── client.ts             # CrashClient
│       │   ├── error-boundary.tsx    # React error boundary
│       │   ├── global-handler.ts     # window.onerror, unhandledrejection
│       │   ├── stacktrace.ts         # Stack trace parsing
│       │   ├── context.ts            # Crash context helpers
│       │   └── transport.ts          # HTTP transport with batching
│       └── package.json
├── loom-web/
│   └── src/
│       └── lib/
│           └── components/
│               └── crash/            # Crash UI components
│                   ├── IssueList.svelte
│                   ├── IssueDetail.svelte
│                   ├── CrashEventCard.svelte
│                   ├── Stacktrace.svelte
│                   ├── SourceContext.svelte
│                   ├── IssueStatusBadge.svelte
│                   ├── RegressionAlert.svelte
│                   └── SymbolUpload.svelte
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Clients                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│    Product App (Web)           Product Backend           loom-cli            │
│    (@loom/crash)               (loom-crash)              (loom-crash)        │
│    + @loom/analytics           + loom-analytics          Panic hook          │
│    + @loom/flags               + loom-flags                                  │
└────────┬─────────────────────────┬──────────────────────────┬───────────────┘
         │                         │                          │
         └─────────────────────────┼──────────────────────────┘
                                   │
                                   ▼ REST API
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │ Crash Routes    │  │ API Key Auth    │  │ Fingerprint     │              │
│  │ /api/crash/*    │  │ Middleware      │  │ Engine          │              │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘              │
│           │                    │                    │                        │
│           └────────────────────┼────────────────────┘                        │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                     Symbolication Pipeline                            │   │
│  │    Source map lookup → VLQ decode → Frame resolution → Context       │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                │                                             │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                         SSE Broadcaster                               │   │
│  │    Real-time crash events, issue state changes, regressions          │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                │                                             │
│                                ▼                                             │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                    Integration Layer                                  │   │
│  │    loom-analytics (person_id) ←→ loom-flags (active_flags)           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  crash_events    crash_issues    symbol_artifacts    crash_releases         │
│  crash_api_keys  crash_issue_assignees  crash_comments                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Entities

### 3.1 CrashEvent

A single crash occurrence captured by the SDK.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashEvent {
    pub id: CrashEventId,
    pub org_id: OrgId,
    pub project_id: ProjectId,
    pub issue_id: Option<IssueId>,           // Assigned after fingerprinting

    // Identity (from loom-analytics integration)
    pub person_id: Option<PersonId>,
    pub distinct_id: String,

    // Error information
    pub exception_type: String,               // "TypeError", "panic", "Error"
    pub exception_value: String,              // Error message
    pub stacktrace: Stacktrace,
    pub raw_stacktrace: Option<Stacktrace>,   // Pre-symbolication (minified)

    // Environment context
    pub release: Option<String>,              // Semantic version or commit SHA
    pub dist: Option<String>,                 // Distribution variant
    pub environment: String,                  // "production", "staging", "dev"
    pub platform: Platform,
    pub runtime: Option<Runtime>,
    pub server_name: Option<String>,

    // Custom context
    pub tags: HashMap<String, String>,
    pub extra: serde_json::Value,
    pub user_context: Option<UserContext>,
    pub device_context: Option<DeviceContext>,
    pub browser_context: Option<BrowserContext>,
    pub os_context: Option<OsContext>,

    // Feature flags active at crash time (from loom-flags integration)
    pub active_flags: HashMap<String, String>, // flag_key -> variant

    // Request context (for server-side crashes)
    pub request: Option<RequestContext>,

    // Breadcrumbs (events leading up to crash)
    pub breadcrumbs: Vec<Breadcrumb>,

    // Timestamps
    pub timestamp: DateTime<Utc>,             // When crash occurred
    pub received_at: DateTime<Utc>,           // When server received it
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stacktrace {
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub function: Option<String>,             // Function/method name
    pub module: Option<String>,               // Module/crate/package
    pub filename: Option<String>,             // Relative filename
    pub abs_path: Option<String>,             // Absolute path (if available)
    pub lineno: Option<u32>,
    pub colno: Option<u32>,

    // Source context (populated after symbolication)
    pub context_line: Option<String>,         // The actual source line
    pub pre_context: Vec<String>,             // 5 lines before
    pub post_context: Vec<String>,            // 5 lines after

    pub in_app: bool,                         // User code vs dependency
    pub instruction_addr: Option<String>,     // For native code
    pub symbol_addr: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    JavaScript,      // Browser
    Node,            // Node.js
    Rust,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    pub name: String,                         // "node", "browser", "rustc"
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub id: Option<String>,
    pub email: Option<String>,
    pub username: Option<String>,
    pub ip_address: Option<Secret<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceContext {
    pub name: Option<String>,
    pub family: Option<String>,
    pub model: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserContext {
    pub name: Option<String>,                 // "Chrome", "Firefox"
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsContext {
    pub name: Option<String>,                 // "Windows", "macOS", "Linux"
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub url: Option<String>,
    pub method: Option<String>,
    pub headers: HashMap<String, String>,
    pub query_string: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breadcrumb {
    pub timestamp: DateTime<Utc>,
    pub category: String,                     // "http", "navigation", "ui", "console"
    pub message: Option<String>,
    pub level: BreadcrumbLevel,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BreadcrumbLevel {
    Debug,
    Info,
    Warning,
    Error,
}
```

### 3.2 Issue

An aggregated group of similar crash events.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub org_id: OrgId,
    pub project_id: ProjectId,

    // Identification
    pub short_id: String,                     // Human-readable: "PROJ-123"
    pub fingerprint: String,                  // SHA256 hash for grouping

    // Display
    pub title: String,                        // Exception type + first line of message
    pub culprit: Option<String>,              // Top in-app frame function
    pub metadata: IssueMetadata,

    // Status
    pub status: IssueStatus,
    pub level: IssueLevel,
    pub priority: IssuePriority,

    // Counts
    pub event_count: u64,
    pub user_count: u64,                      // Unique person_ids affected

    // Timeline
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,

    // Resolution tracking
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<UserId>,
    pub resolved_in_release: Option<String>,  // "fixed in 1.2.4"

    // Regression tracking
    pub times_regressed: u32,
    pub last_regressed_at: Option<DateTime<Utc>>,
    pub regressed_in_release: Option<String>,

    // Assignment
    pub assigned_to: Option<UserId>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueMetadata {
    pub exception_type: String,
    pub exception_value: String,
    pub filename: Option<String>,
    pub function: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IssueStatus {
    Unresolved,
    Resolved,
    Ignored,
    Regressed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IssueLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IssuePriority {
    High,
    Medium,
    Low,
}
```

### 3.3 SymbolArtifact

Uploaded debug artifacts for symbolication.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolArtifact {
    pub id: SymbolArtifactId,
    pub org_id: OrgId,
    pub project_id: ProjectId,

    pub release: String,                      // Version this applies to
    pub dist: Option<String>,                 // Distribution variant
    pub artifact_type: ArtifactType,
    pub name: String,                         // e.g., "main.js.map", "app.min.js"

    // Storage
    pub data: Vec<u8>,                        // Blob content (SQLite BLOB)
    pub size_bytes: u64,
    pub sha256: String,                       // For deduplication

    // Metadata for source maps
    pub source_map_url: Option<String>,       // //# sourceMappingURL value
    pub sources_content: bool,                // Whether sourcesContent is embedded

    pub uploaded_at: DateTime<Utc>,
    pub uploaded_by: UserId,
    pub last_accessed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactType {
    SourceMap,           // .map files
    MinifiedSource,      // Minified .js files (for URL matching)
    RustDebugInfo,       // Future: Rust debug symbols
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMapMetadata {
    pub version: u32,                         // Should be 3
    pub file: Option<String>,
    pub source_root: Option<String>,
    pub sources: Vec<String>,
    pub names: Vec<String>,
    pub has_sources_content: bool,
}
```

### 3.4 Release

Release tracking for crash correlation.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id: ReleaseId,
    pub org_id: OrgId,
    pub project_id: ProjectId,

    pub version: String,                      // Semantic version or commit SHA
    pub short_version: Option<String>,        // Display version
    pub url: Option<String>,                  // Link to release notes/commit

    // Stats
    pub crash_count: u64,
    pub new_issue_count: u64,
    pub regression_count: u64,
    pub user_count: u64,

    // Timestamps
    pub date_released: Option<DateTime<Utc>>,
    pub first_event: Option<DateTime<Utc>>,
    pub last_event: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
}
```

### 3.5 Project

Crash analytics project within an organization.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashProject {
    pub id: ProjectId,
    pub org_id: OrgId,
    pub name: String,
    pub slug: String,                         // URL-safe identifier
    pub platform: Platform,

    // Settings
    pub auto_resolve_age_days: Option<u32>,   // Auto-resolve after N days inactive
    pub fingerprint_rules: Vec<FingerprintRule>, // Custom fingerprinting

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintRule {
    pub match_type: FingerprintMatchType,
    pub pattern: String,
    pub fingerprint: Vec<String>,             // Custom fingerprint components
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FingerprintMatchType {
    ExceptionType,
    ExceptionMessage,
    Module,
    Function,
}
```

### 3.6 Crash API Key

Authentication for SDK clients (follows analytics/flags pattern).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashApiKey {
    pub id: CrashApiKeyId,
    pub project_id: ProjectId,
    pub name: String,
    pub key_type: CrashKeyType,
    pub key_hash: String,                     // Argon2 hash
    pub rate_limit_per_minute: Option<u32>,
    pub allowed_origins: Vec<String>,         // CORS for browser SDKs
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrashKeyType {
    Capture,      // Can only send crashes (safe for client-side)
    Admin,        // Can manage symbols, issues, settings
}
```

---

## 4. Fingerprinting

### 4.1 Default Fingerprinting Algorithm

Issues are grouped by fingerprint. The default algorithm:

```rust
pub fn compute_fingerprint(event: &CrashEvent) -> String {
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();

    // 1. Exception type (most significant)
    hasher.update(event.exception_type.as_bytes());
    hasher.update(b"|");

    // 2. Top N in-app frames (function + module)
    let in_app_frames: Vec<_> = event.stacktrace.frames
        .iter()
        .filter(|f| f.in_app)
        .take(5)
        .collect();

    for frame in &in_app_frames {
        if let Some(func) = &frame.function {
            hasher.update(func.as_bytes());
        }
        hasher.update(b"@");
        if let Some(module) = &frame.module {
            hasher.update(module.as_bytes());
        }
        hasher.update(b"|");
    }

    // 3. If no in-app frames, use all frames
    if in_app_frames.is_empty() {
        for frame in event.stacktrace.frames.iter().take(5) {
            if let Some(func) = &frame.function {
                hasher.update(func.as_bytes());
            }
            hasher.update(b"|");
        }
    }

    hex::encode(hasher.finalize())
}
```

### 4.2 Fingerprint Components

| Priority | Component | Why |
|----------|-----------|-----|
| 1 | Exception type | Groups similar error types |
| 2 | In-app function names | Identifies the failing code |
| 3 | Module names | Distinguishes same function in different modules |
| 4 | All function names | Fallback when no in-app frames |

### 4.3 Custom Fingerprinting (API)

Projects can define custom fingerprint rules:

```json
{
  "fingerprint_rules": [
    {
      "match_type": "exception_message",
      "pattern": "rate limit exceeded.*",
      "fingerprint": ["rate-limit-error"]
    },
    {
      "match_type": "module",
      "pattern": "third_party::*",
      "fingerprint": ["third-party-error", "{{ module }}"]
    }
  ]
}
```

---

## 5. Issue Lifecycle

### 5.1 State Transitions

```
                    ┌─────────────────────────────┐
                    │                             │
                    ▼                             │
┌──────────────┐  resolve   ┌──────────────┐      │
│  Unresolved  │ ────────►  │   Resolved   │      │
└──────────────┘            └──────────────┘      │
       │                           │              │
       │ ignore                    │ new crash    │
       │                           │ (regression) │
       ▼                           ▼              │
┌──────────────┐            ┌──────────────┐      │
│   Ignored    │            │  Regressed   │ ─────┘
└──────────────┘            └──────────────┘   resolve
       │                           │
       │ unignore                  │ ignore
       │                           │
       └───────────────────────────┘
```

### 5.2 Regression Detection

When a new crash arrives:

```rust
async fn process_crash(event: CrashEvent) -> Result<(Issue, bool)> {
    let fingerprint = compute_fingerprint(&event);

    match find_issue_by_fingerprint(&fingerprint).await? {
        Some(mut issue) => {
            let is_regression = issue.status == IssueStatus::Resolved;

            // Update counts
            issue.event_count += 1;
            issue.last_seen = event.timestamp;

            // Update user count
            if let Some(person_id) = &event.person_id {
                if !issue_has_person(&issue.id, person_id).await? {
                    issue.user_count += 1;
                    add_issue_person(&issue.id, person_id).await?;
                }
            }

            // Regression check
            if is_regression {
                issue.status = IssueStatus::Regressed;
                issue.times_regressed += 1;
                issue.last_regressed_at = Some(Utc::now());
                issue.regressed_in_release = event.release.clone();

                // Broadcast regression event via SSE
                broadcast_regression(&issue).await?;
            }

            update_issue(&issue).await?;
            Ok((issue, is_regression))
        }
        None => {
            let issue = create_issue(Issue {
                fingerprint,
                title: format!("{}: {}", event.exception_type,
                    truncate(&event.exception_value, 100)),
                culprit: find_culprit(&event.stacktrace),
                status: IssueStatus::Unresolved,
                event_count: 1,
                user_count: if event.person_id.is_some() { 1 } else { 0 },
                first_seen: event.timestamp,
                last_seen: event.timestamp,
                ..Default::default()
            }).await?;

            // Broadcast new issue via SSE
            broadcast_new_issue(&issue).await?;

            Ok((issue, false))
        }
    }
}

fn find_culprit(stacktrace: &Stacktrace) -> Option<String> {
    stacktrace.frames
        .iter()
        .find(|f| f.in_app)
        .and_then(|f| f.function.clone())
}
```

### 5.3 Resolution

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveRequest {
    pub resolved_in_release: Option<String>,  // "1.2.4"
}

async fn resolve_issue(
    issue_id: IssueId,
    user_id: UserId,
    request: ResolveRequest,
) -> Result<Issue> {
    let mut issue = get_issue(&issue_id).await?;

    issue.status = IssueStatus::Resolved;
    issue.resolved_at = Some(Utc::now());
    issue.resolved_by = Some(user_id);
    issue.resolved_in_release = request.resolved_in_release;

    update_issue(&issue).await?;
    broadcast_issue_resolved(&issue).await?;

    Ok(issue)
}
```

---

## 6. Symbolication

### 6.1 Source Map Processing

```rust
pub struct SourceMapProcessor {
    cache: SymbolCache,
}

impl SourceMapProcessor {
    /// Symbolicate a JavaScript/TypeScript stacktrace
    pub async fn symbolicate_js(
        &self,
        stacktrace: &Stacktrace,
        release: &str,
        project_id: &ProjectId,
    ) -> Result<Stacktrace> {
        let mut symbolicated = stacktrace.clone();

        for frame in &mut symbolicated.frames {
            if let (Some(filename), Some(lineno), Some(colno)) =
                (&frame.filename, frame.lineno, frame.colno)
            {
                // Find source map for this file
                if let Some(source_map) = self.find_source_map(
                    project_id, release, filename
                ).await? {
                    // Decode and lookup position
                    if let Some(original) = source_map.lookup(lineno, colno)? {
                        frame.filename = Some(original.source.clone());
                        frame.lineno = Some(original.line);
                        frame.colno = Some(original.column);
                        frame.function = original.name;

                        // Extract source context if available
                        if let Some(content) = original.source_content {
                            let (pre, line, post) = extract_context(
                                &content,
                                original.line as usize,
                                5  // context lines
                            );
                            frame.pre_context = pre;
                            frame.context_line = Some(line);
                            frame.post_context = post;
                        }
                    }
                }
            }
        }

        Ok(symbolicated)
    }
}
```

### 6.2 Source Map Lookup

```rust
pub struct ParsedSourceMap {
    version: u32,
    sources: Vec<String>,
    sources_content: Vec<Option<String>>,
    names: Vec<String>,
    mappings: DecodedMappings,
}

pub struct OriginalPosition {
    pub source: String,
    pub line: u32,
    pub column: u32,
    pub name: Option<String>,
    pub source_content: Option<String>,
}

impl ParsedSourceMap {
    /// Load and parse a source map from blob storage
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let raw: RawSourceMap = serde_json::from_slice(data)?;

        // Decode VLQ mappings
        let mappings = decode_vlq_mappings(&raw.mappings)?;

        Ok(Self {
            version: raw.version,
            sources: raw.sources,
            sources_content: raw.sources_content.unwrap_or_default(),
            names: raw.names,
            mappings,
        })
    }

    /// Lookup original position for generated line/column
    pub fn lookup(&self, line: u32, column: u32) -> Result<Option<OriginalPosition>> {
        // Binary search in mappings for closest match
        if let Some(mapping) = self.mappings.find(line, column) {
            let source = self.sources.get(mapping.source_index as usize)
                .ok_or_else(|| Error::InvalidSourceIndex)?
                .clone();

            let source_content = self.sources_content
                .get(mapping.source_index as usize)
                .and_then(|c| c.clone());

            let name = mapping.name_index
                .and_then(|i| self.names.get(i as usize).cloned());

            Ok(Some(OriginalPosition {
                source,
                line: mapping.original_line,
                column: mapping.original_column,
                name,
                source_content,
            }))
        } else {
            Ok(None)
        }
    }
}
```

### 6.3 VLQ Decoding

```rust
/// Decode VLQ-encoded source map mappings
pub fn decode_vlq_mappings(mappings: &str) -> Result<DecodedMappings> {
    let mut result = DecodedMappings::new();
    let mut generated_line = 0u32;

    // State for relative decoding
    let mut prev_source = 0i32;
    let mut prev_original_line = 0i32;
    let mut prev_original_column = 0i32;
    let mut prev_name = 0i32;

    for line in mappings.split(';') {
        let mut generated_column = 0u32;

        for segment in line.split(',') {
            if segment.is_empty() {
                continue;
            }

            let values = decode_vlq_segment(segment)?;

            generated_column = (generated_column as i32 + values[0]) as u32;

            if values.len() >= 4 {
                prev_source += values[1];
                prev_original_line += values[2];
                prev_original_column += values[3];

                let name_index = if values.len() >= 5 {
                    prev_name += values[4];
                    Some(prev_name as u32)
                } else {
                    None
                };

                result.add(Mapping {
                    generated_line,
                    generated_column,
                    source_index: prev_source as u32,
                    original_line: prev_original_line as u32,
                    original_column: prev_original_column as u32,
                    name_index,
                });
            }
        }

        generated_line += 1;
    }

    Ok(result)
}

fn decode_vlq_segment(segment: &str) -> Result<Vec<i32>> {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut values = Vec::new();
    let mut value = 0i32;
    let mut shift = 0;

    for ch in segment.bytes() {
        let digit = BASE64_CHARS.iter().position(|&c| c == ch)
            .ok_or_else(|| Error::InvalidVlqChar(ch as char))? as i32;

        let continuation = digit & 0b100000 != 0;
        let digit_value = digit & 0b011111;

        value += digit_value << shift;
        shift += 5;

        if !continuation {
            // Convert from sign-magnitude to two's complement
            let negated = value & 1 != 0;
            value >>= 1;
            if negated {
                value = -value;
            }
            values.push(value);
            value = 0;
            shift = 0;
        }
    }

    Ok(values)
}
```

### 6.4 Rust Symbol Demangling

```rust
use rustc_demangle::demangle;

pub fn symbolicate_rust_frame(frame: &mut Frame) {
    if let Some(func) = &frame.function {
        // Demangle Rust symbols
        let demangled = demangle(func).to_string();
        frame.function = Some(demangled);

        // Extract module from demangled name
        // e.g., "loom_server::handlers::crash::capture" -> "loom_server::handlers::crash"
        if let Some(last_sep) = demangled.rfind("::") {
            frame.module = Some(demangled[..last_sep].to_string());
        }
    }
}
```

---

## 7. Source Map Upload

### 7.1 Upload API

```rust
#[derive(Debug, Deserialize)]
pub struct UploadArtifactRequest {
    pub release: String,
    pub dist: Option<String>,
    // File content in multipart form
}

async fn upload_artifacts(
    State(state): State<AppState>,
    Path(project_id): Path<ProjectId>,
    auth: CrashAdminAuth,
    mut multipart: Multipart,
) -> Result<Json<Vec<SymbolArtifact>>, Error> {
    let mut artifacts = Vec::new();

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or("").to_string();
        let filename = field.file_name().unwrap_or("").to_string();
        let data = field.bytes().await?;

        // Determine artifact type
        let artifact_type = if filename.ends_with(".map") {
            ArtifactType::SourceMap
        } else if filename.ends_with(".js") {
            ArtifactType::MinifiedSource
        } else {
            continue; // Skip unknown types
        };

        // Parse and validate source map
        let metadata = if artifact_type == ArtifactType::SourceMap {
            Some(validate_source_map(&data)?)
        } else {
            None
        };

        let sha256 = compute_sha256(&data);

        // Check for duplicate
        if let Some(existing) = find_artifact_by_hash(&project_id, &sha256).await? {
            artifacts.push(existing);
            continue;
        }

        let artifact = SymbolArtifact {
            id: SymbolArtifactId::new(),
            org_id: auth.org_id.clone(),
            project_id: project_id.clone(),
            release: name.clone(), // From form field
            dist: None,
            artifact_type,
            name: filename,
            data: data.to_vec(),
            size_bytes: data.len() as u64,
            sha256,
            source_map_url: metadata.as_ref().and_then(|m| m.file.clone()),
            sources_content: metadata.map(|m| m.has_sources_content).unwrap_or(false),
            uploaded_at: Utc::now(),
            uploaded_by: auth.user_id.clone(),
            last_accessed_at: None,
        };

        save_artifact(&artifact).await?;
        artifacts.push(artifact);
    }

    Ok(Json(artifacts))
}
```

### 7.2 CLI Upload Command

```bash
# Upload source maps for a release
loom crash upload-sourcemaps \
  --project my-app \
  --release 1.2.3 \
  --include ./dist/**/*.map \
  --include ./dist/**/*.js
```

### 7.3 Build Tool Integration

**Vite plugin example:**

```typescript
// vite.config.ts
import { loomSourceMaps } from '@loom/crash-vite';

export default {
  plugins: [
    loomSourceMaps({
      project: 'my-app',
      release: process.env.GIT_SHA,
      apiKey: process.env.LOOM_CRASH_API_KEY,
    }),
  ],
};
```

---

## 8. SDK Design

### 8.1 Rust SDK (`loom-crash`)

```rust
use loom_crash::{CrashClient, CrashContext};
use loom_analytics::AnalyticsClient;
use loom_flags::FlagsClient;

// Initialize with integration
let crash = CrashClient::builder()
    .api_key("loom_crash_xxx")
    .base_url("https://loom.example.com")
    .project("my-rust-app")
    .release(env!("CARGO_PKG_VERSION"))
    // Integrate with analytics for person identity
    .analytics(&analytics_client)
    // Integrate with flags to capture active flags
    .flags(&flags_client)
    .build()?;

// Install panic hook (captures all panics)
crash.install_panic_hook();

// Set user context
crash.set_user(UserContext {
    id: Some(user.id.to_string()),
    email: Some(user.email.clone()),
    ..Default::default()
});

// Add global tags
crash.set_tag("environment", "production");
crash.set_tag("server", hostname);

// Capture breadcrumb
crash.add_breadcrumb(Breadcrumb {
    category: "http".into(),
    message: Some("GET /api/users".into()),
    level: BreadcrumbLevel::Info,
    ..Default::default()
});

// Manual capture
if let Err(e) = do_something() {
    crash.capture_error(&e);
}

// Scoped context
crash.with_scope(|scope| {
    scope.set_tag("handler", "create_user");
    scope.set_extra("request_id", json!(request_id));

    // Errors in this scope have these tags
    process_request()?;
    Ok(())
})?;

// Shutdown (flushes pending events)
crash.shutdown().await?;
```

### 8.2 Panic Hook Implementation

```rust
impl CrashClient {
    pub fn install_panic_hook(&self) {
        let client = self.clone();

        let default_hook = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |info| {
            // Capture backtrace
            let backtrace = std::backtrace::Backtrace::force_capture();

            // Extract panic message
            let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "Box<dyn Any>".to_string()
            };

            // Extract location
            let location = info.location().map(|l| format!(
                "{}:{}:{}",
                l.file(),
                l.line(),
                l.column()
            ));

            // Build crash event
            let event = CrashEvent {
                exception_type: "panic".into(),
                exception_value: message,
                stacktrace: parse_backtrace(&backtrace),
                platform: Platform::Rust,
                tags: client.global_tags.read().clone(),
                extra: client.global_extra.read().clone(),
                active_flags: client.get_active_flags(),
                person_id: client.get_person_id(),
                distinct_id: client.get_distinct_id(),
                ..Default::default()
            };

            // Send synchronously (we're panicking, async won't complete)
            let _ = client.send_sync(&event);

            // Call default hook
            default_hook(info);
        }));
    }
}
```

### 8.3 TypeScript SDK (`@loom/crash`)

```typescript
import { CrashClient } from '@loom/crash';
import { AnalyticsClient } from '@loom/analytics';
import { FlagsClient } from '@loom/flags';

const crash = new CrashClient({
  apiKey: 'loom_crash_xxx',
  baseUrl: 'https://loom.example.com',
  project: 'my-web-app',
  release: '1.2.3',
  environment: 'production',

  // Integrate with other SDKs
  analytics,
  flags,

  // Options
  maxBreadcrumbs: 100,
  beforeSend: (event) => {
    // Filter or modify events before sending
    if (event.exception_value.includes('ResizeObserver')) {
      return null; // Drop this event
    }
    return event;
  },
});

// Install global handlers
crash.installGlobalHandler();

// Set user context
crash.setUser({
  id: user.id,
  email: user.email,
});

// Add breadcrumb
crash.addBreadcrumb({
  category: 'navigation',
  message: 'User navigated to /dashboard',
  level: 'info',
});

// Manual capture
try {
  await riskyOperation();
} catch (error) {
  crash.captureException(error, {
    tags: { operation: 'risky' },
    extra: { attemptNumber: 3 },
  });
}

// React Error Boundary
import { CrashBoundary } from '@loom/crash/react';

function App() {
  return (
    <CrashBoundary
      client={crash}
      fallback={<ErrorPage />}
      onError={(error, errorInfo) => {
        console.log('Crash captured:', error);
      }}
    >
      <MyApp />
    </CrashBoundary>
  );
}
```

### 8.4 Global Handler (Browser)

```typescript
export function installGlobalHandler(client: CrashClient) {
  // Uncaught errors
  window.addEventListener('error', (event) => {
    client.captureException(event.error || event.message, {
      mechanism: {
        type: 'onerror',
        handled: false,
      },
    });
  });

  // Unhandled promise rejections
  window.addEventListener('unhandledrejection', (event) => {
    client.captureException(event.reason, {
      mechanism: {
        type: 'onunhandledrejection',
        handled: false,
      },
    });
  });

  // Console errors (optional)
  const originalConsoleError = console.error;
  console.error = (...args) => {
    client.addBreadcrumb({
      category: 'console',
      message: args.map(String).join(' '),
      level: 'error',
    });
    originalConsoleError.apply(console, args);
  };
}
```

### 8.5 Stack Trace Parsing (JavaScript)

```typescript
interface ParsedFrame {
  function?: string;
  filename?: string;
  lineno?: number;
  colno?: number;
  in_app: boolean;
}

export function parseStackTrace(error: Error): ParsedFrame[] {
  if (!error.stack) return [];

  const frames: ParsedFrame[] = [];
  const lines = error.stack.split('\n');

  for (const line of lines) {
    // Chrome/V8 format: "    at functionName (filename:line:col)"
    const chromeMatch = line.match(
      /^\s*at\s+(?:(.+?)\s+\()?(?:(.+?):(\d+):(\d+))\)?$/
    );

    if (chromeMatch) {
      frames.push({
        function: chromeMatch[1] || '<anonymous>',
        filename: chromeMatch[2],
        lineno: parseInt(chromeMatch[3], 10),
        colno: parseInt(chromeMatch[4], 10),
        in_app: !chromeMatch[2]?.includes('node_modules'),
      });
      continue;
    }

    // Firefox format: "functionName@filename:line:col"
    const firefoxMatch = line.match(
      /^(.+?)@(.+?):(\d+):(\d+)$/
    );

    if (firefoxMatch) {
      frames.push({
        function: firefoxMatch[1] || '<anonymous>',
        filename: firefoxMatch[2],
        lineno: parseInt(firefoxMatch[3], 10),
        colno: parseInt(firefoxMatch[4], 10),
        in_app: !firefoxMatch[2]?.includes('node_modules'),
      });
    }
  }

  // Reverse to match server expectations (bottom-up)
  return frames.reverse();
}
```

---

## 9. API Endpoints

### 9.1 Base Path

All crash analytics endpoints are under `/api/crash/*`.

### 9.2 Capture (Requires Capture API Key)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/crash/capture` | Ingest single crash event |
| POST | `/api/crash/batch` | Ingest batch of crash events |

### 9.3 Symbol Management (Requires Admin API Key)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/crash/projects/{id}/artifacts` | Upload source maps (multipart) |
| GET | `/api/crash/projects/{id}/artifacts` | List artifacts |
| GET | `/api/crash/projects/{id}/artifacts/{id}` | Get artifact metadata |
| DELETE | `/api/crash/projects/{id}/artifacts/{id}` | Delete artifact |

### 9.4 Issue Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects/{id}/issues` | List issues with filters |
| GET | `/api/crash/projects/{id}/issues/{id}` | Get issue detail |
| POST | `/api/crash/projects/{id}/issues/{id}/resolve` | Resolve issue |
| POST | `/api/crash/projects/{id}/issues/{id}/unresolve` | Unresolve issue |
| POST | `/api/crash/projects/{id}/issues/{id}/ignore` | Ignore issue |
| POST | `/api/crash/projects/{id}/issues/{id}/assign` | Assign to user |
| DELETE | `/api/crash/projects/{id}/issues/{id}` | Delete issue |
| GET | `/api/crash/projects/{id}/issues/{id}/events` | List events for issue |

### 9.5 Event Queries (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects/{id}/events` | List events with filters |
| GET | `/api/crash/projects/{id}/events/{id}` | Get event detail |

### 9.6 Release Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects/{id}/releases` | List releases |
| POST | `/api/crash/projects/{id}/releases` | Create release |
| GET | `/api/crash/projects/{id}/releases/{version}` | Get release detail |

### 9.7 Project Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects` | List projects for org |
| POST | `/api/crash/projects` | Create project |
| GET | `/api/crash/projects/{id}` | Get project |
| PATCH | `/api/crash/projects/{id}` | Update project |
| DELETE | `/api/crash/projects/{id}` | Delete project |

### 9.8 API Key Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects/{id}/keys` | List API keys |
| POST | `/api/crash/projects/{id}/keys` | Create API key |
| DELETE | `/api/crash/projects/{id}/keys/{id}` | Revoke API key |

### 9.9 Real-time (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crash/projects/{id}/stream` | SSE stream for events |

---

## 10. SSE Streaming

### 10.1 Connection

```
GET /api/crash/projects/{id}/stream
Authorization: Bearer <user_token>
```

### 10.2 Events

| Event | Description |
|-------|-------------|
| `crash.new` | New crash event received |
| `issue.new` | New issue created |
| `issue.regressed` | Resolved issue regressed |
| `issue.resolved` | Issue resolved |
| `issue.assigned` | Issue assigned |
| `heartbeat` | Keep-alive (every 30s) |

### 10.3 Event Format

```json
{
  "event": "issue.regressed",
  "data": {
    "issue_id": "iss_xxx",
    "short_id": "PROJ-123",
    "title": "TypeError: Cannot read property 'x' of undefined",
    "times_regressed": 2,
    "regressed_in_release": "1.2.5",
    "timestamp": "2026-01-18T12:00:00Z"
  }
}
```

---

## 11. Web UI Components

### 11.1 Component Structure

```
web/loom-web/src/lib/components/crash/
├── IssueList.svelte              # Paginated issue list with filters
├── IssueListItem.svelte          # Single issue row
├── IssueDetail.svelte            # Full issue view
├── IssueStatusBadge.svelte       # Status pill (Unresolved, Resolved, etc.)
├── IssuePriorityBadge.svelte     # Priority indicator
├── CrashEventCard.svelte         # Crash event summary
├── CrashEventDetail.svelte       # Full event view with context
├── Stacktrace.svelte             # Collapsible stacktrace viewer
├── StacktraceFrame.svelte        # Single frame with source context
├── SourceContext.svelte          # Syntax-highlighted source lines
├── Breadcrumbs.svelte            # Breadcrumb timeline
├── TagsDisplay.svelte            # Tag key-value display
├── ActiveFlags.svelte            # Feature flags active at crash
├── RegressionAlert.svelte        # Regression notification banner
├── SymbolUpload.svelte           # Source map upload form
├── ReleaseList.svelte            # Release overview
├── ReleaseDetail.svelte          # Release with crash stats
├── ProjectSettings.svelte        # Project configuration
└── ApiKeyManager.svelte          # API key CRUD
```

### 11.2 IssueList Component

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import IssueListItem from './IssueListItem.svelte';
  import IssueStatusBadge from './IssueStatusBadge.svelte';

  let { projectId } = $props();

  let issues = $state<Issue[]>([]);
  let loading = $state(true);
  let filters = $state({
    status: 'unresolved' as IssueStatus | 'all',
    sort: 'last_seen' as 'last_seen' | 'first_seen' | 'event_count',
    query: '',
  });

  const filteredIssues = $derived(
    issues.filter(issue => {
      if (filters.status !== 'all' && issue.status !== filters.status) {
        return false;
      }
      if (filters.query && !issue.title.toLowerCase().includes(filters.query.toLowerCase())) {
        return false;
      }
      return true;
    })
  );

  async function loadIssues() {
    loading = true;
    const response = await fetch(`/api/crash/projects/${projectId}/issues`);
    issues = await response.json();
    loading = false;
  }

  onMount(() => {
    loadIssues();

    // SSE for real-time updates
    const eventSource = new EventSource(
      `/api/crash/projects/${projectId}/stream`
    );

    eventSource.addEventListener('issue.new', (e) => {
      const issue = JSON.parse(e.data);
      issues = [issue, ...issues];
    });

    eventSource.addEventListener('issue.regressed', (e) => {
      const data = JSON.parse(e.data);
      issues = issues.map(i =>
        i.id === data.issue_id
          ? { ...i, status: 'regressed', times_regressed: data.times_regressed }
          : i
      );
    });

    return () => eventSource.close();
  });
</script>

<div class="issue-list">
  <div class="filters">
    <select bind:value={filters.status}>
      <option value="all">All</option>
      <option value="unresolved">Unresolved</option>
      <option value="resolved">Resolved</option>
      <option value="regressed">Regressed</option>
      <option value="ignored">Ignored</option>
    </select>

    <input
      type="search"
      placeholder="Search issues..."
      bind:value={filters.query}
    />
  </div>

  {#if loading}
    <div class="loading">Loading issues...</div>
  {:else if filteredIssues.length === 0}
    <div class="empty">No issues found</div>
  {:else}
    <ul>
      {#each filteredIssues as issue (issue.id)}
        <IssueListItem {issue} />
      {/each}
    </ul>
  {/if}
</div>
```

### 11.3 Stacktrace Component

```svelte
<script lang="ts">
  import StacktraceFrame from './StacktraceFrame.svelte';

  let { stacktrace, raw = false } = $props<{
    stacktrace: Stacktrace;
    raw?: boolean;
  }>();

  let expandedFrames = $state<Set<number>>(new Set());
  let showAllFrames = $state(false);

  const frames = $derived(
    showAllFrames
      ? stacktrace.frames
      : stacktrace.frames.filter(f => f.in_app)
  );

  function toggleFrame(index: number) {
    if (expandedFrames.has(index)) {
      expandedFrames.delete(index);
    } else {
      expandedFrames.add(index);
    }
    expandedFrames = new Set(expandedFrames);
  }
</script>

<div class="stacktrace" class:raw>
  <div class="stacktrace-header">
    <span class="frame-count">{frames.length} frames</span>
    <button onclick={() => showAllFrames = !showAllFrames}>
      {showAllFrames ? 'Show app frames only' : 'Show all frames'}
    </button>
  </div>

  <ol class="frames">
    {#each frames as frame, index (index)}
      <StacktraceFrame
        {frame}
        expanded={expandedFrames.has(index)}
        ontoggle={() => toggleFrame(index)}
      />
    {/each}
  </ol>
</div>

<style>
  .stacktrace {
    font-family: var(--font-mono);
    font-size: 0.875rem;
  }

  .frames {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .raw {
    opacity: 0.7;
  }
</style>
```

### 11.4 SourceContext Component

```svelte
<script lang="ts">
  let {
    preContext = [],
    contextLine,
    postContext = [],
    lineNumber
  } = $props<{
    preContext: string[];
    contextLine: string;
    postContext: string[];
    lineNumber: number;
  }>();

  const startLine = $derived(lineNumber - preContext.length);
</script>

<div class="source-context">
  <table>
    <tbody>
      {#each preContext as line, i}
        <tr class="context-line">
          <td class="line-number">{startLine + i}</td>
          <td class="line-content"><code>{line}</code></td>
        </tr>
      {/each}

      <tr class="error-line">
        <td class="line-number">{lineNumber}</td>
        <td class="line-content"><code>{contextLine}</code></td>
      </tr>

      {#each postContext as line, i}
        <tr class="context-line">
          <td class="line-number">{lineNumber + 1 + i}</td>
          <td class="line-content"><code>{line}</code></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .source-context {
    background: var(--color-bg-subtle);
    border-radius: 4px;
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  .line-number {
    width: 50px;
    text-align: right;
    padding-right: 12px;
    color: var(--color-text-muted);
    user-select: none;
  }

  .line-content {
    white-space: pre;
  }

  .error-line {
    background: var(--color-error-bg);
  }

  .error-line .line-number {
    color: var(--color-error);
    font-weight: bold;
  }
</style>
```

---

## 12. Database Schema

### 12.1 Migration: `XXX_crash_analytics.sql`

```sql
-- Crash projects
CREATE TABLE crash_projects (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    platform TEXT NOT NULL,
    auto_resolve_age_days INTEGER,
    fingerprint_rules TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(org_id, slug)
);

CREATE INDEX idx_crash_projects_org_id ON crash_projects(org_id);

-- Crash API keys
CREATE TABLE crash_api_keys (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_type TEXT NOT NULL,  -- 'capture', 'admin'
    key_hash TEXT NOT NULL,
    rate_limit_per_minute INTEGER,
    allowed_origins TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX idx_crash_api_keys_project_id ON crash_api_keys(project_id);
CREATE INDEX idx_crash_api_keys_key_hash ON crash_api_keys(key_hash);

-- Crash issues (aggregated)
CREATE TABLE crash_issues (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    short_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    title TEXT NOT NULL,
    culprit TEXT,
    metadata TEXT NOT NULL,  -- JSON: IssueMetadata
    status TEXT NOT NULL DEFAULT 'unresolved',
    level TEXT NOT NULL DEFAULT 'error',
    priority TEXT NOT NULL DEFAULT 'medium',
    event_count INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    resolved_in_release TEXT,
    times_regressed INTEGER NOT NULL DEFAULT 0,
    last_regressed_at TEXT,
    regressed_in_release TEXT,
    assigned_to TEXT REFERENCES users(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, fingerprint)
);

CREATE INDEX idx_crash_issues_project_id ON crash_issues(project_id);
CREATE INDEX idx_crash_issues_status ON crash_issues(status);
CREATE INDEX idx_crash_issues_last_seen ON crash_issues(last_seen);
CREATE INDEX idx_crash_issues_fingerprint ON crash_issues(fingerprint);

-- Crash events (individual occurrences)
CREATE TABLE crash_events (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    issue_id TEXT REFERENCES crash_issues(id) ON DELETE SET NULL,
    person_id TEXT,  -- From analytics integration
    distinct_id TEXT NOT NULL,
    exception_type TEXT NOT NULL,
    exception_value TEXT NOT NULL,
    stacktrace TEXT NOT NULL,      -- JSON: Stacktrace
    raw_stacktrace TEXT,           -- JSON: Stacktrace (pre-symbolication)
    release TEXT,
    dist TEXT,
    environment TEXT NOT NULL,
    platform TEXT NOT NULL,
    runtime TEXT,                  -- JSON: Runtime
    server_name TEXT,
    tags TEXT NOT NULL DEFAULT '{}',
    extra TEXT NOT NULL DEFAULT '{}',
    user_context TEXT,             -- JSON: UserContext
    device_context TEXT,           -- JSON: DeviceContext
    browser_context TEXT,          -- JSON: BrowserContext
    os_context TEXT,               -- JSON: OsContext
    active_flags TEXT NOT NULL DEFAULT '{}',  -- JSON: flag -> variant
    request TEXT,                  -- JSON: RequestContext
    breadcrumbs TEXT NOT NULL DEFAULT '[]',   -- JSON: Breadcrumb[]
    timestamp TEXT NOT NULL,
    received_at TEXT NOT NULL
);

CREATE INDEX idx_crash_events_project_id ON crash_events(project_id);
CREATE INDEX idx_crash_events_issue_id ON crash_events(issue_id);
CREATE INDEX idx_crash_events_timestamp ON crash_events(timestamp);
CREATE INDEX idx_crash_events_release ON crash_events(release);
CREATE INDEX idx_crash_events_person_id ON crash_events(person_id);

-- Issue-person mapping for user_count
CREATE TABLE crash_issue_persons (
    issue_id TEXT NOT NULL REFERENCES crash_issues(id) ON DELETE CASCADE,
    person_id TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    PRIMARY KEY (issue_id, person_id)
);

-- Symbol artifacts (source maps, debug symbols)
CREATE TABLE symbol_artifacts (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    release TEXT NOT NULL,
    dist TEXT,
    artifact_type TEXT NOT NULL,  -- 'source_map', 'minified_source', 'rust_debug_info'
    name TEXT NOT NULL,
    data BLOB NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    source_map_url TEXT,
    sources_content INTEGER NOT NULL DEFAULT 0,  -- Boolean
    uploaded_at TEXT NOT NULL,
    uploaded_by TEXT NOT NULL REFERENCES users(id),
    last_accessed_at TEXT,
    UNIQUE(project_id, release, name, dist)
);

CREATE INDEX idx_symbol_artifacts_lookup
ON symbol_artifacts(project_id, release, artifact_type);
CREATE INDEX idx_symbol_artifacts_sha256 ON symbol_artifacts(sha256);

-- Releases
CREATE TABLE crash_releases (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    short_version TEXT,
    url TEXT,
    crash_count INTEGER NOT NULL DEFAULT 0,
    new_issue_count INTEGER NOT NULL DEFAULT 0,
    regression_count INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    date_released TEXT,
    first_event TEXT,
    last_event TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(project_id, version)
);

CREATE INDEX idx_crash_releases_project_id ON crash_releases(project_id);
CREATE INDEX idx_crash_releases_version ON crash_releases(version);
```

---

## 13. Retention Policy

Following Sentry's retention model:

| Data Type | Retention | Notes |
|-----------|-----------|-------|
| Raw crash events | 90 days | Configurable per org |
| Issue metadata | Forever | Until manually deleted |
| Symbol artifacts | 90 days after last access | Auto-cleanup job |
| Aggregated stats | Forever | Counts, timeseries |
| Breadcrumbs | 90 days | Part of event |

### 13.1 Cleanup Job

```rust
// Run daily via loom-jobs scheduler
pub async fn cleanup_old_data(db: &Database, config: &RetentionConfig) -> Result<()> {
    let cutoff = Utc::now() - Duration::days(config.event_retention_days as i64);

    // Delete old events
    sqlx::query!(
        "DELETE FROM crash_events WHERE timestamp < ?",
        cutoff.to_rfc3339()
    )
    .execute(db)
    .await?;

    // Delete unused symbol artifacts
    let artifact_cutoff = Utc::now() - Duration::days(config.artifact_retention_days as i64);
    sqlx::query!(
        "DELETE FROM symbol_artifacts
         WHERE last_accessed_at < ? OR
               (last_accessed_at IS NULL AND uploaded_at < ?)",
        artifact_cutoff.to_rfc3339(),
        artifact_cutoff.to_rfc3339()
    )
    .execute(db)
    .await?;

    Ok(())
}
```

---

## 14. API Key Format

Following the analytics/flags pattern:

| Type | Prefix | Use Case |
|------|--------|----------|
| Capture | `loom_crash_capture_` | Client-side SDK (safe for browser) |
| Admin | `loom_crash_admin_` | Server-side, symbol uploads, issue management |

Example:
```
loom_crash_capture_proj123_7a3b9f2e1c4d8a5b6e0f3c2d1a4b5c6d
loom_crash_admin_proj123_8b4c0g3f2d5e9a6c7f1g4d3e2b5a6c7d
```

---

## 15. Integration Points

### 15.1 Analytics Integration

The crash SDK integrates with `loom-analytics` for identity:

```rust
impl CrashClient {
    pub fn with_analytics(mut self, analytics: &AnalyticsClient) -> Self {
        self.analytics = Some(analytics.clone());
        self
    }

    fn get_person_id(&self) -> Option<PersonId> {
        self.analytics.as_ref()?.get_person_id()
    }

    fn get_distinct_id(&self) -> String {
        self.analytics
            .as_ref()
            .map(|a| a.get_distinct_id())
            .unwrap_or_else(|| self.fallback_distinct_id.clone())
    }
}
```

### 15.2 Feature Flags Integration

Capture active flags at crash time:

```rust
impl CrashClient {
    pub fn with_flags(mut self, flags: &FlagsClient) -> Self {
        self.flags = Some(flags.clone());
        self
    }

    fn get_active_flags(&self) -> HashMap<String, String> {
        self.flags
            .as_ref()
            .map(|f| f.get_all_cached_variants())
            .unwrap_or_default()
    }
}
```

### 15.3 Query by Active Flags

Find crashes that occurred with a specific flag variant:

```sql
SELECT ce.* FROM crash_events ce
WHERE json_extract(ce.active_flags, '$.checkout.new_flow') = 'treatment_a'
  AND ce.timestamp > ?
ORDER BY ce.timestamp DESC;
```

---

## 16. Audit Events

Crash operations logged via `loom-server-audit`:

| Event | Description |
|-------|-------------|
| `CrashProjectCreated` | New crash project created |
| `CrashProjectUpdated` | Project settings changed |
| `CrashProjectDeleted` | Project deleted |
| `CrashApiKeyCreated` | API key created |
| `CrashApiKeyRevoked` | API key revoked |
| `CrashIssueResolved` | Issue marked resolved |
| `CrashIssueIgnored` | Issue ignored |
| `CrashIssueAssigned` | Issue assigned to user |
| `CrashIssueDeleted` | Issue deleted |
| `CrashSymbolsUploaded` | Source maps uploaded |
| `CrashSymbolsDeleted` | Symbols deleted |
| `CrashReleaseCreated` | Release created |

---

## 17. Permissions

### 17.1 Project Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List projects | ✓ | ✓ (read) | ✓ (all orgs) |
| Create project | ✓ | ✗ | ✓ |
| Update project | ✓ | ✗ | ✓ |
| Delete project | ✓ | ✗ | ✓ |

### 17.2 Issue Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| View issues | ✓ | ✓ | ✓ (all) |
| Resolve/ignore | ✓ | ✓ | ✓ |
| Assign | ✓ | ✓ | ✓ |
| Delete | ✓ | ✗ | ✓ |

### 17.3 Symbol Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| Upload symbols | ✓ | ✓ | ✓ |
| View symbols | ✓ | ✓ | ✓ |
| Delete symbols | ✓ | ✗ | ✓ |

### 17.4 API Key Management

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List keys | ✓ | ✗ | ✓ (all) |
| Create key | ✓ | ✗ | ✓ |
| Revoke key | ✓ | ✗ | ✓ |

---

## 18. Configuration

### 18.1 Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_CRASH_ENABLED` | boolean | Enable crash analytics | `true` |
| `LOOM_CRASH_MAX_EVENT_SIZE_BYTES` | integer | Max crash event size | `1048576` (1MB) |
| `LOOM_CRASH_MAX_STACKTRACE_FRAMES` | integer | Max frames to store | `100` |
| `LOOM_CRASH_EVENT_RETENTION_DAYS` | integer | Event retention | `90` |
| `LOOM_CRASH_ARTIFACT_RETENTION_DAYS` | integer | Symbol retention | `90` |
| `LOOM_CRASH_SSE_HEARTBEAT_INTERVAL` | duration | SSE heartbeat | `30s` |

---

## 19. Implementation Phases

### Phase 1: Core Types & Database (3-4 hours)

- [ ] Create `loom-crash-core` crate
- [ ] Define CrashEvent, Stacktrace, Frame types
- [ ] Define Issue, IssueStatus types
- [ ] Define SymbolArtifact types
- [ ] Add database migration
- [ ] Create repository layer

### Phase 2: Ingestion & Fingerprinting (3-4 hours)

- [ ] POST `/api/crash/capture` endpoint
- [ ] Fingerprinting algorithm
- [ ] Issue creation/update logic
- [ ] Regression detection
- [ ] API key authentication middleware

### Phase 3: Issue Management (3-4 hours)

- [ ] Issue CRUD handlers
- [ ] Status transitions (resolve, ignore, unresolve)
- [ ] Assignment
- [ ] Event listing for issues
- [ ] Filtering and pagination

### Phase 4: Source Map Processing (4-5 hours)

- [ ] Create `loom-crash-symbolicate` crate
- [ ] VLQ decoder implementation
- [ ] Source map parser
- [ ] Symbolication pipeline
- [ ] Source context extraction

### Phase 5: Symbol Upload (3-4 hours)

- [ ] Multipart upload endpoint
- [ ] Source map validation
- [ ] SQLite blob storage
- [ ] Deduplication by SHA256
- [ ] Symbol lookup for releases

### Phase 6: Release Tracking (2-3 hours)

- [ ] Release CRUD
- [ ] Auto-create on first crash
- [ ] Crash stats per release
- [ ] Regression correlation

### Phase 7: SSE Streaming (2-3 hours)

- [ ] SSE endpoint
- [ ] New crash broadcasts
- [ ] Issue state change broadcasts
- [ ] Regression alerts
- [ ] Heartbeat

### Phase 8: Rust SDK (4-5 hours)

- [ ] Create `loom-crash` crate
- [ ] CrashClient implementation
- [ ] Panic hook
- [ ] Backtrace capture and parsing
- [ ] Analytics/flags integration
- [ ] Breadcrumb API
- [ ] Transport with batching

### Phase 9: TypeScript SDK (4-5 hours)

- [ ] Create `@loom/crash` package
- [ ] CrashClient implementation
- [ ] Global error handlers
- [ ] Stack trace parsing
- [ ] React error boundary
- [ ] Analytics/flags integration
- [ ] Breadcrumb API

### Phase 10: Web UI Components (5-6 hours)

- [ ] IssueList component
- [ ] IssueDetail component
- [ ] Stacktrace viewer
- [ ] SourceContext viewer
- [ ] Breadcrumbs timeline
- [ ] ActiveFlags display
- [ ] RegressionAlert banner
- [ ] SymbolUpload form
- [ ] Storybook stories for all

### Phase 11: Project & API Keys (2-3 hours)

- [ ] Project CRUD
- [ ] API key generation
- [ ] Rate limiting support
- [ ] CORS for browser SDKs

### Phase 12: Cleanup & Retention (2-3 hours)

- [ ] Retention job in loom-jobs
- [ ] Event cleanup
- [ ] Artifact cleanup
- [ ] Auto-resolve old issues

### Phase 13: Audit Integration (1-2 hours)

- [ ] Add audit event types
- [ ] Integrate with handlers
- [ ] Test audit logging

---

## 20. Rust Dependencies

```toml
# loom-crash-core
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }
loom-secret = { path = "../loom-secret" }

# loom-crash (SDK)
[dependencies]
loom-crash-core = { path = "../loom-crash-core" }
loom-http = { path = "../loom-http" }
loom-analytics = { path = "../loom-analytics", optional = true }
loom-flags = { path = "../loom-flags", optional = true }
async-trait = "0.1"
backtrace = "0.3"
rustc-demangle = "0.1"
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"

[features]
analytics = ["loom-analytics"]
flags = ["loom-flags"]

# loom-crash-symbolicate
[dependencies]
loom-crash-core = { path = "../loom-crash-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
hex = "0.4"
thiserror = "2"

# loom-server-crash
[dependencies]
loom-crash-core = { path = "../loom-crash-core" }
loom-crash-symbolicate = { path = "../loom-crash-symbolicate" }
loom-db = { path = "../loom-db" }
loom-server-audit = { path = "../loom-server-audit" }
axum = "0.8"
sqlx = { version = "0.8", features = ["sqlite"] }
argon2 = "0.5"
tokio = { version = "1", features = ["sync"] }
tokio-stream = "0.1"
```

---

## Appendix A: Sentry Compatibility Notes

This system is inspired by Sentry but is not a drop-in replacement:

| Sentry | Loom Crash | Notes |
|--------|------------|-------|
| DSN format | API key header | Simpler auth model |
| Envelope format | JSON | Simpler ingestion |
| `@sentry/browser` | `@loom/crash` | Similar API |
| Integrations | SDK options | Opt-in |
| Scope | Context | Similar concept |

---

## Appendix B: Future Considerations

| Feature | Description |
|---------|-------------|
| iOS dSYM | Native iOS crash symbolication |
| Alerting | Email/Slack/webhook on regressions |
| Custom fingerprinting UI | Visual rule builder |
| Performance monitoring | Transactions and spans |
| Session replay | Video-style replay integration |
| GitHub integration | Auto-create issues, link commits |
| PagerDuty integration | Incident management |
| Suspect commits | Blame crashes on specific commits |
