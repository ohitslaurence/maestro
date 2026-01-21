<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Session Analytics System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-18

---

## 1. Overview

### Purpose

This specification defines a session analytics system for Loom. Sessions track user engagement periods and enable release health metrics like crash-free rate and adoption tracking. The system uses a hybrid storage approach: individual sessions for recent data (debugging) and aggregates for historical data (dashboards).

### Goals

- **Session tracking** for web, Node.js, and Rust applications
- **Release health metrics**: crash-free rate, adoption percentage
- **Hybrid storage**: individual sessions (30 days) + hourly aggregates (forever)
- **Integration** with `loom-crash` (crashed sessions) and `loom-analytics` (person identity)
- **Sampling support** for high-volume applications
- **Real-time updates** via SSE for release health changes

### Non-Goals (v1)

- Session replay (video-like recording) — separate feature if ever
- Funnel analysis — use `loom-analytics` events
- Cohort analysis — deferred
- Mobile SDK (iOS/Android) — deferred

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-sessions-core/               # Shared types for sessions
│   ├── src/
│   │   ├── lib.rs
│   │   ├── session.rs                # Session, SessionStatus
│   │   ├── release_health.rs         # ReleaseHealth metrics
│   │   ├── aggregate.rs              # SessionAggregate
│   │   └── error.rs                  # Error types
│   └── Cargo.toml
├── loom-sessions/                    # Rust SDK (session tracking)
│   ├── src/
│   │   ├── lib.rs
│   │   ├── tracker.rs                # SessionTracker
│   │   └── integration.rs            # Crash/analytics integration
│   └── Cargo.toml
├── loom-server-sessions/             # Server-side API and aggregation
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs                 # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── sessions.rs           # Session ingest/query
│   │   │   ├── releases.rs           # Release health endpoints
│   │   │   └── aggregates.rs         # Aggregate queries
│   │   ├── repository.rs             # Database repository
│   │   ├── aggregator.rs             # Hourly aggregation job
│   │   ├── cleanup.rs                # Old session cleanup
│   │   └── sse.rs                    # SSE streaming
│   └── Cargo.toml

web/
├── packages/
│   └── sessions/                     # @loom/sessions - TypeScript SDK
│       ├── src/
│       │   ├── index.ts
│       │   ├── tracker.ts            # SessionTracker
│       │   ├── visibility.ts         # Page visibility handling
│       │   └── integration.ts        # Crash/analytics integration
│       └── package.json
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Clients                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│    Browser App              Node.js App              Rust App               │
│    ┌──────────────┐        ┌──────────────┐        ┌──────────────┐        │
│    │ @loom/crash  │        │ @loom/crash  │        │ loom-crash   │        │
│    │ + sessions   │        │ + sessions   │        │ + sessions   │        │
│    └──────┬───────┘        └──────┬───────┘        └──────┬───────┘        │
│           │                       │                       │                 │
└───────────┼───────────────────────┼───────────────────────┼─────────────────┘
            │                       │                       │
            └───────────────────────┼───────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐             │
