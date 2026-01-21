<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Analytics System Implementation Plan

Implementation checklist for `specs/analytics-system.md`. Each item cites the relevant specification section and source code to modify.

---

## Phase 1: Core Types (`loom-analytics-core`) ✅ COMPLETED

**Reference:** [analytics-system.md §3](./analytics-system.md#3-core-entities)

**Completed in commit:** d1dd21f (2026-01-11)

- [x] Create `crates/loom-analytics-core/Cargo.toml`
  - Dependencies: `chrono`, `serde`, `serde_json`, `thiserror`, `uuid` (v4/v7), `loom-common-secret`
  - See [analytics-system.md §15](./analytics-system.md#15-rust-dependencies)

- [x] Create `crates/loom-analytics-core/src/lib.rs`
  - Re-export all types

- [x] Create `crates/loom-analytics-core/src/person.rs`
  - `Person` struct with `id`, `org_id`, `properties`, timestamps
  - `PersonWithIdentities` wrapper
  - See [analytics-system.md §3.1](./analytics-system.md#31-person)

- [x] Create `crates/loom-analytics-core/src/identity.rs`
  - `PersonIdentity` struct with `distinct_id`, `identity_type`
  - `IdentityType` enum: `Anonymous`, `Identified`
  - See [analytics-system.md §3.2](./analytics-system.md#32-personidentity)

- [x] Create `crates/loom-analytics-core/src/event.rs`
  - `Event` struct with `ip_address: Option<SecretString>`
  - Use `loom-common-secret` for IP addresses per [analytics-system.md §14.1](./analytics-system.md#141-ip-address-handling)
  - See [analytics-system.md §3.3](./analytics-system.md#33-event)

- [x] Create `crates/loom-analytics-core/src/identify.rs`
  - `IdentifyPayload`, `AliasPayload`, `SetPayload`, `SetOncePayload`, `UnsetPayload` structs
  - `PersonMerge` and `MergeReason` for merge audit trail
  - See [analytics-system.md §4.2](./analytics-system.md#42-identify-operation)

- [x] Create `crates/loom-analytics-core/src/api_key.rs`
  - `AnalyticsApiKey` struct
  - `AnalyticsKeyType` enum: `Write`, `ReadWrite`
  - Key format: `loom_analytics_write_` / `loom_analytics_rw_`
  - See [analytics-system.md §3.4](./analytics-system.md#34-analytics-api-key), [§10](./analytics-system.md#10-api-key-management)

- [x] Create `crates/loom-analytics-core/src/error.rs`
  - Error types using `thiserror`
  - Pattern: follow `crates/loom-flags-core/src/error.rs`

- [x] Add to workspace `Cargo.toml`

- [x] Run `cargo2nix-update` to regenerate `Cargo.nix`

**Tests:** 75 property-based and unit tests passing

---

## Phase 2: Database Schema ✅ COMPLETED

**Reference:** [analytics-system.md §9](./analytics-system.md#9-database-schema)

**Completed in commit:** (2026-01-11)

- [x] Create migration `crates/loom-server/migrations/032_analytics.sql`
  - `analytics_persons` table
  - `analytics_person_identities` table
  - `analytics_events` table
  - `analytics_person_merges` table
  - `analytics_api_keys` table
  - All indexes as specified
  - Composite index `idx_analytics_events_org_timestamp` for common query pattern
  - Pattern: follow `crates/loom-server/migrations/030_feature_flags.sql`

- [x] Run `cargo2nix-update` after adding migration (per CLAUDE.md)

---

## Phase 3: Server Repository Layer (`loom-server-analytics`) ✅ COMPLETED

**Reference:** [analytics-system.md §2](./analytics-system.md#2-architecture)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-server-analytics/Cargo.toml`
  - Dependencies: `loom-analytics-core`, `loom-common-secret`, `sqlx`, `argon2`, `async-trait`, `tracing`

- [x] Create `crates/loom-server-analytics/src/lib.rs`
  - Re-exports all modules and core types

- [x] Create `crates/loom-server-analytics/src/repository.rs`
  - `AnalyticsRepository` trait with all CRUD operations
  - `SqliteAnalyticsRepository` implementation
  - CRUD for `analytics_persons`
  - CRUD for `analytics_person_identities`
  - Insert/query for `analytics_events` with filters
  - CRUD for `analytics_person_merges`
  - CRUD for `analytics_api_keys`
  - Pattern: follows `crates/loom-server-flags/src/repository.rs`

- [x] Create `crates/loom-server-analytics/src/api_key.rs`
  - `hash_api_key()` - Argon2 hashing
  - `verify_api_key()` - Key verification
  - Pattern: follows `crates/loom-server-flags/src/sdk_auth.rs`

- [x] Create `crates/loom-server-analytics/src/error.rs`
  - `AnalyticsServerError` enum using `thiserror`

- [x] Add to workspace `Cargo.toml`

- [x] Run `cargo2nix-update` to regenerate `Cargo.nix`

**Tests:** 9 property-based and unit tests passing

---

## Phase 4: Identity Resolution ✅ COMPLETED

**Reference:** [analytics-system.md §4](./analytics-system.md#4-identity-resolution)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-server-analytics/src/identity_resolution.rs`
  - `resolve_person_for_distinct_id(org_id, distinct_id)` → creates Person if needed
  - `identify(org_id, IdentifyPayload)` → links anonymous to identified
  - `alias(org_id, AliasPayload)` → links two distinct_ids
  - Person merge logic per [analytics-system.md §4.3](./analytics-system.md#43-person-merge)
    - Winner selection rules (identified > anonymous, older > newer)
    - Event reassignment via `reassign_events()`
    - Identity transfer via `transfer_identities()`
    - Property merge (winner precedence, loser fills gaps)

- [x] Add `analytics_person_merges` audit trail insert

**Tests:** 14 unit tests passing covering all identity resolution scenarios

---

## Phase 5: API Handlers ✅ COMPLETED

**Reference:** [analytics-system.md §7](./analytics-system.md#7-api-endpoints)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-server-analytics/src/handlers/mod.rs`

- [x] Create `crates/loom-server-analytics/src/handlers/capture.rs`
  - `capture_event_impl` - single event capture
  - `batch_capture_impl` - batch events
  - Add automatic properties (`$ip`, `$user_agent`, `$lib`, etc.) per [§5.2](./analytics-system.md#52-automatic-properties)
  - Validate event per [§14.3](./analytics-system.md#143-event-validation)

- [x] Create `crates/loom-server-analytics/src/handlers/identify.rs`
  - `identify_impl` - identify user
  - `alias_impl` - create alias
  - `set_properties_impl` - set person properties

- [x] Create `crates/loom-server-analytics/src/handlers/persons.rs`
  - `list_persons_impl` (requires ReadWrite key)
  - `get_person_impl`
  - `get_person_by_distinct_id_impl`

- [x] Create `crates/loom-server-analytics/src/handlers/events.rs`
  - `list_events_impl` (requires ReadWrite key)
  - `count_events_impl`
  - `export_events_impl`

- [x] Create `crates/loom-server-analytics/src/handlers/api_keys.rs`
  - `list_api_keys_impl` (requires User Auth)
  - `create_api_key_impl`
  - `revoke_api_key_impl`

- [x] Create `crates/loom-server-analytics/src/routes.rs`
  - Exports all handler implementations for use in loom-server
  - Auth middleware applied in loom-server integration layer

- [x] Create `crates/loom-server-analytics/src/middleware.rs`
  - `AnalyticsApiKeyContext` for API key auth
  - `parse_key_type` and `extract_bearer_token` utilities

- [x] Add API types to `crates/loom-server-api/src/analytics.rs`
  - Request/response types for all endpoints
  - OpenAPI schema support via utoipa

**Tests:** 42 unit tests passing (handlers, validation, middleware)

---

## Phase 6: Integration with loom-server ✅ COMPLETED

**Reference:** [analytics-system.md §2](./analytics-system.md#2-architecture)

**Completed in commit:** (2026-01-11)

- [x] Update `crates/loom-server/Cargo.toml`
  - Added `loom-server-analytics` and `loom-analytics-core` dependencies

- [x] Create `crates/loom-server/src/routes/analytics.rs`
  - SDK routes: capture, batch, identify, alias, set_properties
  - Query routes: list_persons, get_person, get_person_by_distinct_id, list_events, count_events, export_events
  - API key management routes: list_api_keys, create_api_key, revoke_api_key
  - API key authentication via Bearer token

- [x] Update `crates/loom-server/src/routes/mod.rs`
  - Added `pub mod analytics;`
  - Added analytics type re-exports

- [x] Update `crates/loom-server/src/api.rs`
  - Added `analytics_repo` and `analytics_state` fields to AppState
  - Initialized analytics repository and state
  - Mounted SDK routes on public router at `/api/analytics/*`
  - Mounted API key management routes on authed router at `/api/orgs/{org_id}/analytics/*`

- [x] Update `crates/loom-server-api/src/analytics.rs`
  - Added `IntoParams` derive to query types for OpenAPI support

- [x] Add configuration for analytics (Phase 6.1)
  - `LOOM_ANALYTICS_ENABLED`
  - `LOOM_ANALYTICS_BATCH_SIZE`
  - `LOOM_ANALYTICS_FLUSH_INTERVAL_SECS`
  - `LOOM_ANALYTICS_EVENT_RETENTION_DAYS`
  - See [analytics-system.md §11](./analytics-system.md#11-configuration)
  - Added `crates/loom-server-config/src/sections/analytics.rs` with `AnalyticsConfig` and `AnalyticsConfigLayer`
  - Added analytics to `ServerConfig` in `crates/loom-server-config/src/lib.rs`
  - Added environment variable loading in `crates/loom-server-config/src/sources.rs`
  - Added merge support in `crates/loom-server-config/src/layer.rs`
  - 7 unit tests for analytics configuration

**Tests:** All 42 loom-server-analytics tests pass, 74 loom-analytics-core tests pass

---

## Phase 7: Experiment Integration ✅ COMPLETED

**Reference:** [analytics-system.md §6](./analytics-system.md#6-experiment-integration)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-flags/src/analytics.rs`
  - `AnalyticsHook` trait for receiving flag evaluation events
  - `FlagExposure` struct with flag_key, variant, user_id, distinct_id, evaluation_reason
  - `to_event_properties()` method returns `$feature_flag`, `$feature_flag_response`, `$feature_flag_reason`
  - `NoOpAnalyticsHook` default implementation
  - `SharedAnalyticsHook` type alias for `Arc<dyn AnalyticsHook>`

- [x] Update `crates/loom-flags/src/client.rs`
  - Added `analytics_hook` field to `FlagsClientBuilder`
  - Added `analytics_hook()` builder method to set custom hook
  - Added `analytics_hook` field to `FlagsClient`
  - Added `track_flag_exposure()` method called after every flag evaluation
  - Hook receives `FlagExposure` with all event data needed for `$feature_flag_called`
  - See [analytics-system.md §6.1](./analytics-system.md#61-feature-flag-exposure-tracking)

- [x] Update `crates/loom-flags/src/lib.rs`
  - Export `AnalyticsHook`, `FlagExposure`, `NoOpAnalyticsHook`, `SharedAnalyticsHook`

- [x] Document query pattern for experiment analysis
  - SQL example in module-level documentation
  - Join `exposure_logs` with `analytics_events` via `distinct_id`
  - See [analytics-system.md §6.3](./analytics-system.md#63-experiment-metrics)

**Tests:** 37 tests passing (9 new analytics-related tests, 28 existing)

---

## Phase 8: Rust SDK (`loom-analytics`) ✅ COMPLETED

**Reference:** [analytics-system.md §8.1](./analytics-system.md#81-rust-sdk-loom-analytics)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-analytics/Cargo.toml`
  - Dependencies: `loom-analytics-core`, `loom-common-http`, `tokio`, `reqwest`, `tracing`

- [x] Create `crates/loom-analytics/src/lib.rs`
  - Re-export `AnalyticsClient`, `Properties`, error types

- [x] Create `crates/loom-analytics/src/client.rs`
  - `AnalyticsClient` with builder pattern
  - `capture(event, distinct_id, properties)`
  - `identify(distinct_id, user_id, properties)`
  - `alias(distinct_id, alias)`
  - `set(distinct_id, properties)`
  - `flush()` - force immediate flush
  - `shutdown()` - flush pending events and stop background task

- [x] Create `crates/loom-analytics/src/batch.rs`
  - `BatchProcessor` with background flush loop
  - `BatchConfig` with configurable interval (default 10s), batch size (default 10), queue size (default 1000)
  - Queue overflow handling (drops oldest events)
  - `BatchSender` trait for testability
  - Uses `loom-common-http` retry for HTTP requests
  - See [analytics-system.md §8.3](./analytics-system.md#83-sdk-behavior)

- [x] Create `crates/loom-analytics/src/properties.rs`
  - `Properties` builder for event and person properties
  - Supports strings, numbers, booleans, JSON values
  - `insert()`, `merge()`, `into_value()` methods

- [x] Create `crates/loom-analytics/src/error.rs`
  - `AnalyticsError` enum with retryable errors
  - Implements `RetryableError` trait for retry logic

- [x] Add to workspace `Cargo.toml`

- [x] Run `cargo2nix-update` to regenerate `Cargo.nix`

**Tests:** 43 property-based and unit tests passing

---

## Phase 9: TypeScript SDK (`@loom/analytics`) ✅ COMPLETED

**Reference:** [analytics-system.md §8.2](./analytics-system.md#82-typescript-sdk-loomanalytics)

**Completed in commit:** (2026-01-11)

- [x] Create `web/packages/analytics/package.json`
  - Dependencies: `@loom/http`

- [x] Create `web/packages/analytics/src/index.ts`
  - Export `AnalyticsClient`, types, errors, storage utilities

- [x] Create `web/packages/analytics/src/client.ts`
  - `AnalyticsClient` class with builder-style options
  - `capture(event, properties)` - Enqueue event for batch processing
  - `identify(userId, properties)` - Link anonymous to identified user
  - `alias(alias)` - Create alias for current distinct_id
  - `set(properties)` - Set person properties
  - `reset()` - Generate new anonymous distinct_id
  - `getDistinctId()` - Get current distinct_id
  - `flush()` - Manual flush of queued events
  - `shutdown()` - Graceful shutdown with final flush

- [x] Create `web/packages/analytics/src/storage.ts`
  - `generateDistinctId()` - UUIDv7 generation (time-ordered)
  - `DistinctIdManager` - Manages distinct_id lifecycle
  - `MemoryStorage`, `CookieStorage`, `LocalStorageStorage`, `CombinedStorage`
  - Cookie name: `loom_analytics_distinct_id`
  - Cross-subdomain support via configurable cookie domain

- [x] Create `web/packages/analytics/src/batch.ts`
  - `BatchProcessor` with background flush loop
  - Flush on interval (10s default) or batch size (10 default)
  - Queue overflow handling (drops oldest, max 1000 default)
  - Retry with exponential backoff via `@loom/http`
  - Event listener hooks for flush, drop, and error events

- [x] Create `web/packages/analytics/src/types.ts`
  - `AnalyticsClientOptions` - Configuration interface
  - `CapturePayload`, `IdentifyPayload`, `AliasPayload`, `SetPayload`
  - `BatchConfig`, `AutocaptureConfig`
  - `PersistenceMode` - localStorage+cookie, localStorage, cookie, memory

- [x] Create `web/packages/analytics/src/errors.ts`
  - `AnalyticsError` base class with `isRetryable()` method
  - `InvalidApiKeyError`, `InvalidBaseUrlError`, `ClientClosedError`
  - `CaptureError`, `IdentifyError`, `StorageError`, `ValidationError`
  - `NetworkError`, `ServerError`, `RateLimitedError`

- [x] Add autocapture option
  - `$pageview` on page load and SPA navigation (popstate)
  - `$pageleave` on beforeunload and pagehide
  - Configurable via `autocapture: true | false | { pageview: bool, pageleave: bool }`

- [x] Update `web/packages/http/` if needed
  - Existing `@loom/http` package provides HTTP client with retry
  - No changes needed

**Tests:** 79 property-based and unit tests passing (storage, batch, client, types, errors)

---

## Phase 10: Audit Integration ✅ COMPLETED

**Reference:** [analytics-system.md §12](./analytics-system.md#12-audit-events)

**Completed in commit:** (2026-01-11)

- [x] Add audit event types to `crates/loom-server-audit/src/event.rs`
  - `AnalyticsApiKeyCreated` - severity: Info
  - `AnalyticsApiKeyRevoked` - severity: Notice
  - `AnalyticsPersonMerged` - severity: Notice
  - `AnalyticsEventsExported` - severity: Info
  - Display implementations (snake_case format)
  - Default severity mappings
  - Tests: 3 new tests (severities, display, serialize/deserialize)

- [x] Call audit logging from handlers
  - `create_api_key` → logs `AnalyticsApiKeyCreated`
  - `revoke_api_key` → logs `AnalyticsApiKeyRevoked`
  - `export_events` → logs `AnalyticsEventsExported`

- [x] Add audit logging for person merges
  - Added `MergeAuditHook` trait to `loom-server-analytics/src/identity_resolution.rs`
  - Added `PersonMergeDetails` struct with merge context (org_id, winner_id, loser_id, reason, events_reassigned, identities_transferred)
  - `IdentityResolutionService::with_audit_hook()` constructor to inject hook
  - `AnalyticsMergeAuditHook` implementation in `loom-server/src/routes/analytics.rs` logs `AnalyticsPersonMerged`
  - Integrated with `AnalyticsState::with_audit_hook()` in `api.rs`
  - Tests: 6 new audit hook tests verifying merge callbacks

**Tests:** All 66 loom-server-audit tests pass, 48 loom-server-analytics tests pass

---

## Phase 11: Authorization Tests ✅ COMPLETED

**Reference:** [CLAUDE.md routes section](../CLAUDE.md)

**Completed in commit:** (2026-01-11)

- [x] Create `crates/loom-server/tests/authz/analytics.rs`
  - 29 comprehensive authorization tests
  - Test Write key can only capture, not query (403 Forbidden for query endpoints)
  - Test ReadWrite key can capture and query
  - Test User auth required for API key management
  - Test org membership validation
  - Test cross-org access prevention
  - Test revoked API key rejection
  - Pattern: follows `crates/loom-server/tests/authz/*.rs`

- [x] Fixed API key authentication bug
  - Changed from `get_api_key_by_hash` (broken with Argon2 random salts) to `find_api_key_by_raw` (proper verification)
  - Added `find_api_key_by_raw` method to `AnalyticsRepository` trait and SQLite implementation

- [x] Added missing migrations 031 and 032 to `run_migrations` in `db/mod.rs`

---

## Phase 12: Documentation ✅ COMPLETED

**Completed in commit:** (2026-01-14)

- [x] Add inline rustdoc to all public types
  - `loom-analytics-core`: All types documented (person, identity, event, api_key, identify, error)
  - `loom-analytics`: Already well-documented (client, batch, properties, error)
  - `loom-server-analytics`: All types documented (repository, middleware, identity_resolution, api_key, error)

- [x] Documentation tests pass: `cargo test -p loom-analytics-core -p loom-analytics -p loom-server-analytics`

- [ ] Update main README if analytics is a significant feature (defer - not user-facing yet)

---

## Files to Create

```
crates/
├── loom-analytics-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── person.rs
│       ├── identity.rs
│       ├── event.rs
│       ├── identify.rs
│       ├── api_key.rs
│       └── error.rs
├── loom-analytics/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── client.rs
│       ├── batch.rs
│       └── error.rs
├── loom-server-analytics/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── routes.rs
│       ├── repository.rs
│       ├── api_key.rs
│       ├── identity_resolution.rs
│       └── handlers/
│           ├── mod.rs
│           ├── capture.rs
│           ├── identify.rs
│           ├── persons.rs
│           ├── events.rs
│           └── api_keys.rs
├── loom-server/
│   └── migrations/
│       └── 032_analytics.sql

web/
└── packages/
    └── analytics/
        ├── package.json
        └── src/
            ├── index.ts
            ├── client.ts
            ├── storage.ts
            └── batch.ts
```

---

## Files to Modify

| File | Change |
|------|--------|
| `Cargo.toml` (workspace) | Add `loom-analytics-core`, `loom-analytics`, `loom-server-analytics` to members |
| `crates/loom-server/Cargo.toml` | Add `loom-server-analytics` dependency |
| `crates/loom-server/src/routes/mod.rs` | Mount `/api/analytics/*` routes |
| `crates/loom-server/src/config.rs` | Add `LOOM_ANALYTICS_*` env vars |
| `crates/loom-server-audit/src/events.rs` | Add analytics audit event types |
| `crates/loom-flags/src/client.rs` | Optional: auto-capture `$feature_flag_called` |
| `Cargo.nix` | Regenerate via `cargo2nix-update` |

---

## Verification Checklist

After implementation:

- [x] `cargo build --workspace` succeeds (verified 2026-01-15)
- [x] `cargo test --workspace` passes (verified 2026-01-15)
- [x] `cargo clippy --workspace -- -D warnings` clean (verified 2026-01-15)
- [x] `cargo fmt --all` applied (verified 2026-01-15)
- [ ] `cargo2nix-update` run if Cargo.lock changed
- [ ] Migration runs on fresh database
- [ ] Capture endpoint accepts events
- [ ] Identify links anonymous to authenticated
- [ ] API keys authenticate correctly
- [ ] Rust SDK can capture and identify
- [ ] TypeScript SDK can capture and identify
- [ ] Distinct_id persists across page reloads (browser)
- [ ] Events visible in query endpoint (with ReadWrite key)
