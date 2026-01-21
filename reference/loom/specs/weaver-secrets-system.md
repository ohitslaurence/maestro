<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Weaver Secrets System Specification

**Status:** Planned\
**Version:** 1.0\
**Last Updated:** 2026-01-03

---

## 1. Overview

### 1.1 Purpose

The Weaver Secrets System provides secure, identity-based secret management for Loom weavers. It enables:

- **Secret Definition**: Users and organizations define reusable secrets (API keys, tokens, credentials)
- **Cryptographic Identity**: Weavers receive verifiable identities (SPIFFE-style SVIDs) for authentication
- **Runtime Secret Access**: Weavers pull secrets on-demand via secure HTTP API, never via environment variables
- **Fine-grained Access Control**: Secrets scoped at org, repo, and weaver levels with ABAC enforcement

### 1.2 Goals

- **Identity-based access**: Every weaver has a verifiable cryptographic identity; secrets granted based on org/repo/weaver attributes, not IP or network position
- **Encrypted at rest**: All secret values encrypted under a master key using envelope encryption
- **Pull-based runtime access**: Weavers explicitly request secrets over TLS; no pre-injection via env or image
- **Fine-grained scoping**: Secret scoping at org, repo, and weaver level, aligned with ABAC/SCM models
- **Zero-trust friendly**: Assume cluster network is hostile; authenticate and authorize every request
- **Auditability**: Secret lifecycle and access events fully audited (no secret values logged)
- **Vault-like UX**: Adopt familiar patterns (paths, policies, leases) while keeping implementation simple
- **HSM-ready**: Abstraction layer for future hardware security module integration

### 1.3 Non-Goals (v1)

- Multi-cluster or multi-region secret replication
- Full SPIFFE/SPIRE deployment (we implement a subset of SPIFFE concepts)
- Automatic secret rotation with external providers (AWS Secrets Manager, etc.)
- Sidecars or init containers for weavers (remain single-container Pods)
- K8s Secrets as the primary secret store (we use our own encrypted store)
- Constant-time comparison (not required for current use cases)

---

## 2. Design Principles & Threat Model

### 2.1 Zero-Trust Principles

| Principle | Implementation |
|-----------|----------------|
| Never trust the network | All secret operations authenticated and authorized; no reliance on namespace or IP |
| Short-lived credentials | Short TTLs for weaver identity tokens (15 min) and derived credentials |
| Least privilege | Secrets scoped tightly (org/repo/weaver); policies default-deny |
| Explicit access | Weavers must explicitly fetch secrets; nothing magically injected into env |
| Defense in depth | Multiple authorization layers (SVID validation, ABAC, audit) |

### 2.2 Threat Model

**Protects Against:**

| Threat | Mitigation |
|--------|------------|
| Compromised weaver pod | Short SVID TTL, least privilege scoping, per-weaver secrets |
| Lateral movement from other pods | K8s SA JWT validation, Pod label verification, namespace isolation |
| Secrets in logs/config dumps | `SecretString` type with redacted Debug/Display/Serialize |
| Database compromise | Envelope encryption; secrets unreadable without KEK |
| Replay of weaver identity tokens | Short TTL, Pod UID binding, optional online Pod existence check |
| Unauthorized secret access | ABAC policy enforcement, audit logging |

**Out of Scope (v1):**

- Host/kernel compromise
- Advanced side-channel attacks
- Nation-state actors with physical hardware access
- Memory inspection before drop (use shorter-lived secrets)

---

## 3. Architecture

### 3.1 Crate Structure

```
crates/
├── loom-server-secrets/          # Server-side secrets management
│   ├── src/
│   │   ├── lib.rs                # Public API exports
│   │   ├── config.rs             # Configuration types
│   │   ├── types.rs              # Secret, SecretVersion, SecretScope
│   │   ├── store.rs              # SecretStore trait + SQLite impl
│   │   ├── encryption.rs         # Envelope encryption logic
│   │   ├── key_backend.rs        # KeyBackend trait (software/HSM)
│   │   ├── key_backend_software.rs  # Software key backend
│   │   ├── svid.rs               # Weaver SVID issuance/validation
│   │   ├── policy.rs             # ABAC integration for secrets
│   │   └── error.rs              # Error types
│   └── Cargo.toml
├── loom-weaver-secrets/          # Client library for weavers
│   ├── src/
│   │   ├── lib.rs                # Public API
│   │   ├── client.rs             # HTTP client for secrets API
│   │   ├── svid.rs               # SVID acquisition and refresh
│   │   └── error.rs              # Error types
│   └── Cargo.toml
└── loom-server/
    └── src/
        └── routes/
            ├── secrets.rs        # User-facing secret management APIs
            └── weaver_secrets.rs # Internal weaver secret access APIs
```

