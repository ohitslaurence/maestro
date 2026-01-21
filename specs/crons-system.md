<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Cron Monitoring System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-18

---

## 1. Overview

### Purpose

This specification defines a cron/job monitoring system for Loom. Products built on Loom can monitor scheduled jobs, background tasks, and cron jobs to detect missed runs, failures, and performance degradation. The system supports both simple ping-based monitoring (like Healthchecks.io) and SDK-based check-ins (like Sentry Crons).

### Goals

- **Dual integration approach**: Ping URLs for simple scripts, SDK for application code
- **Missed run detection**: Alert when expected job doesn't run
- **Failure tracking**: Capture job errors with optional crash linking
- **Duration monitoring**: Track job runtime, alert on timeout
- **Schedule support**: Cron expressions and fixed intervals
- **Integration** with `loom-crash` for error linking and `loom-jobs` for auto-instrumentation
- **Real-time updates** via SSE streaming for status changes

### Non-Goals (v1)

- Complex dependency graphs between jobs
- Job scheduling/execution (use existing `loom-jobs`)
- Distributed tracing within jobs (future: `loom-traces`)
- Alerting notifications (deferred)

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-crons-core/                  # Shared types for cron monitoring
│   ├── src/
│   │   ├── lib.rs
│   │   ├── monitor.rs                # Monitor, MonitorSchedule
│   │   ├── checkin.rs                # CheckIn, CheckInStatus
│   │   └── error.rs                  # Error types
│   └── Cargo.toml
├── loom-crons/                       # Rust SDK client
│   ├── src/
│   │   ├── lib.rs
│   │   ├── client.rs                 # CronsClient
│   │   ├── checkin.rs                # Check-in helpers
│   │   └── integration.rs            # loom-jobs integration
│   └── Cargo.toml
├── loom-server-crons/                # Server-side API and monitoring
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs                 # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── monitors.rs           # Monitor CRUD
│   │   │   ├── checkins.rs           # Check-in endpoints
│   │   │   └── ping.rs               # Simple ping endpoints
│   │   ├── repository.rs             # Database repository
│   │   ├── scheduler.rs              # Missed run detection job
│   │   ├── sse.rs                    # SSE streaming
│   │   └── cron_parser.rs            # Cron expression parsing
│   └── Cargo.toml

web/
├── packages/
│   └── crons/                        # @loom/crons - TypeScript SDK
│       ├── src/
│       │   ├── index.ts
│       │   ├── client.ts             # CronsClient
│       │   └── checkin.ts            # Check-in helpers
│       └── package.json
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Clients                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│    Shell Script          Application Code           loom-jobs               │
│    ┌──────────┐          ┌──────────────┐          ┌──────────────┐        │
│    │ curl     │          │ loom-crons   │          │ Auto-        │        │
│    │ /ping/x  │          │ SDK          │          │ instrumented │        │
│    └────┬─────┘          └──────┬───────┘          └──────┬───────┘        │
│         │                       │                         │                 │
└─────────┼───────────────────────┼─────────────────────────┼─────────────────┘
          │                       │                         │
          ▼                       ▼                         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐             │
