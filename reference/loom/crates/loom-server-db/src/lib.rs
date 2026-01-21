// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! # loom-server-db
//!
//! Centralized persistence layer for Loom server using SQLite via sqlx.
//!
//! ## Repository Pattern
//!
//! Each domain has two components:
//! - **`*Store` trait**: Defines the interface (e.g., `UserStore`, `OrgStore`)
//! - **`*Repository` struct**: Concrete implementation holding a `SqlitePool`
//!
//! ```rust,ignore
//! #[async_trait]
//! pub trait FooStore: Send + Sync {
//!     async fn get_foo(&self, id: &FooId) -> Result<Option<Foo>, DbError>;
//!     async fn create_foo(&self, foo: &Foo) -> Result<(), DbError>;
//! }
//!
//! pub struct FooRepository {
//!     pool: SqlitePool,
//! }
//!
//! impl FooRepository {
//!     pub fn new(pool: SqlitePool) -> Self { Self { pool } }
//! }
//!
//! #[async_trait]
//! impl FooStore for FooRepository { /* delegate to inherent methods */ }
//! ```
//!
//! ## Error Handling
//!
//! Use [`DbError`] variants appropriately:
//!
//! | Variant | When to use |
//! |---------|-------------|
//! | `NotFound` | Resource must exist but doesn't (update/delete by ID, foreign key lookup) |
//! | `Conflict` | Unique constraint violation, concurrent modification, business rule conflict |
//! | `Sqlx` | Let sqlx errors propagate via `?` for unexpected database errors |
//! | `Internal` | Data corruption, invalid stored data (e.g., unparseable UUID) |
//!
//! **`Option<T>` vs `NotFound`:**
//! - Return `Result<Option<T>>` for lookups where absence is normal (get by ID, get by email)
//! - Return `DbError::NotFound` only when the caller provided an ID that should exist
//!
//! ## Return Type Conventions
//!
//! | Operation | Return type |
//! |-----------|-------------|
//! | Get by ID/unique key | `Result<Option<T>>` |
//! | List/search | `Result<Vec<T>>` or `Result<(Vec<T>, i64)>` for paginated |
//! | Create | `Result<()>` or `Result<Id>` if ID is generated |
//! | Update | `Result<()>` |
//! | Delete | `Result<bool>` (true if deleted) or `Result<()>` |
//! | Exists/count | `Result<bool>` or `Result<i64>` |
//!
//! ## Method Naming
//!
//! - `get_*_by_*` - Single item lookup (returns `Option<T>`)
//! - `list_*` - Multiple items, possibly filtered
//! - `create_*` - Insert new record
//! - `update_*` - Modify existing record
//! - `delete_*` / `soft_delete_*` - Remove or mark as deleted
//! - `count_*` - Return count
//! - `find_or_create_*` - Upsert pattern
//!
//! ## Testing
//!
//! Tests use in-memory SQLite with manually created schemas:
//!
//! ```rust,ignore
//! async fn create_test_pool() -> SqlitePool {
//!     let pool = SqlitePool::connect(":memory:").await.unwrap();
//!     sqlx::query("CREATE TABLE ...").execute(&pool).await.unwrap();
//!     pool
//! }
//!
//! #[tokio::test]
//! async fn test_example() {
//!     let pool = create_test_pool().await;
//!     let repo = FooRepository::new(pool);
//!     // test operations...
//! }
//! ```
//!
//! Prefer property-based tests (`proptest`) for ID uniqueness and pagination bounds.
//!
//! ## Adding a New Repository
//!
//! 1. Create `src/foo.rs` with module doc explaining the domain
//! 2. Define `FooStore` trait with all async methods
//! 3. Define `FooRepository` struct with `pool: SqlitePool`
//! 4. Implement inherent methods on `FooRepository` with `#[tracing::instrument]`
//! 5. Implement `FooStore for FooRepository` by delegating to inherent methods
//! 6. Add `pub mod foo;` and re-exports to this file
//! 7. Add migration to `loom-server/migrations/NNN_foo.sql`
//! 8. Add tests (unit + proptest for invariants)
//!
//! ## Instrumentation
//!
//! Use `#[tracing::instrument]` on all public methods:
//!
//! ```rust,ignore
//! #[tracing::instrument(skip(self, user), fields(user_id = %user.id))]
//! pub async fn create_user(&self, user: &User) -> Result<(), DbError> { ... }
//! ```
//!
//! Skip `self` and large/sensitive arguments; include identifying fields.

pub mod api_key;
pub mod audit;
pub mod cse;
pub mod docs;
mod error;
pub mod job;
pub mod mirror;
pub mod org;
pub mod pool;
pub mod protection;
pub mod scm;
pub mod secrets;
pub mod session;
pub mod share;
pub mod team;
pub mod thread;
pub mod types;
pub mod user;
pub mod wgtunnel;

#[cfg(test)]
pub mod testing;

pub use api_key::{ApiKeyRepository, ApiKeyStore};
pub use audit::{AuditRepository, AuditStore};
pub use cse::{normalize_cache_query, CseRepository, CseStore};
pub use docs::{DocIndexEntry, DocSearchHit, DocSearchParams, DocsRepository, DocsStore};
pub use error::{DbError, Result};
pub use job::{JobDefinition, JobRepository, JobRun, JobStatus, JobStore, TriggerSource};
pub use mirror::{
	CreateExternalMirror, CreatePushMirror, ExternalMirror, ExternalMirrorStore, MirrorBranchRule,
	MirrorRepository, Platform, PushMirror, PushMirrorStore,
};
pub use org::{OrgRepository, OrgStore};
pub use pool::create_pool;
pub use protection::{BranchProtectionRuleRecord, ProtectionRepository, ProtectionStore};
pub use scm::{
	MaintenanceJobRecord, RepoRecord, RepoTeamAccessRecord, ScmRepository, ScmStore,
	WebhookDeliveryRecord, WebhookRecord,
};
pub use secrets::{
	CreateSecretParams, CreateVersionParams, EncryptedDekRow, SecretFilterParams, SecretRow,
	SecretVersionRow, SecretsRepository, SecretsStore, StoreDekParams,
};
pub use session::{SessionRepository, SessionStore};
pub use share::{ShareRepository, ShareStore};
pub use team::{ScimTeam, TeamRepository, TeamStore};
pub use thread::{ThreadRepository, ThreadSearchHit, ThreadStore};
pub use types::{GithubInstallation, GithubInstallationInfo, GithubRepo};
pub use user::{ScimUserRow, UserRepository, UserStore};
pub use wgtunnel::{
	DeviceRowTuple, IpAllocationRow, SessionRowTuple, WeaverRowTuple, WgTunnelRepository,
	WgTunnelStore,
};