### 3.2 Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              User/Admin                                      │
│                    (Web UI, CLI, API)                                        │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │ HTTPS + Session/Token Auth
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐           │
│  │  /api/.../secrets│  │  /internal/      │  │  /internal/      │           │
│  │  (User APIs)     │  │  weaver-auth     │  │  weaver-secrets  │           │
│  │                  │  │  (SVID issuance) │  │  (Secret fetch)  │           │
│  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘           │
│           │                     │                     │                      │
│           └─────────────────────┼─────────────────────┘                      │
│                                 ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                    loom-server-secrets                                │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐      │   │
│  │  │ SecretStore│  │ KeyBackend │  │ SVID Issuer│  │ ABAC Policy│      │   │
│  │  │ (SQLite)   │  │ (Software) │  │            │  │            │      │   │
│  │  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘      │   │
│  │        │               │               │               │              │   │
│  └────────┼───────────────┼───────────────┼───────────────┼──────────────┘   │
│           │               │               │               │                  │
│           ▼               ▼               ▼               ▼                  │
│  ┌────────────────────────────────────────────────────────────────────┐     │
│  │                         SQLite Database                             │     │
│  │  secrets | secret_versions | encrypted_deks | audit_logs           │     │
│  └────────────────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────────────┘
                               ▲
                               │ HTTPS + K8s SA JWT → Weaver SVID
                               │
┌──────────────────────────────┴──────────────────────────────────────────────┐
│                           Weaver Pod                                         │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │  loom-weaver-secrets client library                                   │   │
│  │  1. Read K8s SA JWT from /var/run/secrets/.../token                   │   │
│  │  2. POST /internal/weaver-auth/token → Weaver SVID                    │   │
│  │  3. GET /internal/weaver-secrets/v1/secrets/{scope}/{name}            │   │
│  │  4. Use secret in memory, zeroize when done                           │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 3.3 Data Flow: Weaver Secret Access

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Weaver Pod Starts                                                         │
│    K8s injects:                                                              │
│    - /var/run/secrets/kubernetes.io/serviceaccount/token (SA JWT)           │
│    - Pod labels: loom.dev/weaver-id, loom.dev/org-id, loom.dev/repo-id      │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Weaver Code: Obtain SVID                                                  │
│    POST /internal/weaver-auth/token                                          │
│    Authorization: Bearer <k8s_sa_jwt>                                        │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Secrets Service: Validate & Issue SVID                                    │
│    a. Validate SA JWT via K8s TokenReview API                                │
│    b. Fetch Pod via K8s API, verify loom.dev/managed=true                    │
│    c. Extract weaver-id, org-id, repo-id from labels                         │
│    d. Sign Weaver SVID (JWT) with KeyBackend                                 │
│    e. Return SVID (15 min TTL)                                               │
│    f. Audit: WeaverSvidIssued                                                │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Weaver Code: Fetch Secret                                                 │
│    GET /internal/weaver-secrets/v1/secrets/org/STRIPE_API_KEY               │
│    Authorization: Bearer <weaver_svid>                                       │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 5. Secrets Service: Authorize & Return Secret                                │
│    a. Validate Weaver SVID signature and expiry                              │
│    b. Optional: verify Pod still exists via K8s API                          │
│    c. Run ABAC policy check (weaver.org_id == secret.org_id)                 │
│    d. Decrypt secret value (envelope decryption)                             │
│    e. Return plaintext value                                                 │
│    f. Audit: SecretAccessed                                                  │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 6. Weaver Code: Use Secret                                                   │
│    - Hold in SecretString (memory)                                           │
│    - Use for API calls, DB connections, etc.                                 │
│    - Zeroize on drop                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Comparison: GitHub Actions vs Loom Weavers vs SPIFFE

### 4.1 GitHub Actions Secrets Model

| Aspect | GitHub Actions | Loom Weavers |
|--------|----------------|--------------|
| Workload identity | Job token / OIDC token | K8s SA JWT → Weaver SVID (SPIFFE-style) |
| Secret storage | GitHub internal, regionally replicated | SQLite + envelope encryption |
| Secret injection | Env vars + masked logs | **Pull via HTTP API; no secrets in env** |
| Scoping | Org / repo / environment | Org / repo / weaver |
| Rotation | UI/API, new jobs see new values | Secret versions, weavers fetch latest |
| Log masking | Automatic masking in logs | `SecretString` + structured logging redaction |
| Identity trust | GitHub platform trust | K8s platform trust + Loom CA |

### 4.2 SPIFFE/SPIRE Alignment

| SPIFFE Concept | Loom Implementation |
|----------------|---------------------|
| SPIFFE ID | `spiffe://loom.dev/weaver/{weaver-id}` |
| SVID Type | JWT-SVID (v1); X.509 possible in future |
| Trust Bundle | Loom-managed signing key (own CA) |
| Workload Attestation | K8s SA JWT + Pod labels |
| SPIRE Agent | Not used (v1); can migrate later |

### 4.3 Why Not Full SPIRE?

- **Complexity**: SPIRE requires agents on every node, complex attestation
- **Dependencies**: External project with its own upgrade/maintenance burden
- **Scope**: Loom only needs weaver identity, not full mesh identity
- **Future path**: Design allows SPIRE adoption if multi-cluster needs arise

---

## 5. Weaver Identity & SVID

### 5.1 SPIFFE ID Format

```
spiffe://loom.dev/weaver/{weaver-id}
```

Example:
```
spiffe://loom.dev/weaver/018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g
```

### 5.2 Weaver SVID (JWT) Structure

```json
{
  "sub": "spiffe://loom.dev/weaver/018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "weaver_id": "018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "pod_name": "weaver-018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "pod_namespace": "loom-weavers",
  "pod_uid": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "org_id": "org-uuid-here",
  "repo_id": "repo-uuid-here",
  "owner_user_id": "user-uuid-here",
  "iat": 1730000000,
  "exp": 1730000900,
  "iss": "loom-secrets",
  "aud": ["loom-secrets"]
}
```