│  │ Ping Routes     │  │ Check-in Routes │  │ Monitor Routes  │             │
│  │ /ping/{key}     │  │ /api/crons/*    │  │ /api/crons/*    │             │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘             │
│           │                    │                    │                       │
│           └────────────────────┼────────────────────┘                       │
│                                ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                    Missed Run Detector                                │  │
│  │    Background job checking for overdue monitors every minute         │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                │                                            │
│                                ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                         SSE Broadcaster                               │  │
│  │    Real-time status updates, missed alerts, failures                 │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                │                                            │
│                                ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                    Crash Integration                                  │  │
│  │    Link failed check-ins to crash events                             │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  cron_monitors    cron_checkins    cron_monitor_stats                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Entities

### 3.1 Monitor

A monitored job or scheduled task.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub id: MonitorId,
    pub org_id: OrgId,
    pub project_id: Option<ProjectId>,    // Optional link to crash project

    // Identification
    pub slug: String,                      // URL-safe: "daily-cleanup"
    pub name: String,                      // Human-readable: "Daily Cleanup Job"
    pub description: Option<String>,

    // Status
    pub status: MonitorStatus,
    pub health: MonitorHealth,

    // Schedule configuration
    pub schedule: MonitorSchedule,
    pub timezone: String,                  // IANA timezone: "America/New_York"

    // Tolerance settings
    pub checkin_margin_minutes: u32,       // Grace period before marking missed (default: 5)
    pub max_runtime_minutes: Option<u32>,  // Alert if job exceeds this duration

    // Ping URL (for simple integration)
    pub ping_key: String,                  // UUID for /ping/{key}

    // Environment filter
    pub environments: Vec<String>,         // ["production"] or empty for all

    // Stats (denormalized for quick display)
    pub last_checkin_at: Option<DateTime<Utc>>,
    pub last_checkin_status: Option<CheckInStatus>,
    pub next_expected_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub total_checkins: u64,
    pub total_failures: u64,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MonitorStatus {
    Active,      // Monitoring enabled
    Paused,      // Temporarily disabled (won't alert on missed)
    Disabled,    // Fully disabled
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MonitorHealth {
    Healthy,     // Recent check-in was OK
    Failing,     // Recent check-in was Error
    Missed,      // Expected check-in didn't arrive
    Timeout,     // Job exceeded max_runtime
    Unknown,     // No check-ins yet
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MonitorSchedule {
    /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
    Cron { expression: String },

    /// Fixed interval (e.g., every 30 minutes)
    Interval { minutes: u32 },
}
```

### 3.2 CheckIn

A single job execution report.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckIn {
    pub id: CheckInId,
    pub monitor_id: MonitorId,

    // Status
    pub status: CheckInStatus,

    // Timing
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: DateTime<Utc>,
    pub duration_ms: Option<u64>,

    // Context
    pub environment: Option<String>,       // "production", "staging"
    pub release: Option<String>,           // App version

    // Error details (for failed check-ins)
    pub exit_code: Option<i32>,
    pub output: Option<String>,            // Truncated stdout/stderr (max 10KB)
    pub crash_event_id: Option<CrashEventId>, // Link to crash system

    // Source
    pub source: CheckInSource,

    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckInStatus {
    InProgress,  // Job started, not yet finished
    Ok,          // Job completed successfully
    Error,       // Job failed (explicit error)
    Missed,      // System-generated: expected ping didn't arrive
    Timeout,     // System-generated: max_runtime exceeded
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckInSource {
    Ping,        // Simple HTTP ping
    Sdk,         // SDK check-in
    Manual,      // Manual via API/UI
    System,      // System-generated (missed, timeout)
}
```

### 3.3 MonitorStats

Aggregated statistics for dashboards.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorStats {
    pub monitor_id: MonitorId,
    pub period: StatsPeriod,

    pub total_checkins: u64,
    pub successful_checkins: u64,
    pub failed_checkins: u64,
    pub missed_checkins: u64,
    pub timeout_checkins: u64,

    pub avg_duration_ms: Option<u64>,
    pub p50_duration_ms: Option<u64>,
    pub p95_duration_ms: Option<u64>,
    pub max_duration_ms: Option<u64>,

    pub uptime_percentage: f64,            // (ok / total) * 100

    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StatsPeriod {
    Day,
    Week,
    Month,
}
```

---

## 4. Ping-Based Monitoring

### 4.1 Simple Ping URLs

For shell scripts, cron jobs, and external services that just need to "phone home":

```bash
# Success ping (job completed OK)
curl https://loom.example.com/ping/abc123-def456

# Start ping (job starting)
curl https://loom.example.com/ping/abc123-def456/start

# Fail ping (job failed)
curl https://loom.example.com/ping/abc123-def456/fail

# Ping with exit code
curl "https://loom.example.com/ping/abc123-def456?exit_code=1"

# Ping with output (POST)
curl -X POST https://loom.example.com/ping/abc123-def456 \
  -d "Job completed. Processed 1000 records."
```

### 4.2 Ping Endpoints

```rust
// GET /ping/{key} - Simple success ping
async fn ping_success(
    Path(key): Path<String>,
    Query(params): Query<PingParams>,
) -> Result<impl IntoResponse, Error> {
    let monitor = find_monitor_by_ping_key(&key).await?;

    create_checkin(CheckIn {
        monitor_id: monitor.id,
        status: CheckInStatus::Ok,
        finished_at: Utc::now(),
        exit_code: params.exit_code,
        source: CheckInSource::Ping,
        ..Default::default()
    }).await?;

    update_monitor_health(&monitor.id, MonitorHealth::Healthy).await?;

    Ok(StatusCode::OK)
}

// GET /ping/{key}/start - Job starting
async fn ping_start(
    Path(key): Path<String>,
) -> Result<impl IntoResponse, Error> {
    let monitor = find_monitor_by_ping_key(&key).await?;

    let checkin = create_checkin(CheckIn {
        monitor_id: monitor.id,
        status: CheckInStatus::InProgress,
        started_at: Some(Utc::now()),
        finished_at: Utc::now(), // Will be updated on completion
        source: CheckInSource::Ping,
        ..Default::default()
    }).await?;

    // Return check-in ID for correlation
    Ok((StatusCode::OK, Json(json!({ "checkin_id": checkin.id }))))
}

// GET /ping/{key}/fail - Job failed
async fn ping_fail(
    Path(key): Path<String>,
    Query(params): Query<PingParams>,
) -> Result<impl IntoResponse, Error> {
    let monitor = find_monitor_by_ping_key(&key).await?;

    create_checkin(CheckIn {
        monitor_id: monitor.id,
        status: CheckInStatus::Error,
        finished_at: Utc::now(),
        exit_code: params.exit_code,
        source: CheckInSource::Ping,
        ..Default::default()
    }).await?;

    update_monitor_health(&monitor.id, MonitorHealth::Failing).await?;
    broadcast_monitor_failure(&monitor).await?;

    Ok(StatusCode::OK)
}

// POST /ping/{key} - Ping with body (output capture)
async fn ping_with_body(
    Path(key): Path<String>,
    Query(params): Query<PingParams>,
    body: String,
) -> Result<impl IntoResponse, Error> {
    let monitor = find_monitor_by_ping_key(&key).await?;

    let status = if params.exit_code.unwrap_or(0) == 0 {
        CheckInStatus::Ok
    } else {
        CheckInStatus::Error
    };

    create_checkin(CheckIn {
        monitor_id: monitor.id,
        status,
        finished_at: Utc::now(),
        exit_code: params.exit_code,
        output: Some(truncate(&body, 10 * 1024)), // 10KB max
        source: CheckInSource::Ping,
        ..Default::default()
    }).await?;

    let health = if status == CheckInStatus::Ok {
        MonitorHealth::Healthy
    } else {
        MonitorHealth::Failing
    };
    update_monitor_health(&monitor.id, health).await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize)]
struct PingParams {
    exit_code: Option<i32>,
}
```

### 4.3 Ping URL Format

When a monitor is created, generate a unique ping key:

```rust
impl Monitor {
    pub fn generate_ping_key() -> String {
        // UUIDv4 for unpredictability
        Uuid::new_v4().to_string()
    }

    pub fn ping_url(&self, base_url: &str) -> String {
        format!("{}/ping/{}", base_url, self.ping_key)
    }
}
```

Example URLs:
```
https://loom.example.com/ping/7a3b9f2e-1c4d-8a5b-6e0f-3c2d1a4b5c6d
https://loom.example.com/ping/7a3b9f2e-1c4d-8a5b-6e0f-3c2d1a4b5c6d/start
https://loom.example.com/ping/7a3b9f2e-1c4d-8a5b-6e0f-3c2d1a4b5c6d/fail
```

---

## 5. SDK-Based Monitoring

### 5.1 Rust SDK (`loom-crons`)

```rust
use loom_crons::{CronsClient, CheckIn, CheckInStatus};
use loom_crash::CrashClient;

// Initialize
let crons = CronsClient::builder()
    .api_key("loom_crons_xxx")
    .base_url("https://loom.example.com")
    .crash_client(&crash)  // Optional: link errors to crashes
    .build()?;

// Manual check-in pattern
let checkin_id = crons.checkin_start("daily-cleanup").await?;

match run_daily_cleanup() {
    Ok(result) => {
        crons.checkin_ok(checkin_id, CheckInOk {
            duration_ms: Some(elapsed.as_millis() as u64),
            output: Some(format!("Processed {} records", result.count)),
        }).await?;
    }
    Err(e) => {
        // Capture error in crash system
        let crash_id = crash.capture_error(&e).await?;

        crons.checkin_error(checkin_id, CheckInError {
            duration_ms: Some(elapsed.as_millis() as u64),
            exit_code: Some(1),
            output: Some(e.to_string()),
            crash_event_id: Some(crash_id),
        }).await?;
    }
}

// Convenience wrapper
crons.with_monitor("daily-cleanup", || async {
    run_daily_cleanup().await
}).await?;
```

### 5.2 SDK Implementation

```rust
pub struct CronsClient {
    http: HttpClient,
    api_key: String,
    base_url: String,
    crash_client: Option<CrashClient>,
}

impl CronsClient {
    /// Start a check-in (job starting)
    pub async fn checkin_start(&self, monitor_slug: &str) -> Result<CheckInId> {
        let response = self.http
            .post(&format!("{}/api/crons/monitors/{}/checkins", self.base_url, monitor_slug))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&CheckInRequest {
                status: CheckInStatus::InProgress,
                started_at: Some(Utc::now()),
                ..Default::default()
            })
            .send()
            .await?;

        let body: CheckInResponse = response.json().await?;
        Ok(body.id)
    }

    /// Complete a check-in successfully
    pub async fn checkin_ok(&self, id: CheckInId, details: CheckInOk) -> Result<()> {
        self.http
            .patch(&format!("{}/api/crons/checkins/{}", self.base_url, id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&CheckInUpdateRequest {
                status: CheckInStatus::Ok,
                finished_at: Some(Utc::now()),
                duration_ms: details.duration_ms,
                output: details.output,
                ..Default::default()
            })
            .send()
            .await?;

        Ok(())
    }

    /// Complete a check-in with error
    pub async fn checkin_error(&self, id: CheckInId, details: CheckInError) -> Result<()> {
        self.http
            .patch(&format!("{}/api/crons/checkins/{}", self.base_url, id))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&CheckInUpdateRequest {
                status: CheckInStatus::Error,
                finished_at: Some(Utc::now()),
                duration_ms: details.duration_ms,
                exit_code: details.exit_code,
                output: details.output,
                crash_event_id: details.crash_event_id,
            })
            .send()
            .await?;

        Ok(())
    }

    /// Convenience wrapper that handles check-in lifecycle
    pub async fn with_monitor<F, Fut, T, E>(&self, slug: &str, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::error::Error + 'static,
    {
        let start = Instant::now();
        let checkin_id = self.checkin_start(slug).await?;

        match f().await {
            Ok(result) => {
                self.checkin_ok(checkin_id, CheckInOk {
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    output: None,
                }).await?;
                Ok(result)
            }
            Err(e) => {
                // Optionally capture in crash system
                let crash_id = if let Some(crash) = &self.crash_client {
                    crash.capture_error(&e).await.ok()
                } else {
                    None
                };

                self.checkin_error(checkin_id, CheckInError {
                    duration_ms: Some(start.elapsed().as_millis() as u64),
                    exit_code: Some(1),
                    output: Some(e.to_string()),
                    crash_event_id: crash_id,
                }).await?;

                Err(Error::JobFailed(e.to_string()))
            }
        }
    }
}
```

### 5.3 TypeScript SDK (`@loom/crons`)

```typescript
import { CronsClient } from '@loom/crons';
import { CrashClient } from '@loom/crash';

const crons = new CronsClient({
  apiKey: 'loom_crons_xxx',
  baseUrl: 'https://loom.example.com',
  crashClient: crash,  // Optional
});

// Manual pattern
const checkinId = await crons.checkinStart('email-digest');

try {
  await sendEmailDigest();
  await crons.checkinOk(checkinId, {
    durationMs: Date.now() - startTime,
    output: 'Sent 150 emails',
  });
} catch (error) {
  const crashId = await crash.captureException(error);
  await crons.checkinError(checkinId, {
    durationMs: Date.now() - startTime,
    output: error.message,
    crashEventId: crashId,
  });
}

// Convenience wrapper
await crons.withMonitor('email-digest', async () => {
  await sendEmailDigest();
});
```

### 5.4 Integration with `loom-jobs`

Auto-instrument the existing job scheduler:

```rust
// In loom-jobs crate
use loom_crons::CronsClient;

impl JobRunner {
    pub fn with_cron_monitoring(mut self, crons: CronsClient) -> Self {
        self.crons_client = Some(crons);
        self
    }

    async fn run_job(&self, job: &Job) -> Result<()> {
        let start = Instant::now();

        // Auto check-in if crons client configured
        let checkin_id = if let Some(crons) = &self.crons_client {
            Some(crons.checkin_start(&job.name).await?)
        } else {
            None
        };

        let result = job.execute().await;

        // Auto complete check-in
        if let (Some(crons), Some(id)) = (&self.crons_client, checkin_id) {
            let duration_ms = start.elapsed().as_millis() as u64;

            match &result {
                Ok(_) => {
                    crons.checkin_ok(id, CheckInOk {
                        duration_ms: Some(duration_ms),
                        output: None,
                    }).await?;
                }
                Err(e) => {
                    crons.checkin_error(id, CheckInError {
                        duration_ms: Some(duration_ms),
                        exit_code: Some(1),
                        output: Some(e.to_string()),
                        crash_event_id: None,
                    }).await?;
                }
            }
        }

        result
    }
}
```

---

## 6. Schedule Parsing

### 6.1 Cron Expression Support

Support standard 5-field cron expressions:

```
┌───────────── minute (0 - 59)
│ ┌───────────── hour (0 - 23)
│ │ ┌───────────── day of month (1 - 31)
│ │ │ ┌───────────── month (1 - 12)
│ │ │ │ ┌───────────── day of week (0 - 6) (Sunday = 0)
│ │ │ │ │
* * * * *
```

Examples:
- `0 0 * * *` - Daily at midnight
- `*/15 * * * *` - Every 15 minutes
- `0 9 * * 1-5` - 9am on weekdays
- `0 0 1 * *` - First of every month

### 6.2 Next Run Calculation

```rust
use cron::Schedule;
use chrono_tz::Tz;

