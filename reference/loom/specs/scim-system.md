<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# SCIM System Specification

**Status:** Planned
**Version:** 1.0
**Last Updated:** 2026-01-03

---

## 1. Overview

### Purpose

This specification defines the SCIM (System for Cross-domain Identity Management) implementation for Loom. SCIM enables automatic user provisioning and deprovisioning from enterprise Identity Providers (IdPs) such as Okta, Azure AD, and OneLogin.

### Goals

- **RFC 7643/7644 compliance** for schema and protocol
- **Automatic provisioning** of users from IdP to Loom
- **Automatic deprovisioning** with session revocation
- **Group-to-Team mapping** for team membership sync
- **Full PATCH support** for incremental updates
- **Audit logging** of all SCIM operations

### Non-Goals

- Multi-tenant SCIM (multiple IdPs per org) — global config for v1
- Custom Loom schema extensions — core schemas only for v1
- Real-time sync — polling-based IdP integration
- Password sync — OAuth/magic link auth only

---

## 2. Architecture

### Crate Structure

```
crates/
├── loom-scim/                    # Core SCIM types and parsing
│   ├── src/
│   │   ├── lib.rs
│   │   ├── types.rs              # ScimUser, ScimGroup, Meta, etc.
│   │   ├── schema.rs             # Schema definitions
│   │   ├── error.rs              # SCIM error types
│   │   ├── filter/
│   │   │   ├── mod.rs
│   │   │   ├── parser.rs         # winnow parser
│   │   │   ├── ast.rs            # Filter AST types
│   │   │   └── eval.rs           # Evaluate against resources
│   │   └── patch.rs              # PATCH operation types
│   └── Cargo.toml
├── loom-server-scim/             # HTTP handlers
│   ├── src/
│   │   ├── lib.rs
│   │   ├── routes.rs             # Axum routes
│   │   ├── handlers/
│   │   │   ├── mod.rs
│   │   │   ├── users.rs
│   │   │   ├── groups.rs
│   │   │   ├── bulk.rs
│   │   │   ├── schemas.rs
│   │   │   └── service_provider.rs
│   │   ├── auth.rs               # SCIM bearer token middleware
│   │   ├── error.rs              # HTTP error handling
│   │   └── mapping.rs            # Loom <-> SCIM type conversions
│   └── Cargo.toml
```

---

## 3. Configuration

### Environment Variables

| Variable | Type | Description | Default |
|----------|------|-------------|---------|
| `LOOM_SERVER_SCIM_ENABLED` | boolean | Enable SCIM endpoints | `false` |
| `LOOM_SERVER_SCIM_TOKEN` | secret | Bearer token for SCIM auth | (required if enabled) |
| `LOOM_SERVER_SCIM_ORG_ID` | UUID | Org to bind SCIM provisioning to | (required if enabled) |

---

## 4. Endpoints

### Base Path

All SCIM endpoints are served under `/api/scim/*`.

### Discovery Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/scim/ServiceProviderConfig` | SCIM capabilities |
| GET | `/api/scim/Schemas` | Schema definitions |
| GET | `/api/scim/Schemas/{id}` | Single schema |
| GET | `/api/scim/ResourceTypes` | Resource type definitions |

### User Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/scim/Users` | List users (supports filter, pagination) |
| POST | `/api/scim/Users` | Create user |
| GET | `/api/scim/Users/{id}` | Get user |
| PUT | `/api/scim/Users/{id}` | Replace user |
| PATCH | `/api/scim/Users/{id}` | Update user |
| DELETE | `/api/scim/Users/{id}` | Deactivate user |

### Group Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/scim/Groups` | List groups |
| POST | `/api/scim/Groups` | Create group (maps to Team) |
| GET | `/api/scim/Groups/{id}` | Get group |
| PUT | `/api/scim/Groups/{id}` | Replace group |
| PATCH | `/api/scim/Groups/{id}` | Update group |
| DELETE | `/api/scim/Groups/{id}` | Delete group |

### Bulk Endpoint

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/scim/Bulk` | Execute multiple operations |

---

## 5. Authentication

Bearer token authentication via `LOOM_SERVER_SCIM_TOKEN` with constant-time comparison.

---

## 6. Database Changes (Migration 029)

```sql
-- users table
ALTER TABLE users ADD COLUMN scim_external_id TEXT;
ALTER TABLE users ADD COLUMN provisioned_by_scim INTEGER NOT NULL DEFAULT 0;

-- teams table  
ALTER TABLE teams ADD COLUMN scim_external_id TEXT;
ALTER TABLE teams ADD COLUMN scim_managed INTEGER NOT NULL DEFAULT 0;

-- org_memberships table
ALTER TABLE org_memberships ADD COLUMN provisioned_by TEXT;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_users_scim_external_id ON users(scim_external_id) WHERE scim_external_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_teams_scim_external_id ON teams(scim_external_id) WHERE scim_external_id IS NOT NULL;
```

---

## 7. Pagination

- Default page size: 100
- Maximum page size: 1000
- Offset-based using `startIndex` and `count`

---

## 8. Filter Support (RFC 7644)

Full filter parser using `winnow`:
- Operators: `eq`, `ne`, `co`, `sw`, `ew`, `gt`, `lt`, `ge`, `le`, `pr`
- Logical: `and`, `or`, `not`, parentheses
- Case-insensitive matching for `userName`

---

## 9. Audit Events

- `ScimUserCreated`, `ScimUserUpdated`, `ScimUserDeleted`
- `ScimGroupCreated`, `ScimGroupUpdated`, `ScimGroupDeleted`
- `ScimGroupMemberAdded`, `ScimGroupMemberRemoved`
- `ScimBulkOperation`, `ScimAuthFailure`