### 5.3 SVID Issuance Flow

```rust
pub struct WeaverClaims {
    pub sub: String,           // SPIFFE ID
    pub weaver_id: String,
    pub pod_name: String,
    pub pod_namespace: String,
    pub pod_uid: String,
    pub org_id: String,
    pub repo_id: Option<String>,
    pub owner_user_id: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
    pub aud: Vec<String>,
}
```

### 5.4 K8s Pod Labels (Extended)

Update weaver provisioner to include new labels:

```yaml
metadata:
  name: weaver-018f6b2a-...
  labels:
    loom.dev/managed: "true"
    loom.dev/weaver-id: "018f6b2a-..."
    loom.dev/owner-user-id: "user-uuid-here"
    loom.dev/org-id: "org-uuid-here"         # NEW
    loom.dev/repo-id: "repo-uuid-here"       # NEW (optional)
  annotations:
    loom.dev/tags: '{"project":"ai-worker"}'
    loom.dev/lifetime-hours: "4"
```

### 5.5 SVID Validation

Every weaver-scoped endpoint validates:

1. JWT signature against `KeyBackend` public key
2. `exp` > now (not expired)
3. `iss` == "loom-secrets"
4. `aud` contains "loom-secrets"
5. `pod_namespace` matches expected weaver namespace
6. Optional (configurable): Pod still exists with `loom.dev/managed=true`

---

## 6. Secret Modeling & Scoping

### 6.1 Secret Scopes

| Scope | Description | Access Rule |
|-------|-------------|-------------|
| `org` | Organization-wide secret | `weaver.org_id == secret.org_id` |
| `repo` | Repository-specific secret | `weaver.repo_id == secret.repo_id` |
| `weaver` | Weaver-instance secret (ephemeral) | `weaver.weaver_id == secret.weaver_id` |

### 6.2 Secret Resolution Order

When fetching by name, secrets are resolved in order (first match wins):

1. **Weaver scope** (if applicable)
2. **Repo scope** (if weaver has repo_id)
3. **Org scope**

This allows repo secrets to override org secrets with the same name (like GitHub Actions).

### 6.3 Secret Naming Convention

```
^[A-Z][A-Z0-9_]{0,127}$
```

- Uppercase letters, digits, underscores
- Must start with a letter
- 1-128 characters
- Unique per `(org_id, scope, repo_id?, weaver_id?)`

Examples:
- `STRIPE_API_KEY`
- `DATABASE_URL`
- `AWS_ACCESS_KEY_ID`

### 6.4 Secret Versioning

Each secret has multiple versions:

```
Secret: STRIPE_API_KEY
├── Version 1 (created 2025-01-01, disabled)
├── Version 2 (created 2025-01-15, disabled)
└── Version 3 (created 2025-02-01, current) ← default when fetching
```

- `current_version` pointer indicates the "live" version
- Old versions retained for rollback
- Specific versions can be requested by ID

---

## 7. Database Schema

### 7.1 Core Tables

```sql
-- Secrets metadata
CREATE TABLE secrets (
    id              TEXT PRIMARY KEY,                    -- UUID
    org_id          TEXT NOT NULL REFERENCES organizations(id),
    scope           TEXT NOT NULL CHECK (scope IN ('org', 'repo', 'weaver')),
    repo_id         TEXT REFERENCES repos(id),           -- for repo scope
    weaver_id       TEXT,                                -- for weaver scope (UUID7)
    name            TEXT NOT NULL,                       -- 'STRIPE_API_KEY'
    description     TEXT,                                -- optional description
    current_version TEXT NOT NULL REFERENCES secret_versions(id),
    created_by      TEXT NOT NULL REFERENCES users(id),
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at      TEXT,                                -- soft delete

    CONSTRAINT unique_secret_name UNIQUE (org_id, scope, repo_id, weaver_id, name)
);

CREATE INDEX idx_secrets_org ON secrets(org_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_secrets_repo ON secrets(repo_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_secrets_weaver ON secrets(weaver_id) WHERE deleted_at IS NULL;

-- Secret versions (encrypted values)
CREATE TABLE secret_versions (
    id              TEXT PRIMARY KEY,                    -- UUID
    secret_id       TEXT NOT NULL REFERENCES secrets(id),
    version         INTEGER NOT NULL,                    -- 1, 2, 3, ...
    ciphertext      BLOB NOT NULL,                       -- AES-GCM encrypted payload
    nonce           BLOB NOT NULL,                       -- encryption nonce
    dek_id          TEXT NOT NULL REFERENCES encrypted_deks(id),
    created_by      TEXT NOT NULL REFERENCES users(id),
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT,                                -- optional logical expiry
    disabled_at     TEXT,                                -- revoked/rotated

    CONSTRAINT unique_version UNIQUE (secret_id, version)
);

CREATE INDEX idx_secret_versions_secret ON secret_versions(secret_id, version);

-- Encrypted Data Encryption Keys (envelope encryption)
CREATE TABLE encrypted_deks (
    id              TEXT PRIMARY KEY,                    -- UUID
    encrypted_key   BLOB NOT NULL,                       -- DEK encrypted under KEK
    key_algorithm   TEXT NOT NULL DEFAULT 'aes-256-gcm', -- cipher for DEK usage
    kek_version     INTEGER NOT NULL DEFAULT 1,          -- which KEK version
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Weaver SVID issuance log (for audit/debugging)
CREATE TABLE weaver_svids (
    id              TEXT PRIMARY KEY,                    -- UUID
    weaver_id       TEXT NOT NULL,
    pod_name        TEXT NOT NULL,
    pod_namespace   TEXT NOT NULL,
    pod_uid         TEXT NOT NULL,
    org_id          TEXT NOT NULL,
    repo_id         TEXT,
    owner_user_id   TEXT NOT NULL,
    issued_at       TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT NOT NULL,
    revoked_at      TEXT                                 -- for explicit revocation
);

CREATE INDEX idx_weaver_svids_weaver ON weaver_svids(weaver_id);
CREATE INDEX idx_weaver_svids_issued ON weaver_svids(issued_at);
```