pub fn calculate_next_expected(
    schedule: &MonitorSchedule,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    match schedule {
        MonitorSchedule::Cron { expression } => {
            let schedule = Schedule::from_str(expression)?;
            let tz: Tz = timezone.parse()?;

            let local_after = after.with_timezone(&tz);
            let next_local = schedule
                .after(&local_after)
                .next()
                .ok_or(Error::NoNextRun)?;

            Ok(next_local.with_timezone(&Utc))
        }
        MonitorSchedule::Interval { minutes } => {
            Ok(after + Duration::minutes(*minutes as i64))
        }
    }
}
```

### 6.3 Grace Period

The `checkin_margin_minutes` provides a grace period:

```
Expected at: 00:00:00
Margin: 5 minutes

├──────────────────────┬───────────────────────┤
│    On-time window    │   Grace period        │
│    00:00:00          │   00:00:00 - 00:05:00 │
├──────────────────────┴───────────────────────┤
│    After 00:05:00 = MISSED                   │
└──────────────────────────────────────────────┘
```

---

## 7. Missed Run Detection

### 7.1 Background Scheduler

A job that runs every minute to detect missed check-ins:

```rust
// Runs every minute via loom-jobs
pub async fn check_missed_monitors(db: &Database, sse: &SseBroadcaster) -> Result<()> {
    let now = Utc::now();

    // Find monitors that are overdue
    let overdue_monitors = sqlx::query_as!(
        Monitor,
        r#"
        SELECT * FROM cron_monitors
        WHERE status = 'active'
          AND next_expected_at IS NOT NULL
          AND datetime(next_expected_at, '+' || checkin_margin_minutes || ' minutes') < ?
          AND (last_checkin_at IS NULL OR last_checkin_at < next_expected_at)
        "#,
        now.to_rfc3339()
    )
    .fetch_all(db)
    .await?;

    for monitor in overdue_monitors {
        // Create synthetic "missed" check-in
        let checkin = create_checkin(CheckIn {
            monitor_id: monitor.id.clone(),
            status: CheckInStatus::Missed,
            finished_at: now,
            source: CheckInSource::System,
            ..Default::default()
        }).await?;

        // Update monitor state
        update_monitor(
            &monitor.id,
            MonitorUpdate {
                health: Some(MonitorHealth::Missed),
                consecutive_failures: Some(monitor.consecutive_failures + 1),
                last_checkin_at: Some(now),
                last_checkin_status: Some(CheckInStatus::Missed),
                next_expected_at: Some(calculate_next_expected(
                    &monitor.schedule,
                    &monitor.timezone,
                    now,
                )?),
            },
        ).await?;

        // Broadcast via SSE
        sse.broadcast(SseEvent::MonitorMissed {
            monitor_id: monitor.id,
            monitor_slug: monitor.slug,
            monitor_name: monitor.name,
            expected_at: monitor.next_expected_at,
            timestamp: now,
        }).await?;
    }

    Ok(())
}
```

### 7.2 Timeout Detection

For jobs with `max_runtime_minutes`, detect timeouts:

```rust
pub async fn check_timeout_monitors(db: &Database, sse: &SseBroadcaster) -> Result<()> {
    let now = Utc::now();

    // Find in-progress check-ins that exceeded max runtime
    let timed_out = sqlx::query!(
        r#"
        SELECT c.*, m.max_runtime_minutes, m.slug, m.name
        FROM cron_checkins c
        JOIN cron_monitors m ON m.id = c.monitor_id
        WHERE c.status = 'in_progress'
          AND m.max_runtime_minutes IS NOT NULL
          AND datetime(c.started_at, '+' || m.max_runtime_minutes || ' minutes') < ?
        "#,
        now.to_rfc3339()
    )
    .fetch_all(db)
    .await?;

    for record in timed_out {
        // Update check-in to timeout
        update_checkin(
            &record.id,
            CheckInUpdate {
                status: Some(CheckInStatus::Timeout),
                finished_at: Some(now),
            },
        ).await?;

        // Update monitor health
        update_monitor(
            &record.monitor_id,
            MonitorUpdate {
                health: Some(MonitorHealth::Timeout),
                consecutive_failures: Some(record.consecutive_failures + 1),
            },
        ).await?;

        // Broadcast
        sse.broadcast(SseEvent::MonitorTimeout {
            monitor_id: record.monitor_id,
            monitor_slug: record.slug,
            checkin_id: record.id,
            started_at: record.started_at,
            max_runtime_minutes: record.max_runtime_minutes,
        }).await?;
    }

    Ok(())
}
```

---

## 8. API Endpoints

### 8.1 Ping Endpoints (No Auth Required)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/ping/{key}` | Success ping |
| GET | `/ping/{key}/start` | Job starting |
| GET | `/ping/{key}/fail` | Job failed |
| POST | `/ping/{key}` | Ping with body (output) |

