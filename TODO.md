# Authentication & ABAC Implementation Plan

Implementation checklist for the Authentication and ABAC system. See
[specs/auth-abac-system.md](./specs/auth-abac-system.md) for full specification.

---

## ✅ Phase 0: Foundation (COMPLETED)

### 0.1 Create loom-auth Crate

- [x] Create `crates/loom-auth/Cargo.toml` with dependencies
- [x] Create `crates/loom-auth/src/lib.rs` with module structure
- [x] Add to workspace members in root `Cargo.toml`

### 0.2 Core Types

- [x] Create `src/types.rs` with all ID newtypes, roles, and enums
- [x] Create `src/error.rs` with `AuthError` enum

### 0.3 Database Migrations

- [x] Create `migrations/008_auth_users.sql` (users, identities)
- [x] Create `migrations/009_auth_sessions.sql` (sessions, access_tokens, device_codes, magic_links)
- [x] Create `migrations/010_auth_orgs.sql` (organizations, memberships, invitations)
- [x] Create `migrations/011_auth_teams.sql` (teams, team_memberships)
- [x] Create `migrations/012_auth_api_keys.sql` (api_keys, api_key_usage)
- [x] Create `migrations/013_auth_threads_ext.sql` (thread extensions, share_links, support_access)
- [x] Create `migrations/014_auth_audit.sql` (audit_logs)
- [x] Update `db.rs` to run new migrations

---

## ✅ Phase 1: Basic Web Auth (COMPLETED)

- [x] Create `src/session.rs` - Session management with 60-day sliding expiry
- [x] Create `src/user.rs` - User struct, Identity, Provider enum
- [x] Create `src/middleware.rs` - CurrentUser, AuthContext, token extraction
- [x] Create auth routes in loom-server:
  - [x] `GET /auth/providers`
  - [x] `GET /auth/me`
  - [x] `POST /auth/logout`

---

## ✅ Phase 2: Magic Link (COMPLETED)

- [x] Create `src/magic_link.rs` - 10-minute single-use tokens
- [x] Create `src/email.rs` - SMTP config, email templates
- [x] Create routes:
  - [x] `POST /auth/magic-link`
  - [x] `GET /auth/magic-link/verify`

---

## ✅ Phase 3: CLI Auth (COMPLETED)

- [x] Create `src/device_code.rs` - Device code flow (123-456-789 format)
- [x] Create `src/access_token.rs` - Bearer tokens with 60-day sliding expiry
- [x] Create routes:
  - [x] `POST /auth/device/start`
  - [x] `POST /auth/device/poll`

---

## ✅ Phase 4: Organizations (COMPLETED)

- [x] Create `src/org.rs` - Organization, OrgMembership, OrgInvitation, OrgJoinRequest
- [x] Create routes in `routes/orgs.rs`:
  - [x] `GET /api/orgs`
  - [x] `POST /api/orgs`
  - [x] `GET /api/orgs/{id}`
  - [x] `PATCH /api/orgs/{id}`
  - [x] `DELETE /api/orgs/{id}`
  - [x] `GET /api/orgs/{id}/members`
  - [x] `POST /api/orgs/{id}/members`
  - [x] `DELETE /api/orgs/{id}/members/{user_id}`

---

## ✅ Phase 5: Teams (COMPLETED)

- [x] Create `src/team.rs` - Team, TeamMembership
- [x] Create routes in `routes/teams.rs`:
  - [x] `GET /api/orgs/{org_id}/teams`
  - [x] `POST /api/orgs/{org_id}/teams`
  - [x] `GET /api/orgs/{org_id}/teams/{team_id}`
  - [x] `PATCH /api/orgs/{org_id}/teams/{team_id}`
  - [x] `DELETE /api/orgs/{org_id}/teams/{team_id}`
  - [x] `GET /api/orgs/{org_id}/teams/{team_id}/members`
  - [x] `POST /api/orgs/{org_id}/teams/{team_id}/members`
  - [x] `DELETE /api/orgs/{org_id}/teams/{team_id}/members/{user_id}`

---

## ✅ Phase 6: ABAC Engine (COMPLETED)

- [x] Create `src/abac/types.rs` - SubjectAttrs, ResourceAttrs, Action
- [x] Create `src/abac/engine.rs` - `is_allowed()` policy dispatcher
- [x] Create `src/abac/policies/thread.rs` - Thread visibility policies
- [x] Create `src/abac/policies/org.rs` - Org/team management policies
- [x] Create `src/abac/policies/llm.rs` - LLM/tool access policies