### 7.2 Migration File

`crates/loom-server/migrations/030_weaver_secrets.sql`

---

## 8. Key Management & Encryption

### 8.1 Envelope Encryption Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Envelope Encryption                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Master Key (KEK)           Data Encryption Key (DEK)       Secret Value    │
│  ─────────────────          ───────────────────────         ────────────    │
│  256-bit AES key            256-bit AES key                 Plaintext       │
│  From env/HSM               Random per secret               User's secret   │
│                                                                              │
│       │                           │                              │          │
│       │    encrypts               │      encrypts                │          │
│       └──────────────►┌───────────┴───────────┐◄─────────────────┘          │
│                       │                       │                              │
│                       ▼                       ▼                              │
│                  encrypted_dek           ciphertext                          │
│                  (stored in DB)          (stored in DB)                      │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 8.2 KeyBackend Trait

```rust
#[async_trait]
pub trait KeyBackend: Send + Sync {
    /// Encrypt a DEK under the master key
    async fn encrypt_dek(&self, dek_plaintext: &[u8]) -> Result<EncryptedDek, KeyError>;

    /// Decrypt a DEK using the master key
    async fn decrypt_dek(&self, encrypted: &EncryptedDek) -> Result<SecretVec<u8>, KeyError>;

    /// Sign a Weaver SVID JWT
    async fn sign_weaver_svid(&self, claims: &WeaverClaims) -> Result<String, KeyError>;

    /// Verify a Weaver SVID JWT signature
    async fn verify_weaver_svid(&self, token: &str) -> Result<WeaverClaims, KeyError>;

    /// Get the public key ID for SVID verification
    fn svid_signing_key_id(&self) -> &str;

    /// Get the JWKS for public key distribution
    fn jwks(&self) -> JsonWebKeySet;
}

pub struct EncryptedDek {
    pub id: String,
    pub encrypted_key: Vec<u8>,
    pub kek_version: u32,
}
```

### 8.3 SoftwareKeyBackend (v1 Implementation)

```rust
pub struct SoftwareKeyBackend {
    kek: SecretVec<u8>,           // Master key from env
    svid_signing_key: Ed25519KeyPair,  // For SVID JWT signing
    kek_version: u32,
}

impl SoftwareKeyBackend {
    pub fn from_env() -> Result<Self, KeyError> {
        let kek = load_secret_env("LOOM_SECRETS_MASTER_KEY")?
            .ok_or(KeyError::MissingMasterKey)?;

        // Derive or load SVID signing key
        let svid_key = load_or_generate_svid_key()?;

        Ok(Self {
            kek: SecretVec::new(decode_key(&kek)?),
            svid_signing_key: svid_key,
            kek_version: 1,
        })
    }
}
```

### 8.4 Configuration

```bash
# Master key for envelope encryption (required)
export LOOM_SECRETS_MASTER_KEY="base64-encoded-256-bit-key"
# Or file-based:
export LOOM_SECRETS_MASTER_KEY_FILE="/run/secrets/loom-master-key"

# SVID signing key (optional, auto-generated if missing)
export LOOM_SECRETS_SVID_SIGNING_KEY="base64-encoded-ed25519-private-key"
export LOOM_SECRETS_SVID_SIGNING_KEY_FILE="/run/secrets/svid-signing-key"

# SVID TTL (default: 15 minutes)
export LOOM_SECRETS_SVID_TTL_SECONDS="900"
```

### 8.5 Future: HSM Backend

```rust
pub struct HsmKeyBackend {
    pkcs11_ctx: Pkcs11Context,
    kek_slot: SlotId,
    svid_slot: SlotId,
}

// Or cloud KMS:
pub struct AwsKmsKeyBackend {
    kms_client: aws_sdk_kms::Client,
    kek_key_id: String,
    svid_key_id: String,
}
```

The `KeyBackend` trait allows swapping implementations without changing other code.

---

## 9. API Endpoints

### 9.1 User-Facing Secret Management

