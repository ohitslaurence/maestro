# Job Scheduler System Specification

**Status:** Implemented  
**Version:** 1.0  
**Last Updated:** 2026-01-01

---

## 1. Overview

### Purpose

Provide a unified background job system for loom-server with:
- Centralized management of periodic and one-shot jobs
- Persistence of job history and state in SQLite
- Admin visibility into job status, history, and health
- Graceful shutdown with job cancellation
- Retry with exponential backoff on failure
- Email alerts on repeated failures
- Health endpoint integration

### Goals

- **Unified job management**: All background tasks use the same infrastructure
- **Visibility**: Admins can view job status, history, and manually trigger jobs
- **Reliability**: Jobs survive restarts, retry on failure, alert on problems
- **Graceful shutdown**: Clean cancellation of running jobs on SIGTERM

### Non-Goals

- Distributed job scheduling (single-server only)
- Priority queues or job dependencies
- Real-time job output streaming

---

## 2. Jobs Inventory

### Existing Jobs to Migrate

| Job | Current Location | Type | Interval |
|-----|-----------------|------|----------|
| Weaver Cleanup | `main.rs` / `loom_weaver::start_cleanup_task` | Periodic | Configurable (default 30 min) |
| Anthropic Token Refresh | `LlmService` / `AnthropicPool::spawn_refresh_task` | Periodic | 5 min |

### New Jobs to Add

| Job | Type | Interval | Description |
|-----|------|----------|-------------|
| Session Cleanup | Periodic | 1 hour | Delete expired sessions from database |
| OAuth State Cleanup | Periodic | 15 min | Remove expired OAuth state entries |
| Job History Cleanup | Periodic | 24 hours | Prune job runs older than 90 days |

---

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              loom-server                                     │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                         JobScheduler                                  │   │
│  │  - Manages all registered jobs                                        │   │
│  │  - Handles graceful shutdown                                          │   │
│  │  - Provides status for health checks                                  │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│         │                    │                    │                          │
│         ▼                    ▼                    ▼                          │
│  ┌────────────┐      ┌────────────┐      ┌────────────┐                     │
│  │ Periodic   │      │ Periodic   │      │ Periodic   │                     │
│  │ Job Runner │      │ Job Runner │      │ Job Runner │                     │
│  │            │      │            │      │            │                     │
│  │ - Weaver   │      │ - Token    │      │ - Session  │                     │
│  │   Cleanup  │      │   Refresh  │      │   Cleanup  │                     │
│  └─────┬──────┘      └─────┬──────┘      └─────┬──────┘                     │
│        │                   │                   │                             │
│        └───────────────────┼───────────────────┘                             │
│                            ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                      JobRepository (SQLite)                           │   │
│  │  - job_definitions: registered jobs and their config                 │   │
│  │  - job_runs: execution history (90 day retention)                    │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                            │                                                 │
│                            ▼                                                 │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                      AlertService (loom-smtp)                         │   │
│  │  - Send email on repeated job failures                                │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Database Schema

### job_definitions

```sql
CREATE TABLE job_definitions (
    id TEXT PRIMARY KEY,                    -- e.g., "weaver-cleanup"
    name TEXT NOT NULL,                     -- Human-readable name
    description TEXT,                       -- What the job does
    job_type TEXT NOT NULL,                 -- "periodic" or "one_shot"
    interval_secs INTEGER,                  -- For periodic jobs
    enabled INTEGER NOT NULL DEFAULT 1,     -- 1 = enabled, 0 = disabled
    created_at TEXT NOT NULL,               -- ISO 8601
    updated_at TEXT NOT NULL                -- ISO 8601
);
```

### job_runs