---

## ✅ Phase 7: API Keys (COMPLETED)

- [x] Create `src/api_key.rs` - lk_ prefixed keys, Argon2 hashing
- [x] Create routes in `routes/api_keys.rs`:
  - [x] `GET /api/orgs/{org_id}/api-keys`
  - [x] `POST /api/orgs/{org_id}/api-keys`
  - [x] `DELETE /api/orgs/{org_id}/api-keys/{id}`
  - [x] `GET /api/orgs/{org_id}/api-keys/{id}/usage`

---

## ✅ Phase 8: Audit & Security (COMPLETED)

- [x] Create `src/audit.rs` - AuditEventType, AuditLogEntry, 90-day retention
- [x] CSRF protection ready (SameSite cookies + tokens)

---

## ✅ Phase 9: Admin Features (COMPLETED)

- [x] Create `src/admin.rs` - ImpersonationSession, promotion/demotion checks
- [x] Create routes in `routes/admin.rs`:
  - [x] `GET /api/admin/users`
  - [x] `PATCH /api/admin/users/{id}/roles`
  - [x] `POST /api/admin/users/{id}/impersonate`
  - [x] `POST /api/admin/impersonate/stop`
  - [x] `GET /api/admin/audit-logs`

---

## ✅ Phase 10: Sharing & Support (COMPLETED)

- [x] Create `src/share_link.rs` - 48-hex token, expiry, revocation
- [x] Create `src/support_access.rs` - 31-day auto-expiry
- [x] Create routes in `routes/share.rs`:
  - [x] `POST /api/threads/{id}/share`
  - [x] `DELETE /api/threads/{id}/share`
  - [x] `GET /api/threads/{id}/share/{token}` (public)
  - [x] `POST /api/threads/{id}/support-access/request`
  - [x] `POST /api/threads/{id}/support-access/approve`
  - [x] `DELETE /api/threads/{id}/support-access`

---

## ✅ Phase 11: User Profile & Account (COMPLETED)

- [x] Create `src/account_deletion.rs` - 90-day grace, tombstone users
- [x] Create routes in `routes/users.rs`:
  - [x] `GET /api/users/{id}`
  - [x] `PATCH /api/users/me`
  - [x] `POST /api/users/me/delete`
  - [x] `POST /api/users/me/restore`

---

## ✅ Phase 12: WebSocket Auth (COMPLETED)

- [x] Update WebSocket handler to validate session cookie
- [x] Implement first-message auth for CLI (30s timeout)
- [x] Add bearer token support for WebSocket connections
- [x] Add WebSocket upgrade route at `/v1/ws/sessions/{session_id}`
- [x] Implement keepalive ping/pong with 30s interval
- [x] Add comprehensive tests (31 tests in loom-server, 15 in loom-auth)

---

## ✅ Phase 13: Session Routes (COMPLETED)

- [x] Create routes in `routes/sessions.rs`:
  - [x] `GET /api/sessions`
  - [x] `DELETE /api/sessions/{id}`

---

## ✅ Phase 14: Documentation & OpenAPI (COMPLETED)

- [x] Added utoipa annotations to all route handlers
- [x] Added schemas to api_docs.rs
- [x] Added tags: auth, sessions, organizations, teams, users, api-keys, admin, share

---

## ✅ Phase 15: Testing (COMPLETED)

- [x] 365+ unit tests in loom-auth covering:
  - Session management
  - Token generation and verification
  - ABAC policy enforcement
  - Magic link flow
  - Device code flow
  - API key management
  - Audit logging
  - Share links and support access

---

## Summary

| Component | Status | Tests |
|-----------|--------|-------|
| loom-auth crate | ✅ Complete | 380 |
| Database migrations | ✅ Complete | - |
| HTTP routes | ✅ Complete | - |
| ABAC engine | ✅ Complete | 75 |
| WebSocket auth | ✅ Complete | 46 |

### Files Created

**loom-auth crate (19 modules):**
```
crates/loom-auth/src/
├── abac/
│   ├── engine.rs
│   ├── mod.rs
│   ├── policies/
│   │   ├── llm.rs
│   │   ├── mod.rs
│   │   ├── org.rs
│   │   └── thread.rs
│   └── types.rs
├── access_token.rs
├── account_deletion.rs
├── admin.rs
├── api_key.rs
├── audit.rs
├── device_code.rs
├── email.rs
├── error.rs
├── lib.rs
├── magic_link.rs
├── middleware.rs
├── org.rs
├── session.rs
├── share_link.rs
├── support_access.rs
├── team.rs
├── types.rs
└── user.rs
```