#### Org Secrets

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/orgs/{org_id}/secrets` | Create org secret |
| GET | `/api/orgs/{org_id}/secrets` | List org secrets (metadata only) |
| GET | `/api/orgs/{org_id}/secrets/{name}` | Get secret metadata |
| PATCH | `/api/orgs/{org_id}/secrets/{name}` | Update secret (new version) |
| DELETE | `/api/orgs/{org_id}/secrets/{name}` | Soft delete secret |
| POST | `/api/orgs/{org_id}/secrets/{name}/rotate` | Create new version |
| GET | `/api/orgs/{org_id}/secrets/{name}/versions` | List versions |

#### Repo Secrets

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/repos/{repo_id}/secrets` | Create repo secret |
| GET | `/api/repos/{repo_id}/secrets` | List repo secrets |
| GET | `/api/repos/{repo_id}/secrets/{name}` | Get secret metadata |
| PATCH | `/api/repos/{repo_id}/secrets/{name}` | Update secret |
| DELETE | `/api/repos/{repo_id}/secrets/{name}` | Soft delete secret |

#### Request/Response Examples

**Create Secret:**

```http
POST /api/orgs/org-uuid/secrets
Content-Type: application/json
Authorization: Bearer <session_token>

{
  "name": "STRIPE_API_KEY",
  "value": "sk_live_abc123...",
  "description": "Production Stripe API key"
}
```

**Response (201 Created):**

```json
{
  "id": "secret-uuid",
  "name": "STRIPE_API_KEY",
  "scope": "org",
  "org_id": "org-uuid",
  "description": "Production Stripe API key",
  "current_version": 1,
  "created_at": "2025-02-01T12:00:00Z",
  "created_by": {
    "id": "user-uuid",
    "username": "ghuntley"
  }
}
```

**Note:** Secret values are never returned after creation.

### 9.2 Weaver Authentication (Internal)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/internal/weaver-auth/token` | Exchange K8s SA JWT for Weaver SVID |
| GET | `/internal/weaver-auth/.well-known/jwks.json` | JWKS for SVID verification |

**Request:**

```http
POST /internal/weaver-auth/token
Content-Type: application/json
Authorization: Bearer <k8s_sa_jwt>

{
  "pod_name": "weaver-018f6b2a-...",
  "pod_namespace": "loom-weavers"
}
```

**Response:**

```json
{
  "token": "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_at": "2025-02-01T12:15:00Z",
  "spiffe_id": "spiffe://loom.dev/weaver/018f6b2a-..."
}
```

### 9.3 Weaver Secret Access (Internal)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/internal/weaver-secrets/v1/secrets/org/{name}` | Fetch org secret |
| GET | `/internal/weaver-secrets/v1/secrets/repo/{name}` | Fetch repo secret |
| GET | `/internal/weaver-secrets/v1/secrets/weaver/{name}` | Fetch weaver secret |
| GET | `/internal/weaver-secrets/v1/secrets/{name}` | Auto-resolve by scope priority |

**Request:**

```http
GET /internal/weaver-secrets/v1/secrets/org/STRIPE_API_KEY
Authorization: Bearer <weaver_svid>
```

**Response:**

```json
{
  "name": "STRIPE_API_KEY",
  "scope": "org",
  "version": 3,
  "value": "sk_live_abc123...",
  "expires_at": null
}
```

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| `version` | integer | Fetch specific version (default: current) |

### 9.4 System APIs (Internal)

| Method | Path | Description |
|--------|------|-------------|
| POST | `/internal/weavers/{weaver_id}/secrets` | Create weaver-scoped secret (for SCM, etc.) |
| DELETE | `/internal/weavers/{weaver_id}/secrets/{name}` | Delete weaver secret |

---

## 10. ABAC Integration

### 10.1 Secret Resource Attributes

```rust
pub struct SecretResourceAttrs {
    pub resource_type: &'static str,  // "secret"
    pub resource_id: String,          // secret_id
    pub scope: SecretScope,           // org | repo | weaver
    pub org_id: String,
    pub repo_id: Option<String>,
    pub weaver_id: Option<String>,
    pub name: String,
}
```

### 10.2 Weaver Principal Attributes

```rust
pub struct WeaverPrincipalAttrs {
    pub principal_type: &'static str,  // "weaver"
    pub weaver_id: String,
    pub org_id: String,
    pub repo_id: Option<String>,
    pub owner_user_id: String,
    pub pod_name: String,
    pub pod_namespace: String,
}
```

### 10.3 Access Policies

**Org Secret Access:**

```rust
fn can_access_org_secret(weaver: &WeaverPrincipalAttrs, secret: &SecretResourceAttrs) -> bool {
    secret.scope == SecretScope::Org
        && weaver.org_id == secret.org_id
}
```

**Repo Secret Access:**

```rust
fn can_access_repo_secret(weaver: &WeaverPrincipalAttrs, secret: &SecretResourceAttrs) -> bool {
    secret.scope == SecretScope::Repo
        && weaver.repo_id.as_ref() == secret.repo_id.as_ref()
        && weaver.org_id == secret.org_id
}
```

**Weaver Secret Access:**

```rust
fn can_access_weaver_secret(weaver: &WeaverPrincipalAttrs, secret: &SecretResourceAttrs) -> bool {
    secret.scope == SecretScope::Weaver
        && Some(&weaver.weaver_id) == secret.weaver_id.as_ref()
}
```

### 10.4 User Management Permissions

| Role | Capabilities |
|------|--------------|
| Org Owner | Full secret management for org and all repos |
| Org Admin | Full secret management for org and all repos |
| Repo Admin | Secret management for specific repo only |
| Org Member | Read secret metadata (not values) |

---

## 11. SCM Integration