### 8.2 Monitor Management (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crons/monitors` | List monitors |
| POST | `/api/crons/monitors` | Create monitor |
| GET | `/api/crons/monitors/{slug}` | Get monitor |
| PATCH | `/api/crons/monitors/{slug}` | Update monitor |
| DELETE | `/api/crons/monitors/{slug}` | Delete monitor |
| POST | `/api/crons/monitors/{slug}/pause` | Pause monitoring |
| POST | `/api/crons/monitors/{slug}/resume` | Resume monitoring |

### 8.3 Check-in Management (Requires API Key or User Auth)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/crons/monitors/{slug}/checkins` | Create check-in (SDK) |
| GET | `/api/crons/monitors/{slug}/checkins` | List check-ins |
| GET | `/api/crons/checkins/{id}` | Get check-in |
| PATCH | `/api/crons/checkins/{id}` | Update check-in |

### 8.4 Stats (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crons/monitors/{slug}/stats` | Get monitor stats |
| GET | `/api/crons/stats/overview` | Org-wide overview |

### 8.5 Real-time (Requires User Auth)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/crons/stream` | SSE stream for all monitors |
| GET | `/api/crons/monitors/{slug}/stream` | SSE stream for single monitor |

---

## 9. SSE Streaming

### 9.1 Events