```sql
CREATE TABLE job_runs (
    id TEXT PRIMARY KEY,                    -- UUID
    job_id TEXT NOT NULL REFERENCES job_definitions(id),
    status TEXT NOT NULL,                   -- "running", "succeeded", "failed", "cancelled"
    started_at TEXT NOT NULL,               -- ISO 8601
    completed_at TEXT,                      -- ISO 8601 (null if running)
    duration_ms INTEGER,                    -- Computed on completion
    error_message TEXT,                     -- If failed
    retry_count INTEGER NOT NULL DEFAULT 0, -- Number of retries
    triggered_by TEXT NOT NULL,             -- "schedule", "manual", "retry"
    metadata TEXT                           -- JSON blob for job-specific data
);

CREATE INDEX idx_job_runs_job_id ON job_runs(job_id);
CREATE INDEX idx_job_runs_started_at ON job_runs(started_at);
CREATE INDEX idx_job_runs_status ON job_runs(status);
```

---

## 5. Core Types

### Job Trait

```rust
#[async_trait]
pub trait Job: Send + Sync {
    /// Unique identifier for this job.
    fn id(&self) -> &str;
    
    /// Human-readable name.
    fn name(&self) -> &str;
    
    /// Description of what the job does.
    fn description(&self) -> &str;
    
    /// Execute the job. Returns Ok(metadata) on success.
    async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError>;
}

pub struct JobContext {
    pub run_id: String,
    pub triggered_by: TriggerSource,
    pub db: SqlitePool,
    pub config: Arc<ServerConfig>,
}

pub struct JobOutput {
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("Job failed: {message}")]
    Failed { message: String, retryable: bool },
    
    #[error("Job cancelled")]
    Cancelled,
}
```

### Job Status

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    Schedule,
    Manual,
    Retry,
}
```

### Job Definition

```rust
pub struct JobDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub job_type: JobType,
    pub enabled: bool,
}

pub enum JobType {
    Periodic { interval: Duration },
    OneShot,
}
```

### Job Run

```rust
pub struct JobRun {
    pub id: String,
    pub job_id: String,
    pub status: JobStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub triggered_by: TriggerSource,
    pub metadata: Option<serde_json::Value>,
}
```

---

## 6. JobScheduler

### Interface

```rust
pub struct JobScheduler {
    jobs: HashMap<String, Arc<dyn Job>>,
    handles: HashMap<String, JoinHandle<()>>,
    repository: Arc<JobRepository>,
    alert_service: Option<Arc<AlertService>>,
    shutdown_tx: broadcast::Sender<()>,
}

impl JobScheduler {
    /// Create a new scheduler.
    pub fn new(
        repository: Arc<JobRepository>,
        alert_service: Option<Arc<AlertService>>,
    ) -> Self;
    
    /// Register a periodic job.
    pub fn register_periodic(
        &mut self,
        job: Arc<dyn Job>,
        interval: Duration,
    );
    
    /// Register a one-shot job (manual trigger only).
    pub fn register_one_shot(&mut self, job: Arc<dyn Job>);
    
    /// Start all registered jobs.
    pub async fn start(&mut self);
    
    /// Graceful shutdown - cancel all jobs and wait.
    pub async fn shutdown(&mut self);
    
    /// Trigger a job manually (returns run ID).
    pub async fn trigger(&self, job_id: &str) -> Result<String, JobError>;
    
    /// Cancel a running job.
    pub async fn cancel(&self, run_id: &str) -> Result<(), JobError>;
    
    /// Get status of all jobs for health check.
    pub async fn health_status(&self) -> JobHealthStatus;
    
    /// List all jobs with their last run info.
    pub async fn list_jobs(&self) -> Vec<JobInfo>;
    