### 11.1 Git Credential Flow

The SCM system creates weaver-scoped secrets for git authentication:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Weaver Launch Request                                                     │
│    POST /api/weaver { repo: "github.com/org/repo" }                         │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Weaver Provisioner                                                        │
│    a. Create weaver Pod with labels (org-id, repo-id)                        │
│    b. Call SCM system to generate short-lived git credentials                │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. SCM System                                                                │
│    POST /internal/weavers/{weaver_id}/secrets                                │
│    Creates: GIT_HTTP_USERNAME, GIT_HTTP_PASSWORD (weaver-scoped)             │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 4. Weaver Code                                                               │
│    a. GET /internal/weaver-secrets/v1/secrets/weaver/GIT_HTTP_USERNAME       │
│    b. GET /internal/weaver-secrets/v1/secrets/weaver/GIT_HTTP_PASSWORD       │
│    c. Configure git credential helper with fetched values                    │
│    d. Clone/push repository                                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 11.2 Benefits

- All credentials go through unified encryption and audit
- Easy global revocation (disable weaver-scoped secrets on weaver termination)
- Consistent access patterns for all secret types

---

## 12. Audit Integration

### 12.1 New Audit Event Types

```rust
pub enum AuditEventType {
    // ... existing types ...

    // Secret lifecycle events
    SecretCreated,
    SecretUpdated,
    SecretDeleted,
    SecretRotated,
    SecretVersionDisabled,

    // Secret access events
    SecretAccessed,
    SecretAccessDenied,

    // Weaver identity events
    WeaverSvidIssued,
    WeaverSvidRejected,
    WeaverSvidRevoked,
}
```

### 12.2 Event Details

**SecretCreated:**

```json
{
  "event_type": "secret_created",
  "actor_user_id": "user-uuid",
  "resource_type": "secret",
  "resource_id": "secret-uuid",
  "details": {
    "name": "STRIPE_API_KEY",
    "scope": "org",
    "org_id": "org-uuid",
    "description": "Production Stripe API key"
  }
}
```

**SecretAccessed (weaver):**

```json
{
  "event_type": "secret_accessed",
  "actor_user_id": null,
  "resource_type": "secret",
  "resource_id": "secret-uuid",
  "details": {
    "weaver_id": "018f6b2a-...",
    "scope": "org",
    "name": "STRIPE_API_KEY",
    "version": 3,
    "spiffe_id": "spiffe://loom.dev/weaver/018f6b2a-..."
  }
}
```

**WeaverSvidIssued:**

```json
{
  "event_type": "weaver_svid_issued",
  "actor_user_id": null,
  "resource_type": "weaver",
  "resource_id": "018f6b2a-...",
  "details": {
    "pod_name": "weaver-018f6b2a-...",
    "pod_namespace": "loom-weavers",
    "org_id": "org-uuid",
    "repo_id": "repo-uuid",
    "owner_user_id": "user-uuid",
    "expires_at": "2025-02-01T12:15:00Z"
  }
}
```

### 12.3 No Secret Values in Logs

- Secret values are **never** included in audit events
- Only metadata: name, scope, version, accessor identity
- Use `SecretString` throughout server code

---

## 13. Client Library: loom-weaver-secrets

### 13.1 Public API

```rust
use loom_weaver_secrets::{SecretsClient, SecretScope};
use loom_secret::SecretString;

// Initialize client (auto-discovers server endpoint in-cluster)
let client = SecretsClient::new()?;

// Fetch org secret
let stripe_key: SecretString = client.get_secret(SecretScope::Org, "STRIPE_API_KEY").await?;

// Use secret
let stripe_client = stripe::Client::new(stripe_key.expose());

// Secret is zeroized when `stripe_key` is dropped
```

### 13.2 Implementation

```rust
pub struct SecretsClient {
    http_client: reqwest::Client,
    server_url: Url,
    svid: RwLock<Option<CachedSvid>>,
}

struct CachedSvid {
    token: String,
    expires_at: DateTime<Utc>,
}

impl SecretsClient {
    pub fn new() -> Result<Self, Error> {
        let server_url = std::env::var("LOOM_SECRETS_SERVER_URL")
            .unwrap_or_else(|_| "http://loom-server.loom-weavers.svc.cluster.local".into());

        Ok(Self {
            http_client: reqwest::Client::new(),
            server_url: Url::parse(&server_url)?,
            svid: RwLock::new(None),
        })
    }

    pub async fn get_secret(&self, scope: SecretScope, name: &str) -> Result<SecretString, Error> {
        let svid = self.ensure_svid().await?;

        let path = match scope {
            SecretScope::Org => format!("/internal/weaver-secrets/v1/secrets/org/{}", name),
            SecretScope::Repo => format!("/internal/weaver-secrets/v1/secrets/repo/{}", name),
            SecretScope::Weaver => format!("/internal/weaver-secrets/v1/secrets/weaver/{}", name),
        };

        let resp = self.http_client
            .get(self.server_url.join(&path)?)
            .bearer_auth(&svid)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Error::AccessDenied(resp.status()));
        }

        let body: SecretResponse = resp.json().await?;
        Ok(SecretString::new(body.value))
    }

    async fn ensure_svid(&self) -> Result<String, Error> {
        // Check cache
        {
            let cached = self.svid.read().await;
            if let Some(ref svid) = *cached {
                if svid.expires_at > Utc::now() + Duration::seconds(60) {
                    return Ok(svid.token.clone());
                }
            }
        }

        // Refresh SVID
        let new_svid = self.obtain_svid().await?;
        let mut cached = self.svid.write().await;
        *cached = Some(new_svid.clone());
        Ok(new_svid.token)
    }

    async fn obtain_svid(&self) -> Result<CachedSvid, Error> {
        // Read K8s SA token
        let sa_token = tokio::fs::read_to_string(
            "/var/run/secrets/kubernetes.io/serviceaccount/token"
        ).await?;

        let pod_name = std::env::var("HOSTNAME")?;
        let pod_namespace = tokio::fs::read_to_string(
            "/var/run/secrets/kubernetes.io/serviceaccount/namespace"
        ).await?.trim().to_string();

        let resp = self.http_client
            .post(self.server_url.join("/internal/weaver-auth/token")?)
            .bearer_auth(&sa_token)
            .json(&serde_json::json!({
                "pod_name": pod_name,
                "pod_namespace": pod_namespace
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(Error::SvidIssuanceFailed(resp.status()));
        }

        let body: SvidResponse = resp.json().await?;
        Ok(CachedSvid {
            token: body.token,
            expires_at: body.expires_at,
        })
    }
}
```