| Event | Description |
|-------|-------------|
| `checkin.started` | Job started (in_progress) |
| `checkin.ok` | Job completed successfully |
| `checkin.error` | Job failed |
| `monitor.missed` | Expected check-in didn't arrive |
| `monitor.timeout` | Job exceeded max runtime |
| `monitor.healthy` | Monitor recovered from failure |
| `heartbeat` | Keep-alive (every 30s) |

### 9.2 Event Format

```json
{
  "event": "monitor.missed",
  "data": {
    "monitor_id": "mon_xxx",
    "monitor_slug": "daily-cleanup",
    "monitor_name": "Daily Cleanup Job",
    "expected_at": "2026-01-18T00:00:00Z",
    "consecutive_failures": 2,
    "timestamp": "2026-01-18T00:05:01Z"
  }
}
```

---

## 10. Database Schema

### 10.1 Migration: `XXX_cron_monitoring.sql`

```sql
-- Monitors
CREATE TABLE cron_monitors (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT REFERENCES crash_projects(id) ON DELETE SET NULL,

    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    status TEXT NOT NULL DEFAULT 'active',
    health TEXT NOT NULL DEFAULT 'unknown',

    schedule_type TEXT NOT NULL,          -- 'cron' or 'interval'
    schedule_value TEXT NOT NULL,         -- Cron expression or interval minutes
    timezone TEXT NOT NULL DEFAULT 'UTC',

    checkin_margin_minutes INTEGER NOT NULL DEFAULT 5,
    max_runtime_minutes INTEGER,

    ping_key TEXT NOT NULL UNIQUE,

    environments TEXT NOT NULL DEFAULT '[]',  -- JSON array

    last_checkin_at TEXT,
    last_checkin_status TEXT,
    next_expected_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    total_checkins INTEGER NOT NULL DEFAULT 0,
    total_failures INTEGER NOT NULL DEFAULT 0,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    UNIQUE(org_id, slug)
);

CREATE INDEX idx_cron_monitors_org_id ON cron_monitors(org_id);
CREATE INDEX idx_cron_monitors_ping_key ON cron_monitors(ping_key);
CREATE INDEX idx_cron_monitors_status ON cron_monitors(status);
CREATE INDEX idx_cron_monitors_next_expected ON cron_monitors(next_expected_at);

-- Check-ins
CREATE TABLE cron_checkins (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES cron_monitors(id) ON DELETE CASCADE,

    status TEXT NOT NULL,

    started_at TEXT,
    finished_at TEXT NOT NULL,
    duration_ms INTEGER,

    environment TEXT,
    release TEXT,

    exit_code INTEGER,
    output TEXT,
    crash_event_id TEXT,

    source TEXT NOT NULL,

    created_at TEXT NOT NULL
);

CREATE INDEX idx_cron_checkins_monitor_id ON cron_checkins(monitor_id);
CREATE INDEX idx_cron_checkins_status ON cron_checkins(status);
CREATE INDEX idx_cron_checkins_finished_at ON cron_checkins(finished_at);

-- Aggregated stats (daily rollups)
CREATE TABLE cron_monitor_stats (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES cron_monitors(id) ON DELETE CASCADE,
    date TEXT NOT NULL,                   -- "2026-01-18"

    total_checkins INTEGER NOT NULL DEFAULT 0,
    successful_checkins INTEGER NOT NULL DEFAULT 0,
    failed_checkins INTEGER NOT NULL DEFAULT 0,
    missed_checkins INTEGER NOT NULL DEFAULT 0,
    timeout_checkins INTEGER NOT NULL DEFAULT 0,

    avg_duration_ms INTEGER,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,

    updated_at TEXT NOT NULL,

    UNIQUE(monitor_id, date)
);

CREATE INDEX idx_cron_monitor_stats_monitor_id ON cron_monitor_stats(monitor_id);
CREATE INDEX idx_cron_monitor_stats_date ON cron_monitor_stats(date);
```