**loom-server routes (9 new modules):**
```
crates/loom-server/src/routes/
├── admin.rs
├── api_keys.rs
├── auth.rs (updated)
├── orgs.rs
├── sessions.rs
├── share.rs
├── teams.rs
└── users.rs
```

**Database migrations (7 new):**
```
crates/loom-server/migrations/
├── 008_auth_users.sql
├── 009_auth_sessions.sql
├── 010_auth_orgs.sql
├── 011_auth_teams.sql
├── 012_auth_api_keys.sql
├── 013_auth_threads_ext.sql
└── 014_auth_audit.sql
```

---

## Next Steps

1. ~~**WebSocket Auth**~~ - ✅ Implemented cookie-based and first-message auth for WebSocket connections
2. ~~**OAuth Integration**~~ - ✅ GitHub, Google, and Okta OAuth clients implemented
3. ~~**Database Repositories**~~ - ✅ All route handlers connected to database operations
4. ~~**GeoIP Integration**~~ - ✅ MaxMind database for session location tracking and feature flag evaluation
5. **Rate Limiting** - Add per-IP/per-user rate limits (deferred from v1)

---
---

# Feature Flags & Experiments Implementation Plan

Implementation checklist for the Feature Flags system. See
[specs/feature-flags-system.md](./specs/feature-flags-system.md) for full specification.

---

## ✅ Phase 1: Core Types & Database (COMPLETED)

**Goal:** Establish foundational types and database schema.

**Spec References:**
- Core entities: `specs/feature-flags-system.md:95-232` (Flag, Variant, Strategy, KillSwitch)
- Evaluation types: `specs/feature-flags-system.md:204-232` (EvaluationContext, EvaluationResult)
- Database schema: `specs/feature-flags-system.md:477-573`

**Tasks:**
- [x] Create `crates/loom-flags-core/` crate
  - [x] `flag.rs` - Flag, Variant, VariantValue, FlagPrerequisite types
  - [x] `strategy.rs` - Strategy, Condition, AttributeOperator, Schedule types
  - [x] `kill_switch.rs` - KillSwitch type
  - [x] `environment.rs` - Environment type
  - [x] `sdk_key.rs` - SdkKey, SdkKeyType types
  - [x] `evaluation.rs` - EvaluationContext, EvaluationResult, EvaluationReason
  - [x] `error.rs` - Error types using thiserror
- [x] Create `crates/loom-server-flags/` crate structure
  - [x] `repository.rs` - FlagsRepository trait and SqliteFlagsRepository implementation
  - [x] `evaluation.rs` - Server-side flag evaluation engine
  - [x] `sdk_auth.rs` - SDK key hashing and verification
  - [x] `error.rs` - FlagsServerError types
- [x] Add database migration `030_feature_flags.sql`
  - [x] `flag_environments` table
  - [x] `flags` table with org_id nullable for platform flags
  - [x] `flag_prerequisites` table
  - [x] `flag_configs` table (per-environment)
  - [x] `flag_strategies` table
  - [x] `kill_switches` table
  - [x] `sdk_keys` table
  - [x] `exposure_logs` table
  - [x] `flag_stats` table
- [x] Create repository layer in `loom-server-flags/src/repository.rs`
- [x] Add i18n translations for feature flags (server and web)
- [x] 50 tests (40 in loom-flags-core, 9 in loom-server-flags, 1 doc test)

---

## ✅ Phase 2: Environment & SDK Keys (COMPLETED)

**Goal:** Environment management and SDK key authentication.

**Spec References:**
- Environments: `specs/feature-flags-system.md:176-186` (Environment type)
- Auto-created environments: `specs/feature-flags-system.md:261-269`
- SDK keys: `specs/feature-flags-system.md:188-202` (SdkKey, SdkKeyType)
- SDK key format: `specs/feature-flags-system.md:274-289`
- SDK key endpoints: `specs/feature-flags-system.md:410-413`
- Environment endpoints: `specs/feature-flags-system.md:404-408`

**Tasks:**
- [x] Implement Environment CRUD handlers in `routes/flags.rs`
  - [x] `GET /api/orgs/{org_id}/flags/environments`
  - [x] `POST /api/orgs/{org_id}/flags/environments`
  - [x] `GET /api/orgs/{org_id}/flags/environments/{env_id}`
  - [x] `PATCH /api/orgs/{org_id}/flags/environments/{env_id}`
  - [x] `DELETE /api/orgs/{org_id}/flags/environments/{env_id}`