---

## 14. Kubernetes Integration

### 14.1 Service Account Configuration

Weavers use the default service account in `loom-weavers` namespace:

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: default
  namespace: loom-weavers
```

### 14.2 RBAC for Token Review

`loom-server` needs permission to validate SA tokens:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: loom-token-reviewer
rules:
  - apiGroups: ["authentication.k8s.io"]
    resources: ["tokenreviews"]
    verbs: ["create"]
  - apiGroups: [""]
    resources: ["pods"]
    verbs: ["get"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: loom-token-reviewer
subjects:
  - kind: ServiceAccount
    name: loom-server
    namespace: loom
roleRef:
  kind: ClusterRole
  name: loom-token-reviewer
  apiGroup: rbac.authorization.k8s.io
```

### 14.3 Network Policy (Optional)

Restrict weaver network access:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: weaver-egress
  namespace: loom-weavers
spec:
  podSelector:
    matchLabels:
      loom.dev/managed: "true"
  policyTypes:
    - Egress
  egress:
    # Allow access to loom-server for secrets
    - to:
        - namespaceSelector:
            matchLabels:
              name: loom
          podSelector:
            matchLabels:
              app: loom-server
      ports:
        - port: 8080
    # Allow DNS
    - to:
        - namespaceSelector: {}
          podSelector:
            matchLabels:
              k8s-app: kube-dns
      ports:
        - port: 53
          protocol: UDP
    # Allow public internet (configurable)
    - to:
        - ipBlock:
            cidr: 0.0.0.0/0
            except:
              - 10.0.0.0/8
              - 172.16.0.0/12
              - 192.168.0.0/16
```

---

## 15. Security Considerations

### 15.1 What This Design Protects Against

| Threat | Mitigation |
|--------|------------|
| Secrets in environment variables | Pull-only API; no env injection |
| Secrets in container images | No baked-in secrets |
| Secrets in logs | `SecretString` with redacted Debug/Display |
| Unauthorized pod accessing secrets | K8s SA JWT + Pod label validation |
| Stolen SVID reuse | Short TTL (15 min), Pod UID binding |
| Database compromise | Envelope encryption; KEK required |
| Replay attacks | Token expiry, optional online Pod check |

### 15.2 Best Practices

1. **Rotate master key periodically**: Update KEK and re-encrypt DEKs
2. **Use HSM in production**: Consider cloud KMS or hardware HSM for KEK
3. **Monitor audit logs**: Alert on unusual access patterns
4. **Scope secrets tightly**: Prefer repo-scoped over org-scoped when possible
5. **Short weaver TTL**: Reduce window of exposure for compromised weavers

### 15.3 Operational Security

```bash
# Generate master key
openssl rand -base64 32 > /run/secrets/loom-master-key