---

## 11. API Key Format

Following the existing pattern:

| Type | Prefix | Use Case |
|------|--------|----------|
| SDK | `loom_crons_` | SDK check-ins |

Note: Ping URLs use the `ping_key` UUID directly without authentication for simplicity.

---

## 12. Configuration

### 12.1 Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_CRONS_ENABLED` | boolean | Enable cron monitoring | `true` |
| `LOOM_CRONS_CHECK_INTERVAL_SECS` | integer | Missed run check interval | `60` |
| `LOOM_CRONS_DEFAULT_MARGIN_MINUTES` | integer | Default grace period | `5` |
| `LOOM_CRONS_MAX_OUTPUT_BYTES` | integer | Max output to store | `10240` |
| `LOOM_CRONS_CHECKIN_RETENTION_DAYS` | integer | Check-in retention | `90` |

---

## 13. Audit Events

| Event | Description |
|-------|-------------|
| `CronMonitorCreated` | Monitor created |
| `CronMonitorUpdated` | Monitor settings changed |
| `CronMonitorDeleted` | Monitor deleted |
| `CronMonitorPaused` | Monitoring paused |
| `CronMonitorResumed` | Monitoring resumed |

---

## 14. Permissions

| Action | Org Admin | Org Member | Super Admin |
|--------|-----------|------------|-------------|
| List monitors | ✓ | ✓ | ✓ (all) |
| Create monitor | ✓ | ✓ | ✓ |
| Update monitor | ✓ | ✓ | ✓ |
| Delete monitor | ✓ | ✗ | ✓ |
| View check-ins | ✓ | ✓ | ✓ |
| Send check-ins | API Key | API Key | API Key |