- [x] Auto-create `dev` and `prod` environments on org creation
  - [x] Hook into org creation flow in `routes/orgs.rs`
- [x] Implement SDK key generation
  - [x] Key format: `loom_sdk_{type}_{env}_{random32hex}`
  - [x] Argon2 hashing for storage
  - [x] Fixed SDK key parsing to handle environment names with underscores
- [x] Implement SDK key CRUD handlers
  - [x] `GET /api/orgs/{org_id}/flags/environments/{env_id}/sdk-keys`
  - [x] `POST /api/orgs/{org_id}/flags/environments/{env_id}/sdk-keys`
  - [x] `DELETE /api/orgs/{org_id}/flags/sdk-keys/{key_id}`
- [x] Add flags API types to `loom-server-api/src/flags.rs`
- [x] Add flags_repo to AppState
- [x] 60+ tests (51 in loom-flags-core, 9 in loom-server-flags)
  - Property-based tests for environment name validation
  - Property-based tests for SDK key generation/parsing roundtrip

---

## ✅ Phase 3: Flag Management (COMPLETED)

**Goal:** Complete flag CRUD with per-environment configuration.

**Spec References:**
- Flag type: `specs/feature-flags-system.md:97-131`
- FlagConfig type: `specs/feature-flags-system.md:133-143`
- Flag key format: `specs/feature-flags-system.md:249-258`
- Flag endpoints: `specs/feature-flags-system.md:370-378`