│  │ Session Routes  │  │ Release Health  │  │ Crash Routes    │             │
│  │ /api/sessions/* │  │ /api/releases/* │  │ /api/crash/*    │             │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘             │
│           │                    │                    │                       │
│           └────────────────────┼────────────────────┘                       │
│                                ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                     Session Aggregator                                │  │
│  │    Hourly job: roll up individual sessions into aggregates           │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                │                                            │
│                                ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                     Cleanup Job                                       │  │
│  │    Delete individual sessions older than 30 days                     │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  sessions (30 days)    session_aggregates (forever)    release_health       │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Entities

### 3.1 Session

A single user engagement period.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub org_id: OrgId,
    pub project_id: ProjectId,

    // Identity (from loom-analytics)
    pub person_id: Option<PersonId>,
    pub distinct_id: String,

    // Status
    pub status: SessionStatus,

    // Release context
    pub release: Option<String>,
    pub environment: String,

    // Error tracking
    pub error_count: u32,           // Handled errors during session
    pub crash_count: u32,           // Unhandled errors (crashes)
    pub crashed: bool,              // Shorthand: crash_count > 0

    // Timing
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,

    // Context
    pub platform: Platform,
    pub user_agent: Option<String>,
    pub ip_address: Option<Secret<String>>,

    // Sampling
    pub sampled: bool,              // Whether this session was sampled
    pub sample_rate: f64,           // Rate at which it was sampled

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    /// Session is active (ongoing)
    Active,

    /// Session ended normally (user closed tab, app backgrounded)
    Exited,

    /// Session had at least one unhandled error
    Crashed,

    /// Session ended unexpectedly (no end signal received)
    Abnormal,

    /// Session had handled errors but completed normally
    Errored,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    JavaScript,      // Browser
    Node,            // Node.js
    Rust,
}
```

### 3.2 SessionAggregate

Hourly rollup of session data for efficient querying.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAggregate {
    pub id: SessionAggregateId,
    pub org_id: OrgId,
    pub project_id: ProjectId,

    // Grouping keys
    pub release: Option<String>,
    pub environment: String,
    pub hour: DateTime<Utc>,          // Truncated to hour: "2026-01-18T12:00:00Z"

    // Session counts by status
    pub total_sessions: u64,
    pub exited_sessions: u64,         // Normal exits
    pub crashed_sessions: u64,        // Had unhandled error
    pub abnormal_sessions: u64,       // No end signal
    pub errored_sessions: u64,        // Had handled errors

    // User counts
    pub unique_users: u64,
    pub crashed_users: u64,

    // Duration stats (in ms)
    pub total_duration_ms: u64,
    pub min_duration_ms: Option<u64>,
    pub max_duration_ms: Option<u64>,

    // Error counts
    pub total_errors: u64,
    pub total_crashes: u64,

    pub updated_at: DateTime<Utc>,
}
```

### 3.3 ReleaseHealth

Computed metrics for a release.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseHealth {
    pub project_id: ProjectId,
    pub release: String,
    pub environment: String,

    // Session metrics
    pub total_sessions: u64,
    pub crashed_sessions: u64,
    pub errored_sessions: u64,

    // User metrics
    pub total_users: u64,
    pub crashed_users: u64,

    // Calculated rates
    pub crash_free_session_rate: f64,   // (total - crashed) / total * 100
    pub crash_free_user_rate: f64,      // (total_users - crashed_users) / total_users * 100

    // Adoption
    pub adoption_rate: f64,             // This release sessions / all sessions * 100
    pub adoption_stage: AdoptionStage,

    // Timeline
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,

    // Trend (compared to previous period)
    pub crash_free_rate_trend: Option<f64>,  // +/- percentage points
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdoptionStage {
    New,          // < 5% adoption
    Growing,      // 5-50% adoption
    Adopted,      // 50-95% adoption
    Replaced,     // < 5% (was higher before)
}
```

---

## 4. Session Lifecycle

### 4.1 Web (Browser)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Page      │     │   User      │     │  Tab/Window │     │   Crash     │
│   Load      │     │   Idle      │     │   Close     │     │   Occurs    │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │                   │
       ▼                   ▼                   ▼                   ▼
   ┌───────┐          ┌────────┐          ┌────────┐          ┌────────┐
   │ START │          │ (keep) │          │  END   │          │ CRASH  │
   │session│          │ active │          │ exited │          │ status │
   └───────┘          └────────┘          └────────┘          └────────┘
```

**Session boundaries:**
- **Start**: Page load / SDK initialization
- **End**: `beforeunload`, `pagehide`, or visibility change to hidden for 30+ minutes
- **Crash**: Unhandled error captured by crash SDK

### 4.2 Node.js

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Process   │     │   Process   │     │   Uncaught  │
│   Start     │     │   Exit      │     │   Exception │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       ▼                   ▼                   ▼
   ┌───────┐          ┌────────┐          ┌────────┐
   │ START │          │  END   │          │ CRASH  │
   │session│          │ exited │          │ status │
   └───────┘          └────────┘          └────────┘
```

### 4.3 Rust

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Client    │     │   Client    │     │   Panic     │
│   Init      │     │   Shutdown  │     │   Occurs    │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       ▼                   ▼                   ▼
   ┌───────┐          ┌────────┐          ┌────────┐
   │ START │          │  END   │          │ CRASH  │
   │session│          │ exited │          │ status │
   └───────┘          └────────┘          └────────┘
```

### 4.4 Session Status Determination

```rust
impl Session {
    pub fn determine_status(&self) -> SessionStatus {
        if self.crash_count > 0 {
            SessionStatus::Crashed
        } else if self.error_count > 0 {
            SessionStatus::Errored
        } else if self.ended_at.is_some() {
            SessionStatus::Exited
        } else {
            SessionStatus::Active
        }
    }
}
```

---

## 5. SDK Integration

### 5.1 Integration with Crash SDK

Sessions are managed as part of the crash SDK, not separately:

```rust
// In loom-crash
pub struct CrashClient {
    // ... existing fields
    session_tracker: Option<SessionTracker>,
}

impl CrashClientBuilder {
    /// Enable automatic session tracking
    pub fn with_session_tracking(mut self, enabled: bool) -> Self {
        self.session_tracking = enabled;
        self
    }

    /// Set session sample rate (0.0-1.0)
    pub fn session_sample_rate(mut self, rate: f64) -> Self {
        self.session_sample_rate = rate;
        self
    }
}
```

### 5.2 Rust SDK Session Tracking

```rust
use loom_crash::CrashClient;

// Sessions enabled by default
let crash = CrashClient::builder()
    .api_key("loom_crash_xxx")
    .release(env!("CARGO_PKG_VERSION"))
    .with_session_tracking(true)   // Default: true
    .session_sample_rate(1.0)      // Default: 1.0 (100%)
    .build()?;

// Session starts automatically on build()

// When a crash occurs, session is marked as crashed
// crash.capture_error() automatically updates session

// On shutdown, session ends
crash.shutdown().await?;  // Sends session end
```

### 5.3 Session Tracker Implementation

```rust
pub struct SessionTracker {
    session_id: SessionId,
    started_at: DateTime<Utc>,
    error_count: AtomicU32,
    crash_count: AtomicU32,
    sample_rate: f64,
    sampled: bool,
    http: HttpClient,
}

impl SessionTracker {
    pub fn new(config: SessionConfig) -> Self {
        // Determine if this session should be sampled
        let sampled = rand::random::<f64>() < config.sample_rate;

        Self {
            session_id: SessionId::new(),
            started_at: Utc::now(),
            error_count: AtomicU32::new(0),
            crash_count: AtomicU32::new(0),
            sample_rate: config.sample_rate,
            sampled,
            http: config.http,
        }
    }

    /// Start the session (send to server)
    pub async fn start(&self, context: &SessionContext) -> Result<()> {
        // Always send crashed sessions, respect sampling for others
        if !self.sampled {
            return Ok(());
        }

        self.http.post("/api/sessions/start")
            .json(&SessionStartRequest {
                session_id: self.session_id.clone(),
                distinct_id: context.distinct_id.clone(),
                release: context.release.clone(),
                environment: context.environment.clone(),
                platform: context.platform,
                started_at: self.started_at,
                sample_rate: self.sample_rate,
            })
            .send()
            .await?;

        Ok(())
    }

    /// Record an error (handled)
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Record a crash (unhandled)
    pub fn record_crash(&self) {
        self.crash_count.fetch_add(1, Ordering::SeqCst);
    }

    /// End the session
    pub async fn end(&self, context: &SessionContext) -> Result<()> {
        let crash_count = self.crash_count.load(Ordering::SeqCst);

        // Always send crashed sessions, even if not sampled
        if !self.sampled && crash_count == 0 {
            return Ok(());
        }

        let ended_at = Utc::now();
        let duration_ms = (ended_at - self.started_at).num_milliseconds() as u64;

        self.http.post("/api/sessions/end")
            .json(&SessionEndRequest {
                session_id: self.session_id.clone(),
                status: self.determine_status(),
                error_count: self.error_count.load(Ordering::SeqCst),
                crash_count,
                ended_at,
                duration_ms,
            })
            .send()
            .await?;

        Ok(())
    }

    fn determine_status(&self) -> SessionStatus {
        let crash_count = self.crash_count.load(Ordering::SeqCst);
        let error_count = self.error_count.load(Ordering::SeqCst);

        if crash_count > 0 {
            SessionStatus::Crashed
        } else if error_count > 0 {
            SessionStatus::Errored
        } else {
            SessionStatus::Exited
        }
    }
}
```

### 5.4 TypeScript SDK Session Tracking

```typescript
// In @loom/crash
import { CrashClient } from '@loom/crash';

const crash = new CrashClient({
  apiKey: 'loom_crash_xxx',
  release: '1.2.3',

  // Session options
  autoSessionTracking: true,      // Default: true
  sessionSampleRate: 1.0,         // Default: 1.0
});

// Session starts automatically

// Errors update session
crash.captureException(error);    // Increments error_count
// Unhandled errors mark as crashed

// Session ends on page unload or manual call
crash.endSession();
```

### 5.5 Browser Session Tracking

```typescript
export class BrowserSessionTracker {
  private sessionId: string;
  private startedAt: Date;
  private errorCount = 0;
  private crashCount = 0;
  private ended = false;

  constructor(private config: SessionConfig) {
    this.sessionId = crypto.randomUUID();
    this.startedAt = new Date();

    // Auto-end on page unload
    window.addEventListener('beforeunload', () => this.end());
    window.addEventListener('pagehide', () => this.end());

    // Handle visibility changes (30 min timeout)
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') {
        this.scheduleEnd();
      } else {
        this.cancelScheduledEnd();
      }
    });
  }

  private scheduleEnd() {
    this.endTimeout = setTimeout(() => {
      this.end();
      // Start new session if page becomes visible again
    }, 30 * 60 * 1000); // 30 minutes
  }

  async start(): Promise<void> {
    if (!this.shouldSample()) return;

    await fetch('/api/sessions/start', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        session_id: this.sessionId,
        started_at: this.startedAt.toISOString(),
        // ... other context
      }),
      // Use keepalive for reliability
      keepalive: true,
    });
  }

  async end(): Promise<void> {
    if (this.ended) return;
    this.ended = true;

    // Always send crashed sessions
    if (!this.shouldSample() && this.crashCount === 0) return;

    const endedAt = new Date();

    // Use sendBeacon for reliability on page unload
    navigator.sendBeacon('/api/sessions/end', JSON.stringify({
      session_id: this.sessionId,
      status: this.getStatus(),
      error_count: this.errorCount,
      crash_count: this.crashCount,
      ended_at: endedAt.toISOString(),
      duration_ms: endedAt.getTime() - this.startedAt.getTime(),
    }));
  }

  recordError(): void {
    this.errorCount++;
  }

  recordCrash(): void {
    this.crashCount++;
  }

  private shouldSample(): boolean {
    // Deterministic based on session ID
    const hash = this.hashCode(this.sessionId);
    return (hash % 100) < (this.config.sampleRate * 100);
  }

  private getStatus(): SessionStatus {
    if (this.crashCount > 0) return 'crashed';
    if (this.errorCount > 0) return 'errored';
    return 'exited';
  }
}
```

---

## 6. Sampling

### 6.1 Why Sample?

High-traffic applications can generate millions of sessions. Sampling reduces:
- Storage costs
- Ingestion load
- Query times

### 6.2 Sampling Rules

```rust
pub struct SamplingConfig {
    /// Base sample rate (0.0-1.0)
    pub session_sample_rate: f64,

    /// Always store crashed sessions (recommended: true)
    pub always_store_crashed: bool,

    /// Minimum sessions per release (for small releases)
    pub min_sessions_per_release: Option<u64>,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            session_sample_rate: 1.0,  // 100% by default
            always_store_crashed: true,
            min_sessions_per_release: Some(1000),
        }
    }
}
```

### 6.3 Sampling at Ingestion

```rust
async fn ingest_session(
    session: SessionStartRequest,
    config: &SamplingConfig,
) -> Result<Option<Session>> {
    // Crashed sessions always stored
    if session.crashed && config.always_store_crashed {
        return Ok(Some(create_session(session).await?));
    }

    // Check sample rate
    if !should_sample(&session.session_id, config.session_sample_rate) {
        // Still count in aggregates, just don't store individual
        increment_aggregate_counter(&session).await?;
        return Ok(None);
    }

    Ok(Some(create_session(session).await?))
}

fn should_sample(session_id: &SessionId, rate: f64) -> bool {
    // Deterministic: same session ID always gives same result
    let hash = murmur3_32(session_id.as_bytes(), 0);
    (hash % 10000) < (rate * 10000.0) as u32
}
```

### 6.4 Adjusting Metrics for Sampling

When calculating metrics, account for sampling:

```rust
impl ReleaseHealth {
    pub fn calculate(aggregates: &[SessionAggregate], sample_rate: f64) -> Self {
        let total_sessions = aggregates.iter()
            .map(|a| a.total_sessions)
            .sum::<u64>();

        // Crashed sessions are always stored, don't adjust
        let crashed_sessions = aggregates.iter()
            .map(|a| a.crashed_sessions)
            .sum::<u64>();

        // Adjust non-crashed counts for sampling
        let adjusted_total = if sample_rate < 1.0 {
            let non_crashed = total_sessions - crashed_sessions;
            crashed_sessions + (non_crashed as f64 / sample_rate) as u64
        } else {
            total_sessions
        };

        let crash_free_rate = if adjusted_total > 0 {
            ((adjusted_total - crashed_sessions) as f64 / adjusted_total as f64) * 100.0
        } else {
            100.0
        };

        Self {
            total_sessions: adjusted_total,
            crashed_sessions,
            crash_free_session_rate: crash_free_rate,
            // ...
        }
    }
}
```

---

## 7. Aggregation

### 7.1 Hourly Aggregation Job

Runs every hour to roll up individual sessions:

```rust
// Runs via loom-jobs every hour
pub async fn aggregate_sessions(db: &Database) -> Result<()> {
    let now = Utc::now();
    let current_hour = now.trunc_hour();
    let previous_hour = current_hour - Duration::hours(1);

    // Aggregate sessions from the previous hour
    let aggregates = sqlx::query!(
        r#"
        SELECT
            project_id,
            release,
            environment,
            COUNT(*) as total_sessions,
            SUM(CASE WHEN status = 'exited' THEN 1 ELSE 0 END) as exited_sessions,
            SUM(CASE WHEN status = 'crashed' THEN 1 ELSE 0 END) as crashed_sessions,
            SUM(CASE WHEN status = 'abnormal' THEN 1 ELSE 0 END) as abnormal_sessions,
            SUM(CASE WHEN status = 'errored' THEN 1 ELSE 0 END) as errored_sessions,
            COUNT(DISTINCT person_id) as unique_users,
            COUNT(DISTINCT CASE WHEN crashed = 1 THEN person_id END) as crashed_users,
            SUM(duration_ms) as total_duration_ms,
            MIN(duration_ms) as min_duration_ms,
            MAX(duration_ms) as max_duration_ms,
            SUM(error_count) as total_errors,
            SUM(crash_count) as total_crashes
        FROM sessions
        WHERE started_at >= ? AND started_at < ?
        GROUP BY project_id, release, environment
        "#,
        previous_hour.to_rfc3339(),
        current_hour.to_rfc3339()
    )
    .fetch_all(db)
    .await?;

    // Upsert aggregates
    for agg in aggregates {
        upsert_session_aggregate(SessionAggregate {
            project_id: agg.project_id,
            release: agg.release,
            environment: agg.environment,
            hour: previous_hour,
            total_sessions: agg.total_sessions as u64,
            exited_sessions: agg.exited_sessions as u64,
            crashed_sessions: agg.crashed_sessions as u64,
            abnormal_sessions: agg.abnormal_sessions as u64,
            errored_sessions: agg.errored_sessions as u64,
            unique_users: agg.unique_users as u64,
            crashed_users: agg.crashed_users as u64,
            total_duration_ms: agg.total_duration_ms as u64,
            min_duration_ms: agg.min_duration_ms.map(|v| v as u64),
            max_duration_ms: agg.max_duration_ms.map(|v| v as u64),
            total_errors: agg.total_errors as u64,
            total_crashes: agg.total_crashes as u64,
            ..Default::default()
        }).await?;
    }

    Ok(())
}
```

### 7.2 Cleanup Job

Delete individual sessions older than retention period:

```rust
// Runs daily via loom-jobs
pub async fn cleanup_old_sessions(db: &Database, retention_days: u32) -> Result<u64> {
    let cutoff = Utc::now() - Duration::days(retention_days as i64);

    let result = sqlx::query!(
        "DELETE FROM sessions WHERE started_at < ?",
        cutoff.to_rfc3339()
    )
    .execute(db)
    .await?;

    Ok(result.rows_affected())
}
```

---

## 8. Release Health Calculation

### 8.1 Query for Release Health

```rust
pub async fn get_release_health(
    db: &Database,
    project_id: &ProjectId,
    release: &str,
    environment: &str,
    time_range: TimeRange,
) -> Result<ReleaseHealth> {
    // Query aggregates for the time range
    let aggregates = sqlx::query_as!(
        SessionAggregate,
        r#"
        SELECT * FROM session_aggregates
        WHERE project_id = ?
          AND release = ?
          AND environment = ?
          AND hour >= ?
          AND hour < ?
        "#,
        project_id.to_string(),
        release,
        environment,
        time_range.start.to_rfc3339(),
        time_range.end.to_rfc3339()
    )
    .fetch_all(db)
    .await?;

    // Calculate metrics
    let total_sessions: u64 = aggregates.iter().map(|a| a.total_sessions).sum();
    let crashed_sessions: u64 = aggregates.iter().map(|a| a.crashed_sessions).sum();
    let errored_sessions: u64 = aggregates.iter().map(|a| a.errored_sessions).sum();
    let unique_users: u64 = aggregates.iter().map(|a| a.unique_users).sum();
    let crashed_users: u64 = aggregates.iter().map(|a| a.crashed_users).sum();

    let crash_free_session_rate = if total_sessions > 0 {
        ((total_sessions - crashed_sessions) as f64 / total_sessions as f64) * 100.0
    } else {
        100.0
    };

    let crash_free_user_rate = if unique_users > 0 {
        ((unique_users - crashed_users) as f64 / unique_users as f64) * 100.0
    } else {
        100.0
    };

    // Calculate adoption (this release vs all releases)
    let all_sessions = get_total_sessions(db, project_id, environment, &time_range).await?;
    let adoption_rate = if all_sessions > 0 {
        (total_sessions as f64 / all_sessions as f64) * 100.0
    } else {
        0.0
    };

    Ok(ReleaseHealth {
        project_id: project_id.clone(),
        release: release.to_string(),
        environment: environment.to_string(),
        total_sessions,
        crashed_sessions,
        errored_sessions,
        total_users: unique_users,
        crashed_users,
        crash_free_session_rate,
        crash_free_user_rate,
        adoption_rate,
        adoption_stage: determine_adoption_stage(adoption_rate),
        first_seen: aggregates.first().map(|a| a.hour).unwrap_or(Utc::now()),
        last_seen: aggregates.last().map(|a| a.hour).unwrap_or(Utc::now()),
        crash_free_rate_trend: None, // Calculate separately if needed
    })
}

fn determine_adoption_stage(adoption_rate: f64) -> AdoptionStage {
    if adoption_rate < 5.0 {
        AdoptionStage::New
    } else if adoption_rate < 50.0 {
        AdoptionStage::Growing
    } else if adoption_rate < 95.0 {
        AdoptionStage::Adopted
    } else {
        AdoptionStage::Replaced // Or fully adopted
    }
}
```

### 8.2 Release Comparison

```rust
pub async fn compare_releases(
    db: &Database,
    project_id: &ProjectId,
    environment: &str,
    time_range: TimeRange,
) -> Result<Vec<ReleaseComparison>> {
    let releases = get_releases_in_range(db, project_id, environment, &time_range).await?;

    let mut comparisons = Vec::new();

    for release in releases {
        let health = get_release_health(db, project_id, &release, environment, time_range).await?;

        // Get previous period for trend
        let prev_range = TimeRange {
            start: time_range.start - (time_range.end - time_range.start),
            end: time_range.start,
        };
        let prev_health = get_release_health(db, project_id, &release, environment, prev_range).await.ok();

        let trend = prev_health.map(|prev| {
            health.crash_free_session_rate - prev.crash_free_session_rate
        });

        comparisons.push(ReleaseComparison {
            release,
            health,
            crash_free_rate_trend: trend,
        });
    }

    // Sort by adoption rate descending
    comparisons.sort_by(|a, b| {
        b.health.adoption_rate.partial_cmp(&a.health.adoption_rate).unwrap()
    });

    Ok(comparisons)
}
```

---

## 9. API Endpoints

### 9.1 Session Ingestion (SDK)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/sessions/start` | Start a session |
| POST | `/api/sessions/end` | End a session |
| POST | `/api/sessions/update` | Update session (error counts) |

### 9.2 Release Health (User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/projects/{id}/releases` | List releases with health |
| GET | `/api/projects/{id}/releases/{version}` | Get release health detail |
| GET | `/api/projects/{id}/releases/{version}/sessions` | List sessions for release |
| GET | `/api/projects/{id}/health` | Overall project health |
| GET | `/api/projects/{id}/health/trend` | Health trend over time |

### 9.3 Session Queries (User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/projects/{id}/sessions` | List recent sessions |
| GET | `/api/projects/{id}/sessions/{id}` | Get session detail |
| GET | `/api/projects/{id}/sessions/by-person/{person_id}` | Sessions for person |

### 9.4 Real-time (User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/projects/{id}/sessions/stream` | SSE for session events |

---

## 10. SSE Streaming

### 10.1 Events

| Event | Description |
|-------|-------------|
| `session.started` | New session started |
| `session.ended` | Session ended |
| `session.crashed` | Session marked as crashed |
| `release.health_changed` | Release health metrics updated |
| `heartbeat` | Keep-alive (every 30s) |

### 10.2 Event Format

```json
{
  "event": "release.health_changed",
  "data": {
    "release": "1.2.3",
    "environment": "production",
    "crash_free_session_rate": 98.5,
    "crash_free_user_rate": 99.1,
    "adoption_rate": 45.2,
    "timestamp": "2026-01-18T12:00:00Z"
  }
}
```

---

## 11. Database Schema

### 11.1 Migration: `XXX_sessions.sql`

```sql
-- Individual sessions (retained for 30 days)
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,

    person_id TEXT,
    distinct_id TEXT NOT NULL,

    status TEXT NOT NULL DEFAULT 'active',

    release TEXT,
    environment TEXT NOT NULL,

    error_count INTEGER NOT NULL DEFAULT 0,
    crash_count INTEGER NOT NULL DEFAULT 0,
    crashed INTEGER NOT NULL DEFAULT 0,        -- Boolean: crash_count > 0

    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER,

    platform TEXT NOT NULL,
    user_agent TEXT,
    ip_address TEXT,

    sampled INTEGER NOT NULL DEFAULT 1,        -- Boolean
    sample_rate REAL NOT NULL DEFAULT 1.0,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_sessions_project_id ON sessions(project_id);
CREATE INDEX idx_sessions_release ON sessions(release);
CREATE INDEX idx_sessions_started_at ON sessions(started_at);
CREATE INDEX idx_sessions_person_id ON sessions(person_id);
CREATE INDEX idx_sessions_status ON sessions(status);

-- Hourly aggregates (retained forever)
CREATE TABLE session_aggregates (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,

    release TEXT,
    environment TEXT NOT NULL,
    hour TEXT NOT NULL,                        -- ISO8601 truncated to hour

    total_sessions INTEGER NOT NULL DEFAULT 0,
    exited_sessions INTEGER NOT NULL DEFAULT 0,
    crashed_sessions INTEGER NOT NULL DEFAULT 0,
    abnormal_sessions INTEGER NOT NULL DEFAULT 0,
    errored_sessions INTEGER NOT NULL DEFAULT 0,

    unique_users INTEGER NOT NULL DEFAULT 0,
    crashed_users INTEGER NOT NULL DEFAULT 0,

    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,

    total_errors INTEGER NOT NULL DEFAULT 0,
    total_crashes INTEGER NOT NULL DEFAULT 0,

    updated_at TEXT NOT NULL,

    UNIQUE(project_id, release, environment, hour)
);

CREATE INDEX idx_session_aggregates_project_id ON session_aggregates(project_id);
CREATE INDEX idx_session_aggregates_release ON session_aggregates(release);
CREATE INDEX idx_session_aggregates_hour ON session_aggregates(hour);
CREATE INDEX idx_session_aggregates_lookup
ON session_aggregates(project_id, release, environment, hour);
```

---

## 12. Configuration

### 12.1 Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_SESSIONS_ENABLED` | boolean | Enable session tracking | `true` |
| `LOOM_SESSIONS_RETENTION_DAYS` | integer | Individual session retention | `30` |
| `LOOM_SESSIONS_DEFAULT_SAMPLE_RATE` | float | Default sample rate | `1.0` |
| `LOOM_SESSIONS_ALWAYS_STORE_CRASHED` | boolean | Always store crashed sessions | `true` |
| `LOOM_SESSIONS_AGGREGATION_INTERVAL` | duration | How often to aggregate | `1h` |

---

## 13. Permissions

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| View release health | ✓ | ✓ | ✓ (all) |
| View sessions | ✓ | ✓ | ✓ (all) |
| Configure sampling | ✓ | ✗ | ✓ |
| Send sessions (SDK) | API Key | API Key | API Key |

---

## 14. Implementation Phases

### Phase 1: Core Types & Database (2-3 hours)

- [ ] Create `loom-sessions-core` crate
- [ ] Define Session, SessionAggregate, ReleaseHealth
- [ ] Add database migration
- [ ] Create repository layer

### Phase 2: Session Ingestion (2-3 hours)

- [ ] POST `/api/sessions/start` endpoint
- [ ] POST `/api/sessions/end` endpoint
- [ ] Session creation and updates
- [ ] Sampling logic

### Phase 3: Crash SDK Integration (2-3 hours)

- [ ] Add SessionTracker to `loom-crash`
- [ ] Auto-start on client init
- [ ] Auto-end on shutdown
- [ ] Crash recording integration

### Phase 4: TypeScript SDK (2-3 hours)

- [ ] Add session tracking to `@loom/crash`
- [ ] Browser visibility handling
- [ ] sendBeacon for reliable end
- [ ] Sampling support

### Phase 5: Aggregation Job (2-3 hours)

- [ ] Hourly aggregation job
- [ ] Upsert aggregate records
- [ ] Cleanup old sessions job

### Phase 6: Release Health API (2-3 hours)

- [ ] GET releases endpoint
- [ ] GET release health endpoint
- [ ] Health trend endpoint
- [ ] Release comparison

### Phase 7: SSE Streaming (1-2 hours)

- [ ] Session event streaming
- [ ] Release health change events

### Phase 8: Session Queries (2-3 hours)

- [ ] List sessions endpoint
- [ ] Session detail endpoint
- [ ] By-person query
- [ ] Pagination and filtering

---

## 15. Rust Dependencies

```toml
# loom-sessions-core
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }
loom-secret = { path = "../loom-secret" }

# loom-sessions (part of loom-crash, not separate crate)
# Session tracking is integrated into the crash SDK

# loom-server-sessions
[dependencies]
loom-sessions-core = { path = "../loom-sessions-core" }
loom-db = { path = "../loom-db" }
loom-jobs = { path = "../loom-jobs" }
axum = "0.8"
sqlx = { version = "0.8", features = ["sqlite"] }
tokio = { version = "1", features = ["sync"] }
tokio-stream = "0.1"
murmur3 = "0.5"
```

---

## Appendix A: Metrics Definitions

| Metric | Formula | Description |
|--------|---------|-------------|
| **Crash-free session rate** | (total - crashed) / total × 100 | % of sessions without unhandled errors |
| **Crash-free user rate** | (users - crashed_users) / users × 100 | % of users without crashes |
| **Adoption rate** | release_sessions / all_sessions × 100 | % of traffic on this release |
| **Error rate** | errored / total × 100 | % of sessions with handled errors |

---

## Appendix B: Comparison with Sentry Sessions

| Feature | Loom Sessions | Sentry Sessions |
|---------|---------------|-----------------|
| Individual storage | 30 days | 90 days |
| Aggregates | Hourly | Hourly |
| Sampling | Configurable | Configurable |
| Crashed always stored | Yes | Yes |
| User tracking | Via person_id | Via user ID |
| Release health | Yes | Yes |
| Adoption tracking | Yes | Yes |
| Session replay | No (separate) | Yes (paid) |