    /// Get run history for a job.
    pub async fn job_history(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Vec<JobRun>;
}
```

### Periodic Job Runner

Each periodic job runs in its own tokio task:

```rust
async fn run_periodic_job(
    job: Arc<dyn Job>,
    interval: Duration,
    repository: Arc<JobRepository>,
    alert_service: Option<Arc<AlertService>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut interval_timer = tokio::time::interval(interval);
    let mut consecutive_failures = 0;
    
    loop {
        tokio::select! {
            _ = interval_timer.tick() => {
                let run_id = Uuid::new_v4().to_string();
                let result = execute_with_retry(
                    &job,
                    &run_id,
                    TriggerSource::Schedule,
                    &repository,
                ).await;
                
                match result {
                    Ok(_) => {
                        consecutive_failures = 0;
                    }
                    Err(_) => {
                        consecutive_failures += 1;
                        if consecutive_failures >= 3 {
                            if let Some(ref alert) = alert_service {
                                alert.send_job_failure_alert(&job, consecutive_failures).await;
                            }
                        }
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                info!(job_id = %job.id(), "Job received shutdown signal");
                break;
            }
        }
    }
}
```

### Retry Logic

```rust
async fn execute_with_retry(
    job: &Arc<dyn Job>,
    run_id: &str,
    triggered_by: TriggerSource,
    repository: &JobRepository,
) -> Result<JobOutput, JobError> {
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY: Duration = Duration::from_secs(5);
    
    let mut retry_count = 0;
    
    loop {
        // Record run start
        repository.create_run(run_id, job.id(), triggered_by, retry_count).await?;
        
        let ctx = JobContext { /* ... */ };
        let result = job.run(&ctx).await;
        
        match result {
            Ok(output) => {
                repository.complete_run(run_id, JobStatus::Succeeded, None, &output).await?;
                return Ok(output);
            }
            Err(JobError::Failed { message, retryable }) if retryable && retry_count < MAX_RETRIES => {
                retry_count += 1;
                let delay = BASE_DELAY * 2u32.pow(retry_count - 1);
                warn!(
                    job_id = %job.id(),
                    retry_count,
                    delay_secs = delay.as_secs(),
                    error = %message,
                    "Job failed, retrying"
                );
                repository.update_run_retry(run_id, retry_count).await?;
                tokio::time::sleep(delay).await;
            }
            Err(e) => {
                repository.complete_run(run_id, JobStatus::Failed, Some(&e.to_string()), None).await?;
                return Err(e);
            }
        }
    }
}
```

---

## 7. API Endpoints

### List Jobs

```
GET /api/admin/jobs
Authorization: Bearer {admin_token}

Response 200:
{
  "jobs": [
    {
      "id": "weaver-cleanup",
      "name": "Weaver Cleanup",
      "description": "Remove expired weaver pods",
      "job_type": "periodic",
      "interval_secs": 1800,
      "enabled": true,
      "last_run": {
        "id": "run-123",
        "status": "succeeded",
        "started_at": "2026-01-01T10:00:00Z",
        "completed_at": "2026-01-01T10:00:05Z",
        "duration_ms": 5000
      },
      "next_run_at": "2026-01-01T10:30:00Z"
    }
  ]
}
```

### Get Job History

```
GET /api/admin/jobs/{job_id}/runs?limit=50
Authorization: Bearer {admin_token}

Response 200:
{
  "runs": [
    {
      "id": "run-123",
      "status": "succeeded",
      "started_at": "2026-01-01T10:00:00Z",
      "completed_at": "2026-01-01T10:00:05Z",
      "duration_ms": 5000,
      "retry_count": 0,
      "triggered_by": "schedule",
      "metadata": { "weavers_deleted": 3 }
    }
  ]
}
```

### Trigger Job Manually

```
POST /api/admin/jobs/{job_id}/trigger
Authorization: Bearer {admin_token}

Response 200:
{
  "run_id": "run-456",
  "message": "Job triggered"
}
```

### Cancel Running Job

```
POST /api/admin/jobs/runs/{run_id}/cancel
Authorization: Bearer {admin_token}

Response 200:
{
  "message": "Job cancelled"
}
```

---

## 8. Health Integration

### Job Health Status

```rust
pub struct JobHealthStatus {
    pub status: HealthStatus,  // Healthy, Degraded, Unhealthy
    pub jobs: Vec<JobHealthInfo>,
}

pub struct JobHealthInfo {
    pub id: String,
    pub name: String,
    pub status: JobInstanceStatus,
    pub last_run_status: Option<JobStatus>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}

pub enum JobInstanceStatus {
    Running,
    Idle,
    Disabled,
}
```

### Health Endpoint Response

```json
{
  "status": "degraded",
  "components": {
    "jobs": {
      "status": "degraded",
      "jobs": [
        {
          "id": "weaver-cleanup",
          "name": "Weaver Cleanup",
          "status": "idle",
          "last_run_status": "succeeded",
          "last_run_at": "2026-01-01T10:00:00Z",
          "consecutive_failures": 0
        },
        {
          "id": "token-refresh",
          "name": "Anthropic Token Refresh",
          "status": "idle",
          "last_run_status": "failed",
          "last_run_at": "2026-01-01T10:05:00Z",
          "consecutive_failures": 3
        }
      ]
    }
  }
}
```

### Health Status Rules

| Condition | Status |
|-----------|--------|
| All jobs succeeded in last run | Healthy |
| Any job has 1-2 consecutive failures | Degraded |
| Any job has 3+ consecutive failures | Unhealthy |
| Any job disabled | Degraded |

---

## 9. Email Alerts

### Alert Trigger

Send email when a job has 3+ consecutive failures.

### Email Template

```
Subject: [Loom] Job Failed: {job_name}

The job "{job_name}" has failed {consecutive_failures} times in a row.

Job ID: {job_id}
Last Error: {error_message}
Last Run: {last_run_at}

View job history: {base_url}/admin/jobs/{job_id}
```

### Alert Service

```rust
pub struct AlertService {
    smtp: Arc<SmtpClient>,
    config: AlertConfig,
}

pub struct AlertConfig {
    pub enabled: bool,
    pub recipients: Vec<String>,
    pub from_address: String,
    pub base_url: String,
}

impl AlertService {
    pub async fn send_job_failure_alert(
        &self,
        job: &dyn Job,
        consecutive_failures: u32,
    ) -> Result<(), AlertError>;
}
```

---

## 10. Admin UI

### Jobs Page: `/admin/jobs`

```
┌─────────────────────────────────────────────────────────────────┐
│  Background Jobs                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │ 🟢 Weaver Cleanup                              [Run Now]  │  │
│  │    Periodic (every 30 min) • Last run: 5 min ago          │  │
│  │    Status: Succeeded • Duration: 2.3s                     │  │
│  │                                            [View History] │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │ 🟢 Anthropic Token Refresh                     [Run Now]  │  │
│  │    Periodic (every 5 min) • Last run: 2 min ago           │  │
│  │    Status: Succeeded • Duration: 0.8s                     │  │
│  │                                            [View History] │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │ 🔴 Session Cleanup                             [Run Now]  │  │
│  │    Periodic (every 1 hour) • Last run: 15 min ago         │  │
│  │    Status: Failed (3 consecutive) • Error: DB timeout     │  │
│  │                                            [View History] │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Job History Modal

```
┌─────────────────────────────────────────────────────────────────┐
│  Weaver Cleanup - Run History                           [Close] │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────┬──────────┬──────────┬─────────┬─────────────────┐  │
│  │ Status  │ Started  │ Duration │ Trigger │ Details         │  │
│  ├─────────┼──────────┼──────────┼─────────┼─────────────────┤  │
│  │ 🟢 OK   │ 10:00 AM │ 2.3s     │ Schedule│ Deleted 3       │  │
│  │ 🟢 OK   │ 9:30 AM  │ 1.8s     │ Schedule│ Deleted 0       │  │
│  │ 🟢 OK   │ 9:00 AM  │ 2.1s     │ Manual  │ Deleted 5       │  │
│  │ 🔴 Fail │ 8:30 AM  │ 30.0s    │ Schedule│ Timeout         │  │
│  └─────────┴──────────┴──────────┴─────────┴─────────────────┘  │
│                                                                  │
│  Showing 50 of 1,234 runs                      [Load More]      │
└─────────────────────────────────────────────────────────────────┘
```

---

## 11. Graceful Shutdown

### Shutdown Sequence

1. Server receives SIGTERM
2. `JobScheduler::shutdown()` is called
3. Broadcast shutdown signal to all job runners
4. Wait for running jobs to complete (with timeout)
5. Mark any still-running jobs as "cancelled" in database
6. Server exits

### Integration in main.rs

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... setup ...
    
    // Create job scheduler
    let mut scheduler = JobScheduler::new(job_repo, alert_service);
    
    // Register jobs
    scheduler.register_periodic(
        Arc::new(WeaverCleanupJob::new(provisioner)),
        Duration::from_secs(config.weaver_cleanup_interval_secs),
    );
    scheduler.register_periodic(
        Arc::new(TokenRefreshJob::new(llm_service)),
        Duration::from_secs(300),
    );
    scheduler.register_periodic(
        Arc::new(SessionCleanupJob::new(db_pool.clone())),
        Duration::from_secs(3600),
    );
    
    // Start scheduler
    scheduler.start().await;
    
    // ... start server ...
    
    tokio::select! {
        result = axum::serve(listener, app) => { /* ... */ }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received shutdown signal");
            scheduler.shutdown().await;
        }
    }
}
```

---

## 12. Implementation Checklist

### New Crate: loom-jobs

- [x] Create `crates/loom-jobs/` crate
- [x] Define `Job` trait and types
- [x] Implement `JobScheduler`
- [x] Implement `JobRepository` (SQLite)
- [x] Add retry logic with exponential backoff
- [x] Add shutdown signaling

### Database Migrations

- [x] Add `job_definitions` table
- [x] Add `job_runs` table
- [x] Add indexes for job_runs queries

### Job Implementations

- [x] `WeaverCleanupJob` - migrate from current implementation
- [x] `TokenRefreshJob` - migrate from AnthropicPool
- [x] `SessionCleanupJob` - new
- [x] `OAuthStateCleanupJob` - new
- [x] `JobHistoryCleanupJob` - new (prune runs > 90 days)

### loom-server Integration

- [x] Initialize `JobScheduler` in `main.rs`
- [x] Register all jobs on startup
- [x] Integrate shutdown in signal handler
- [x] Add admin routes for job management

### Alert System

- [ ] Create `AlertService` using loom-smtp
- [ ] Implement job failure alert template
- [x] Add alert configuration to ServerConfig

### Health Integration

- [x] Add `JobHealthStatus` to health endpoint
- [x] Implement health status aggregation

### Admin UI (loom-web)

- [x] Create `/admin/jobs` page
- [x] Job list with status badges
- [x] "Run Now" button per job
- [x] Job history modal
- [x] Cancel running job button

### NixOS Module

- [x] Add job alert configuration options
- [x] Add alert recipient email configuration

---

## 13. Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LOOM_SERVER_JOB_ALERT_ENABLED` | Enable email alerts | `false` |
| `LOOM_SERVER_JOB_ALERT_RECIPIENTS` | Comma-separated emails | (none) |
| `LOOM_SERVER_JOB_HISTORY_RETENTION_DAYS` | Days to keep job runs | `90` |
| `LOOM_SERVER_SESSION_CLEANUP_INTERVAL_SECS` | Session cleanup interval | `3600` |
| `LOOM_SERVER_OAUTH_STATE_CLEANUP_INTERVAL_SECS` | OAuth state cleanup | `900` |

### NixOS Options

```nix
services.loom-server.jobs = {
  alertEnabled = mkOption {
    type = types.bool;
    default = false;
  };
  
  alertRecipients = mkOption {
    type = types.listOf types.str;
    default = [];
  };
  
  historyRetentionDays = mkOption {
    type = types.int;
    default = 90;
  };
};
```

---

## 14. Future Enhancements

- Job scheduling with cron expressions
- Job dependencies (run B after A completes)
- Job priority levels
- Prometheus metrics for job execution
- Webhook notifications on job events
- Job execution timeout configuration
- Pause/resume individual jobs