**Tasks:**
- [x] Flag key validation
  - [x] Pattern: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`
  - [x] Length: 3-100 characters
- [x] Implement Flag CRUD handlers
  - [x] `GET /api/orgs/{org_id}/flags` - list flags for org
  - [x] `POST /api/orgs/{org_id}/flags` - create flag
  - [x] `GET /api/orgs/{org_id}/flags/{flag_id}` - get flag by ID
  - [x] `PATCH /api/orgs/{org_id}/flags/{flag_id}` - update flag
  - [x] `POST /api/orgs/{org_id}/flags/{flag_id}/archive` - archive flag
  - [x] `POST /api/orgs/{org_id}/flags/{flag_id}/restore` - restore archived flag
- [x] Implement FlagConfig handlers
  - [x] `GET /api/orgs/{org_id}/flags/{flag_id}/configs` - get all environment configs
  - [x] `GET /api/orgs/{org_id}/flags/{flag_id}/configs/{env_id}` - get specific config
  - [x] `PATCH /api/orgs/{org_id}/flags/{flag_id}/configs/{env_id}` - update environment config
- [x] Auto-create configs for all environments on flag creation
- [x] Prerequisites handling
  - [x] Store prerequisite relationships
  - [x] Support in create/update flag
- [x] Property-based tests for flag key validation
- [x] 60 tests (all passing in loom-flags-core)

---

## ✅ Phase 4: Strategy System (COMPLETED)

**Goal:** Rollout strategies with targeting conditions.

**Spec References:**
- Strategy type: `specs/feature-flags-system.md:145-175` (Strategy, Condition, Schedule)
- Evaluation engine: `specs/feature-flags-system.md:301-349`
- Percentage hashing: `specs/feature-flags-system.md:322-328`
- Schedule evaluation: `specs/feature-flags-system.md:330-338`
- GeoIP resolution: `specs/feature-flags-system.md:340-349`
- Strategy endpoints: `specs/feature-flags-system.md:380-386`

**Tasks:**
- [x] Implement Strategy CRUD handlers
  - [x] `GET /api/orgs/{org_id}/flags/strategies`
  - [x] `POST /api/orgs/{org_id}/flags/strategies`
  - [x] `GET /api/orgs/{org_id}/flags/strategies/{strategy_id}`
  - [x] `PATCH /api/orgs/{org_id}/flags/strategies/{strategy_id}`
  - [x] `DELETE /api/orgs/{org_id}/flags/strategies/{strategy_id}`
- [x] Condition evaluation engine
  - [x] Attribute conditions (equals, contains, in, etc.)
  - [x] Geographic conditions (country, region, city)
  - [x] Environment conditions
- [x] Percentage hashing with murmur3
  - [x] Consistent hashing for sticky assignment
  - [x] Configurable key (user_id, org_id, session_id)
- [x] Schedule evaluation
  - [x] Time-based percentage ramps
- [x] GeoIP integration (completed)
  - [x] Integrate with existing `loom-server-geoip`
  - [x] Proxy header support (CF-Connecting-IP, X-Forwarded-For, X-Real-IP)
  - [x] Region/subdivision support from MaxMind database
  - [x] Server-resolved GeoIP takes precedence over client-provided geo context
  - [x] Property-based tests for GeoIP context handling
- [x] Strategy API types in `loom-server-api`
- [x] i18n translations (EN, ES, AR)
- [x] 90+ tests including property-based tests for:
  - Attribute operator evaluation
  - Percentage hashing determinism and monotonicity
  - Schedule evaluation
  - Geographic operator case-insensitivity

---

## ✅ Phase 5: Kill Switches (COMPLETED)

**Goal:** Emergency shutoff mechanism with flag linking.

**Spec References:**
- KillSwitch type: `specs/feature-flags-system.md:178-193`
- Kill switch design: `specs/feature-flags-system.md:291-299`
- Activation/deactivation flow: `specs/feature-flags-system.md:301-318`
- Kill switch endpoints: `specs/feature-flags-system.md:388-395`

**Tasks:**
- [x] Implement Kill switch CRUD handlers
  - [x] `GET /api/orgs/{org_id}/flags/kill-switches`
  - [x] `POST /api/orgs/{org_id}/flags/kill-switches`
  - [x] `GET /api/orgs/{org_id}/flags/kill-switches/{kill_switch_id}`
  - [x] `PATCH /api/orgs/{org_id}/flags/kill-switches/{kill_switch_id}`
  - [x] `DELETE /api/orgs/{org_id}/flags/kill-switches/{kill_switch_id}`
- [x] Activation endpoint
  - [x] `POST /api/orgs/{org_id}/flags/kill-switches/{kill_switch_id}/activate`
  - [x] Required: `reason` field (validation enforced)
  - [x] Set `activated_at`, `activated_by`, `activation_reason`
- [x] Deactivation endpoint
  - [x] `POST /api/orgs/{org_id}/flags/kill-switches/{kill_switch_id}/deactivate`
  - [x] Clear activation fields
- [x] Kill switch permissions
  - [x] Uses org membership (same as other flags operations)
  - [x] Any org member can manage kill switches
- [x] i18n translations (server: loom-common-i18n, web: loom-web)
- [x] API types in `loom-server-api/src/flags.rs`
- [x] Property-based tests (6 new tests for kill switch behavior)
- [x] 77 tests passing in loom-flags-core

---

## ✅ Phase 6: Evaluation Engine (COMPLETED)

**Goal:** Complete flag evaluation with all precedence rules.

**Spec References:**
- Evaluation order: `specs/feature-flags-system.md:303-320`
- Precedence rules: `specs/feature-flags-system.md:241-246`
- Evaluation endpoints: `specs/feature-flags-system.md:415-418`

**Tasks:**
- [x] Implement full evaluation flow in `loom-server-flags/src/evaluation.rs`
  1. Check flag exists
  2. Check environment config (enabled/disabled)
  3. Check kill switches (platform first, then org)
  4. Check prerequisites
  5. Evaluate strategy (conditions, percentage, schedule)
  6. Return variant with reason
- [x] Platform vs org precedence
  - [x] Platform flags override org flags with same key
  - [x] Platform kill switches affect all orgs
- [x] Implement evaluation endpoints
  - [x] `POST /api/orgs/{org_id}/flags/evaluate` - evaluate all flags for context
  - [x] `POST /api/orgs/{org_id}/flags/{flag_key}/evaluate` - evaluate single flag
- [x] Return EvaluationResult with reason
- [x] API types for evaluation (EvaluationContextApi, EvaluationResultApi, EvaluationReasonApi)
- [x] 96 tests passing (77 in loom-flags-core, 19 in loom-server-flags)

---

## ✅ Phase 7: SSE Streaming (COMPLETED)

**Goal:** Real-time flag updates via Server-Sent Events.

**Spec References:**
- SSE events: `specs/feature-flags-system.md:420-450`
- Event format: `specs/feature-flags-system.md:436-445`
- Reconnection: `specs/feature-flags-system.md:447-450`

**Tasks:**
- [x] Implement SSE endpoint
  - [x] `GET /api/flags/stream`
  - [x] SDK key authentication with Argon2 verification
- [x] Event types in `loom-flags-core/src/sse.rs`
  - [x] `init` - full state on connect
  - [x] `flag.updated` - flag or config changed
  - [x] `flag.archived` - flag archived
  - [x] `flag.restored` - flag restored from archive
  - [x] `killswitch.activated` - kill switch activated
  - [x] `killswitch.deactivated` - kill switch deactivated
  - [x] `heartbeat` - every 30s (via axum SSE KeepAlive)
- [x] Broadcast mechanism in `loom-server-flags/src/sse.rs`
  - [x] Per-environment channels (org_id, environment_id)
  - [x] Notify on flag/kill switch changes
  - [x] Broadcast to entire org for org-wide changes
- [x] Client connection management
  - [x] FlagsBroadcaster with channel statistics
  - [x] Clean up empty channels
  - [x] Connection tracking metrics
- [x] Event emission on changes
  - [x] update_flag_config broadcasts flag.updated
  - [x] archive_flag broadcasts flag.archived
  - [x] restore_flag broadcasts flag.restored
  - [x] activate_kill_switch broadcasts killswitch.activated
  - [x] deactivate_kill_switch broadcasts killswitch.deactivated
- [x] Stats endpoint `GET /api/flags/stream/stats` (admin only)
- [x] i18n translations (EN, ES, AR)
- [x] 120 tests (91 in loom-flags-core, 29 in loom-server-flags)

---

## ✅ Phase 8: Exposure Tracking (COMPLETED)

**Goal:** Track flag evaluations for experiment analysis.

**Spec References:**
- Exposure logging: `specs/feature-flags-system.md:351-378`
- Exposure endpoints: `specs/feature-flags-system.md:420-423`

**Tasks:**
- [x] Implement ExposureLog creation
  - [x] ExposureLog type with flag_id, environment_id, user_id, org_id, variant, reason
  - [x] Repository methods: create_exposure_log, list_exposure_logs, count_exposure_logs
- [x] Deduplication logic
  - [x] Context hash computation (SHA-256 of user_id + org_id + session_id + environment + attributes + geo)
  - [x] exposure_exists_within_window method to check for duplicates within 1-hour window
- [x] Per-flag exposure toggle
  - [x] Add `exposure_tracking_enabled` to Flag type
  - [x] Database migration `031_exposure_tracking.sql`
  - [x] Updated flag CRUD to include exposure_tracking_enabled
- [x] i18n translations (EN, ES, AR) for server API messages
- [x] i18n translations for loom-web (exposure tracking UI strings)
- [x] Property-based tests for context hashing (determinism, uniqueness, format)
- [x] Unit tests for ExposureLog creation
- [x] 134+ tests passing (105 in loom-flags-core, 29 in loom-server-flags)

---

## ✅ Phase 9: Stale Detection & Stats (COMPLETED)

**Goal:** Track flag usage and identify stale flags.

**Spec References:**
- Staleness criteria: `specs/feature-flags-system.md:380-385`
- Flag stats: `specs/feature-flags-system.md:387-394`
- Stats endpoints: `specs/feature-flags-system.md:420-423`

**Tasks:**
- [x] Implement FlagStats tracking
  - [x] Repository trait methods: `get_flag_stats`, `record_flag_evaluation`, `list_stale_flags`
  - [x] SQLite repository implementation with upsert for stats
  - [x] Update `last_evaluated_at` on evaluation
  - [x] Increment 24h/7d/30d evaluation counts
- [x] Stale flag detection
  - [x] `GET /api/orgs/{org_id}/flags/stale` - list stale flags
  - [x] Configurable threshold via `LOOM_FLAGS_STALE_THRESHOLD_DAYS` (default: 30 days)
  - [x] Returns flags not evaluated within threshold, ordered by staleness
- [x] Flag stats endpoint
  - [x] `GET /api/orgs/{org_id}/flags/{flag_key}/stats` - get individual flag statistics
  - [x] Returns last_evaluated_at and evaluation counts
- [x] Evaluation recording integration
  - [x] Stats recorded asynchronously (fire and forget) in evaluation endpoints
  - [x] Both single flag and bulk evaluation endpoints record stats
- [x] API types in `loom-server-api/src/flags.rs`
  - [x] `FlagStatsResponse` - single flag statistics
  - [x] `StaleFlagResponse` - stale flag entry with days_since_evaluated
  - [x] `ListStaleFlagsResponse` - list of stale flags with threshold
- [x] i18n translations (EN, ES, AR)
  - [x] Server translations in loom-common-i18n
  - [x] Web translations in loom-web
- [x] Property-based tests for FlagStats
  - [x] Count invariants (24h <= 7d <= 30d)
  - [x] Context hash determinism and uniqueness
- [x] 140+ tests passing (112 in loom-flags-core, 29 in loom-server-flags)

---

## ✅ Phase 10: Rust SDK (COMPLETED)

**Goal:** `loom-flags` crate for Rust clients.

**Spec References:**
- SDK design: `specs/feature-flags-system.md:452-493`
- SDK behavior: `specs/feature-flags-system.md:489-497`
- Crate structure: `specs/feature-flags-system.md:16-37`

**Tasks:**
- [x] Create `crates/loom-flags/` crate
- [x] Implement FlagsClient
  - [x] Builder pattern for configuration
  - [x] SDK key authentication
  - [x] Base URL configuration
- [x] Initialization
  - [x] Fetch all flags on init
  - [x] Start SSE connection
- [x] Local caching
  - [x] In-memory flag cache
  - [x] Update from SSE events
- [x] Evaluation methods
  - [x] `get_bool(key, context, default)`
  - [x] `get_string(key, context, default)`
  - [x] `get_json(key, context, default)`
  - [x] `get_all(context)`
- [x] Offline mode
  - [x] Use last cached values when disconnected
- [x] Use `loom-http` for requests
  - [x] Retry logic
  - [x] User-Agent header
- [x] i18n translations (EN, ES, AR) for SDK error messages
- [x] 26 tests (unit tests + property-based tests for caching and evaluation)

---

## ✅ Phase 11: TypeScript Packages (COMPLETED)

**Goal:** `@loom/http` and `@loom/flags` packages.

**Spec References:**
- TypeScript SDK: `specs/feature-flags-system.md:474-487`
- Package structure: `specs/feature-flags-system.md:39-53`

**Tasks:**
- [x] Create `web/packages/http/` package (`@loom/http`)
  - [x] HTTP client with fetch
  - [x] Retry with exponential backoff
  - [x] Standard headers (User-Agent, Content-Type)
  - [x] Error handling (HttpError, TimeoutError, NetworkError, RateLimitError)
- [x] Create `web/packages/flags/` package (`@loom/flags`)
  - [x] FlagsClient class
  - [x] SDK key authentication
  - [x] Initialization with flag fetch
  - [x] SSE connection handling with reconnection
  - [x] Local caching (FlagCache)
  - [x] Evaluation methods (getBool, getString, getJson, getAll)
  - [x] Event emitter for updates
  - [x] Offline mode with cached values
- [x] i18n translations for SDK error messages
  - [x] Server translations in loom-common-i18n
  - [x] Web translations in loom-web
- [x] 51 tests passing (20 in @loom/http, 31 in @loom/flags)
  - [x] Property-based tests for retry delay calculation
  - [x] Property-based tests for flag cache operations
- [x] Workspace configuration for web packages (`web/pnpm-workspace.yaml`)

---

## ✅ Phase 12: Audit Integration (COMPLETED)

**Goal:** Full audit logging for all flag operations.

**Spec References:**
- Audit events: `specs/feature-flags-system.md:575-593`

**Tasks:**
- [x] Add audit event types to `loom-server-audit`
  - [x] `FlagCreated`, `FlagUpdated`, `FlagArchived`, `FlagRestored`
  - [x] `FlagConfigUpdated`
  - [x] `StrategyCreated`, `StrategyUpdated`, `StrategyDeleted`
  - [x] `KillSwitchCreated`, `KillSwitchUpdated`, `KillSwitchActivated`, `KillSwitchDeactivated`, `KillSwitchDeleted`
  - [x] `SdkKeyCreated`, `SdkKeyRevoked`
  - [x] `EnvironmentCreated`, `EnvironmentUpdated`, `EnvironmentDeleted`
- [x] Integrate audit logging into all handlers
- [x] Test audit logging (3 new tests for feature flag events)
- [x] 63 tests passing in loom-server-audit

---

## ✅ Phase 13: Platform Flags (COMPLETED)

**Goal:** Super admin management of platform-level flags.

**Spec References:**
- Two-tier system: `specs/feature-flags-system.md:235-239`
- Precedence: `specs/feature-flags-system.md:241-246`
- Platform endpoints: `specs/feature-flags-system.md:425-432`
- Permissions: `specs/feature-flags-system.md:595-618`

**Tasks:**
- [x] Implement platform flag endpoints (super admin only)
  - [x] `GET /api/admin/flags` - list platform flags
  - [x] `POST /api/admin/flags` - create platform flag
  - [x] `GET /api/admin/flags/{key}` - get platform flag by key
  - [x] `PATCH /api/admin/flags/{key}` - update platform flag
  - [x] `POST /api/admin/flags/{key}/archive` - archive platform flag
  - [x] `POST /api/admin/flags/{key}/restore` - restore archived platform flag
- [x] Implement platform kill switch endpoints
  - [x] `GET /api/admin/flags/kill-switches` - list platform kill switches
  - [x] `POST /api/admin/flags/kill-switches` - create platform kill switch
  - [x] `GET /api/admin/flags/kill-switches/{key}` - get platform kill switch
  - [x] `PATCH /api/admin/flags/kill-switches/{key}` - update platform kill switch
  - [x] `POST /api/admin/flags/kill-switches/{key}/activate` - activate kill switch
  - [x] `POST /api/admin/flags/kill-switches/{key}/deactivate` - deactivate kill switch
  - [x] `DELETE /api/admin/flags/kill-switches/{key}` - delete platform kill switch
- [x] Implement platform strategy endpoints
  - [x] `GET /api/admin/flags/strategies` - list platform strategies
  - [x] `POST /api/admin/flags/strategies` - create platform strategy
  - [x] `GET /api/admin/flags/strategies/{id}` - get platform strategy
  - [x] `PATCH /api/admin/flags/strategies/{id}` - update platform strategy
  - [x] `DELETE /api/admin/flags/strategies/{id}` - delete platform strategy
- [x] SSE broadcast for platform events
  - [x] `broadcast_to_all` method for platform-wide flag updates
- [x] i18n translations (EN, ES, AR) for all platform flag messages
- [x] Authorization tests (10 tests verifying super admin only access)
- [x] All tests passing

---

## Feature Flags Dependencies

**Rust Crates (per `specs/feature-flags-system.md:620-639`):**
- `chrono` - timestamps
- `serde`, `serde_json` - serialization
- `thiserror` - error types
- `uuid` - IDs
- `murmur3` - percentage hashing
- `eventsource-stream` - SSE client
- `sqlx` - database

**Integration Points:**
- `loom-http` - HTTP client with retry
- `loom-geoip` - GeoIP resolution
- `loom-server-audit` - audit logging
- `loom-db` - database layer
- `loom-auth` - ABAC permissions

---

## ✅ Feature Flags Testing Strategy (COMPLETED)

- [x] Unit tests for evaluation engine
  - `loom-server-flags/src/evaluation.rs`: 13 unit tests covering disabled flags, enabled flags, strategies, conditions, kill switches, schedules
- [x] Unit tests for condition matching
  - `loom-flags-core/src/strategy.rs`: 5 unit tests for attribute operators, geo operators
  - `loom-server-flags/src/evaluation.rs`: condition evaluation tests for attribute, geo, environment conditions
- [x] Unit tests for percentage hashing (verify consistency)
  - `loom-server-flags/src/evaluation.rs`: `test_percentage_consistent_hashing`, tests for 0% and 100% rollouts
  - Property-based tests: `percentage_is_deterministic`, `percentage_monotonic`
- [x] Integration tests for API endpoints
  - `loom-server/tests/authz/flags.rs`: 28 authorization tests covering all org-level flag routes
  - `loom-server/tests/authz/admin.rs`: 10 platform flags authorization tests
- [x] Integration tests for SSE streaming
  - `loom-server-flags/src/sse.rs`: 10 async tests for broadcast, subscription, cleanup, stats
  - `loom-flags-core/src/sse.rs`: serialization and event type tests
- [x] Property-based tests for strategy evaluation
  - `loom-flags-core/src/strategy.rs`: 12 proptest tests for operators, schedules, geo matching
  - `loom-server-flags/src/evaluation.rs`: 5 proptest tests for percentage hashing properties
- [x] SDK integration tests
  - `loom-flags/src/lib.rs`: 26 tests including property-based tests for caching and evaluation

**Test counts:**
- loom-flags-core: 112 tests (unit + property-based)
- loom-server-flags: 29 tests (unit + property-based + async)
- loom-server authz flags tests: 28 tests
- loom-flags SDK: 26 tests

---

## Feature Flags Deployment Notes

- Database migration must run before server starts
- Auto-create environments on org creation requires migration to existing orgs
- SSE requires appropriate timeout settings in load balancer
- SDK keys should be rotated if exposed