---

## 15. Implementation Phases

### Phase 1: Core Types & Database (2-3 hours)

- [ ] Create `loom-crons-core` crate
- [ ] Define Monitor, CheckIn types
- [ ] Add database migration
- [ ] Create repository layer

### Phase 2: Ping Endpoints (2-3 hours)

- [ ] GET `/ping/{key}` success
- [ ] GET `/ping/{key}/start`
- [ ] GET `/ping/{key}/fail`
- [ ] POST `/ping/{key}` with body
- [ ] Ping key lookup

### Phase 3: Monitor CRUD (2-3 hours)

- [ ] Monitor CRUD handlers
- [ ] Slug validation
- [ ] Schedule parsing (cron + interval)
- [ ] Next expected calculation

### Phase 4: Check-in API (2-3 hours)

- [ ] Check-in creation (SDK)
- [ ] Check-in update
- [ ] Check-in listing
- [ ] API key authentication

### Phase 5: Missed Run Detection (2-3 hours)

- [ ] Background scheduler job
- [ ] Overdue monitor detection
- [ ] Synthetic missed check-in creation
- [ ] Health state updates

### Phase 6: Timeout Detection (1-2 hours)

- [ ] In-progress timeout check
- [ ] Timeout check-in creation
- [ ] Health state updates