# Generate SVID signing key (Ed25519)
openssl genpkey -algorithm Ed25519 -out /run/secrets/svid-signing-key.pem
```

---

## 16. Implementation Phases

### Phase 1: Core Infrastructure (2-3 days)

- [ ] Create `loom-server-secrets` crate
- [ ] Implement `KeyBackend` trait + `SoftwareKeyBackend`
- [ ] Implement envelope encryption (DEK generation, encrypt/decrypt)
- [ ] Add database migrations for secrets tables
- [ ] Basic secret CRUD operations

### Phase 2: Weaver Identity (2-3 days)

- [ ] Implement Weaver SVID issuance endpoint
- [ ] K8s TokenReview integration
- [ ] K8s Pod label extraction
- [ ] SVID signing with Ed25519
- [ ] SVID validation

### Phase 3: Weaver Secret Access (2-3 days)

- [ ] Implement weaver secrets API endpoints
- [ ] ABAC integration for secret access
- [ ] Scope resolution (org → repo → weaver)
- [ ] Secret version support

### Phase 4: User Management APIs (2 days)

- [ ] Org secret management endpoints
- [ ] Repo secret management endpoints
- [ ] Secret rotation endpoint
- [ ] Version listing

### Phase 5: Client Library (1-2 days)

- [ ] Create `loom-weaver-secrets` crate
- [ ] SVID acquisition and caching
- [ ] Secret fetching with auto-refresh
- [ ] Error handling

### Phase 6: SCM Integration (1 day)

- [ ] Migrate git credentials to weaver-scoped secrets
- [ ] Update weaver provisioner to create git credential secrets
- [ ] Update weaver code to fetch credentials via API

### Phase 7: Audit & Observability (1 day)

- [ ] Add audit events for secret lifecycle
- [ ] Add audit events for SVID issuance
- [ ] Add Prometheus metrics
- [ ] Health check integration

### Phase 8: Documentation & Testing (1-2 days)

- [ ] Integration tests
- [ ] Property-based tests for encryption
- [ ] Documentation and examples
- [ ] NixOS module configuration

---

## 17. Configuration Reference

### 17.1 Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_SECRETS_MASTER_KEY` | Yes* | Base64-encoded 256-bit master key |
| `LOOM_SECRETS_MASTER_KEY_FILE` | Yes* | Path to file containing master key |
| `LOOM_SECRETS_SVID_SIGNING_KEY` | No | Base64-encoded Ed25519 private key |
| `LOOM_SECRETS_SVID_SIGNING_KEY_FILE` | No | Path to SVID signing key file |
| `LOOM_SECRETS_SVID_TTL_SECONDS` | No | SVID TTL (default: 900) |
| `LOOM_SECRETS_VERIFY_POD_EXISTS` | No | Online Pod check (default: true) |
| `LOOM_SECRETS_SERVER_URL` | No | Server URL for weaver client (auto-detected in-cluster) |

*One of `LOOM_SECRETS_MASTER_KEY` or `LOOM_SECRETS_MASTER_KEY_FILE` is required.

### 17.2 NixOS Module

```nix
services.loom-server.secrets = {
  enable = true;
  masterKeyFile = "/run/secrets/loom-master-key";
  svidSigningKeyFile = "/run/secrets/svid-signing-key";
  svidTtlSeconds = 900;
  verifyPodExists = true;
};
```

---

## 18. Future Considerations

### 18.1 Potential Enhancements

| Feature | Description |
|---------|-------------|
| SPIRE integration | Replace custom SVID issuer with SPIRE for standardized workload identity |
| Cloud KMS backends | AWS KMS, GCP KMS, Azure Key Vault for KEK storage |
| HSM backend | PKCS#11 integration for hardware key storage |
| Automatic rotation | Background jobs rotating secrets with external providers |
| Secret templates | Dynamic secret generation (e.g., DB credentials from vault-like engines) |
| Multi-cluster | Replicate secrets across clusters for disaster recovery |
| X.509 SVIDs | Support mTLS between services using X.509 certificates |

### 18.2 Migration to SPIRE

If multi-cluster or heterogeneous workloads appear:

1. Deploy SPIRE server and agents
2. Configure SPIRE for K8s workload attestation
3. Update `KeyBackend` to validate SPIRE-issued SVIDs
4. Retire custom SVID issuance

---

## Appendix A: Rust Dependencies

```toml
[dependencies]
aes-gcm = "0.10"
argon2 = "0.5"
async-trait = "0.1"
base64 = "0.22"
chrono = { version = "0.4", features = ["serde"] }
ed25519-dalek = "2"
jsonwebtoken = "9"
rand = "0.8"
secrecy = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
uuid = { version = "1", features = ["v4", "v7", "serde"] }
zeroize = { version = "1", features = ["derive"] }

[dependencies.loom-secret]
path = "../loom-secret"

[dependencies.loom-k8s]
path = "../loom-k8s"

[dependencies.loom-server-auth]
path = "../loom-server-auth"
```

---

## Appendix B: Example Weaver Code

```rust
use loom_weaver_secrets::{SecretsClient, SecretScope};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize secrets client
    let secrets = SecretsClient::new()?;

    // Fetch API keys
    let stripe_key = secrets.get_secret(SecretScope::Org, "STRIPE_API_KEY").await?;
    let db_url = secrets.get_secret(SecretScope::Repo, "DATABASE_URL").await?;

    // Use secrets (they implement AsRef<str> but auto-redact in Debug)
    println!("Stripe configured: {:?}", stripe_key); // prints "[REDACTED]"

    let db_pool = sqlx::PgPool::connect(db_url.expose()).await?;

    // Do work...

    // Secrets are zeroized when dropped
    Ok(())
}
```

---

## Appendix C: Comparison Matrix

| Feature | Loom Weaver Secrets | GitHub Actions | HashiCorp Vault | K8s Secrets |
|---------|---------------------|----------------|-----------------|-------------|
| Workload identity | SPIFFE-style JWT | OIDC token | AppRole/K8s auth | ServiceAccount |
| Secret injection | Pull API | Env vars | Pull API / Agent | Volume mount |
| Encryption at rest | Envelope (AES-GCM) | Platform | Barrier + backends | etcd encryption |
| Scoping | Org/Repo/Weaver | Org/Repo/Env | Path-based policies | Namespace |
| Rotation | Versioned secrets | Manual | Lease renewal | Manual |
| Audit | Full lifecycle | Limited | Full | Audit logs |
| HSM support | Abstraction ready | Platform | Enterprise | KMS providers |
