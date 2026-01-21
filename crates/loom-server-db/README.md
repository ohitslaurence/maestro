<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# loom-server-db

Centralized persistence layer for Loom server using SQLite via sqlx.

## Repository Pattern

Each domain has two components:

- **`*Store` trait**: Defines the interface (e.g., `UserStore`, `OrgStore`)
- **`*Repository` struct**: Concrete implementation holding a `SqlitePool`

```rust
#[async_trait]
pub trait FooStore: Send + Sync {
    async fn get_foo(&self, id: &FooId) -> Result<Option<Foo>>;
    async fn create_foo(&self, foo: &Foo) -> Result<()>;
}

#[derive(Clone)]
pub struct FooRepository {
    pool: SqlitePool,
}

impl FooRepository {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
}

#[async_trait]
impl FooStore for FooRepository {
    // delegate to inherent methods
}
```

## Available Repositories

| Repository | Store Trait | Tables |
|------------|-------------|--------|
| `UserRepository` | `UserStore` | users, identities |
| `OrgRepository` | `OrgStore` | orgs, org_members, invitations, join_requests |
| `TeamRepository` | `TeamStore` | teams, team_members |
| `SessionRepository` | `SessionStore` | sessions, access_tokens, device_codes, magic_links |
| `ApiKeyRepository` | `ApiKeyStore` | api_keys |
| `ShareRepository` | `ShareStore` | shares |
| `ThreadRepository` | `ThreadStore` | threads, github_installations, github_repos |
| `ScmRepository` | `ScmStore` | repos, repo_team_access, webhooks, webhook_deliveries |
| `SecretsRepository` | `SecretsStore` | secrets, secret_versions, encrypted_deks |
| `DocsRepository` | `DocsStore` | docs_index |
| `WgTunnelRepository` | `WgTunnelStore` | weavers, devices, sessions, ip_allocations |
| `JobRepository` | `JobStore` | job_definitions, job_runs |
| `MirrorRepository` | `PushMirrorStore`, `ExternalMirrorStore` | repo_mirrors, mirror_branch_rules, external_mirrors |
| `ProtectionRepository` | `ProtectionStore` | branch_protection_rules |
| `CseRepository` | `CseStore` | cse_cache |
| `AuditRepository` | `AuditStore` | audit_logs |

## Error Handling

Use `DbError` variants appropriately:

| Variant | When to use |
|---------|-------------|
| `NotFound` | Resource must exist but doesn't (update/delete by ID, foreign key lookup) |
| `Conflict` | Unique constraint violation, concurrent modification, business rule conflict |
| `Sqlx` | Let sqlx errors propagate via `?` for unexpected database errors |
| `Internal` | Data corruption, invalid stored data (e.g., unparseable UUID) |

### `Option<T>` vs `NotFound`

- Return `Result<Option<T>>` for lookups where absence is normal (get by ID, get by email)
- Return `DbError::NotFound` only when the caller provided an ID that **should** exist

## Return Type Conventions

| Operation | Return type |
|-----------|-------------|
| Get by ID/unique key | `Result<Option<T>>` |
| List/search | `Result<Vec<T>>` or `Result<(Vec<T>, i64)>` for paginated |
| Create | `Result<()>` or `Result<Id>` if ID is generated |
| Update | `Result<()>` |
| Delete | `Result<bool>` (true if deleted) or `Result<()>` |
| Exists/count | `Result<bool>` or `Result<i64>` |

## Method Naming

| Pattern | Purpose |
|---------|---------|
| `get_*_by_*` | Single item lookup (returns `Option<T>`) |
| `list_*` | Multiple items, possibly filtered |
| `create_*` | Insert new record |
| `update_*` | Modify existing record |
| `delete_*` / `soft_delete_*` | Remove or mark as deleted |
| `count_*` | Return count |
| `find_or_create_*` | Upsert pattern |

## Testing

Tests use in-memory SQLite with manually created schemas. Use the shared test helpers:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::create_test_pool;

    #[tokio::test]
    async fn test_example() {
        let pool = create_test_pool().await;
        let repo = FooRepository::new(pool);
        // test operations...
    }
}
```

Prefer property-based tests (`proptest`) for ID uniqueness and pagination bounds.

## Adding a New Repository

1. **Create module**: `src/foo.rs` with module doc explaining the domain
2. **Define trait**: `FooStore` with all async methods, using `#[async_trait]`
3. **Define struct**: `FooRepository` with `pool: SqlitePool`
4. **Implement inherent methods** on `FooRepository` with `#[tracing::instrument]`
5. **Implement trait**: `FooStore for FooRepository` by delegating to inherent methods
6. **Export**: Add `pub mod foo;` and re-exports to `lib.rs`
7. **Migration**: Add `loom-server/migrations/NNN_foo.sql`
8. **Tests**: Add unit tests + proptest for invariants

## Instrumentation

Use `#[tracing::instrument]` on all public methods:

```rust
#[tracing::instrument(skip(self, user), fields(user_id = %user.id))]
pub async fn create_user(&self, user: &User) -> Result<()> {
    // ...
}
```

- Skip `self` and large/sensitive arguments
- Include identifying fields (IDs, names)
- Never log secrets or PII in fields

## Dependencies

This crate depends on domain crates for types:

- `loom-server-auth` - User, Org, Team, Session types
- `loom-server-audit` - Audit event types
- `loom-common-thread` - Thread, AgentState types
- `loom-common-secret` - SecretString

SQL operations are centralized here; domain crates define business logic.