### Phase 7: SSE Streaming (2-3 hours)

- [ ] SSE endpoint
- [ ] Event broadcasting
- [ ] Heartbeat
- [ ] Per-monitor streams

### Phase 8: Stats & Rollups (2-3 hours)

- [ ] Daily rollup job
- [ ] Stats calculation
- [ ] Stats API endpoints
- [ ] Overview endpoint

### Phase 9: Rust SDK (2-3 hours)

- [ ] Create `loom-crons` crate
- [ ] CronsClient implementation
- [ ] Check-in helpers
- [ ] `with_monitor` convenience method
- [ ] loom-jobs integration

### Phase 10: TypeScript SDK (2-3 hours)

- [ ] Create `@loom/crons` package
- [ ] CronsClient implementation
- [ ] Check-in helpers
- [ ] `withMonitor` convenience method

### Phase 11: Crash Integration (1-2 hours)

- [ ] Link check-ins to crash events
- [ ] Error capture on failure
- [ ] Cross-linking in UI

---

## 16. Rust Dependencies

```toml
# loom-crons-core
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }

# loom-crons (SDK)
[dependencies]
loom-crons-core = { path = "../loom-crons-core" }
loom-http = { path = "../loom-http" }
loom-crash = { path = "../loom-crash", optional = true }
async-trait = "0.1"
tokio = { version = "1", features = ["sync", "time"] }
tracing = "0.1"

[features]
crash = ["loom-crash"]

# loom-server-crons
[dependencies]
loom-crons-core = { path = "../loom-crons-core" }
loom-db = { path = "../loom-db" }
loom-server-audit = { path = "../loom-server-audit" }
loom-jobs = { path = "../loom-jobs" }
axum = "0.8"
sqlx = { version = "0.8", features = ["sqlite"] }
cron = "0.12"
chrono-tz = "0.8"
tokio = { version = "1", features = ["sync"] }
tokio-stream = "0.1"
```

---

## Appendix A: Shell Integration Examples

### Cron Job
```bash
# /etc/cron.d/my-job
0 0 * * * root /usr/local/bin/my-script.sh && curl -fsS https://loom.example.com/ping/xxx
```

### Script with Error Handling
```bash
#!/bin/bash
set -e

# Signal start
curl -fsS https://loom.example.com/ping/xxx/start

# Run job
if /usr/local/bin/my-script.sh 2>&1 | tee /tmp/job-output.txt; then
    # Success
    curl -fsS -X POST https://loom.example.com/ping/xxx \
        -d @/tmp/job-output.txt
else
    # Failure
    curl -fsS -X POST "https://loom.example.com/ping/xxx/fail?exit_code=$?" \
        -d @/tmp/job-output.txt
fi
```

### Kubernetes CronJob
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: daily-cleanup
spec:
  schedule: "0 0 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: cleanup
            image: my-app:latest
            command:
            - /bin/sh
            - -c
            - |
              curl -fsS https://loom.example.com/ping/xxx/start
              if /app/cleanup.sh; then
                curl -fsS https://loom.example.com/ping/xxx
              else
                curl -fsS https://loom.example.com/ping/xxx/fail
              fi
          restartPolicy: OnFailure
```

---

## Appendix B: Comparison with Alternatives

| Feature | Loom Crons | Healthchecks.io | Sentry Crons |
|---------|------------|-----------------|--------------|
| Ping URLs | ✓ | ✓ | ✗ |
| SDK check-ins | ✓ | ✓ | ✓ |
| Missed detection | ✓ | ✓ | ✓ |
| Timeout detection | ✓ | ✗ | ✓ |
| Error linking | ✓ (crash) | ✗ | ✓ |
| Output capture | ✓ | ✓ | ✗ |
| Grace period | ✓ | ✓ | ✓ |
| Cron + interval | ✓ | ✓ | ✓ |
| Free | ✓ | 20 checks | Paid |
