<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Authentication & ABAC System Specification

**Status:** Implemented\
**Version:** 1.1\
**Last Updated:** 2025-01-02

### Implementation Notes

CLI authentication is implemented in `loom-cli/src/auth.rs` using:
- **Device code flow**: `POST /auth/device/start` and `POST /auth/device/poll`
- **Token storage**: `loom-credentials` crate with `KeyringThenFileStore` (keychain → file fallback)
- **Automatic token loading**: All CLI commands auto-load token from credential store

---

## 1. Overview

### Purpose

This specification defines the authentication and Attribute-Based Access Control (ABAC) system for Loom. It covers user identity, session management, organizational multi-tenancy, and fine-grained access control for all resources.

### Goals

- **Unified authentication** across CLI, web, and VS Code extension
- **Passwordless authentication** via OAuth and magic links
- **Multi-tenant organizations** with teams and role-based permissions
- **Fine-grained ABAC** for threads, workspaces, tools, and LLM access
- **Comprehensive audit logging** for security events
- **Secure session management** with device tracking and revocation

### Non-Goals

- Password-based authentication
- Billing/subscription management
- Rate limiting (deferred to future version)
- External policy DSL (Cedar, Casbin) — in-code policies for v1

---

## 2. Architecture

### Crate Structure

```
loom/
├── crates/
│   ├── loom-auth/               # Core auth types, ABAC engine
│   ├── loom-server-auth-magiclink/     # Magic link authentication
│   ├── loom-server-auth-devicecode/    # Device code flow for CLI
│   ├── loom-server-auth-github/ # GitHub OAuth provider
│   ├── loom-server-auth-google/  # Google OAuth provider
│   ├── loom-server-auth-okta/   # Okta OAuth/OIDC provider
│   ├── loom-smtp/               # Email sending for magic links
│   ├── loom-server/             # HTTP handlers, middleware
│   └── ...
```

### Defense in Depth ABAC

The ABAC system implements defense in depth with multiple authorization layers:

1. **Route-level middleware** (`RequireCapability`, `RequireRole`) - Coarse-grained checks applied at the routing layer to reject unauthorized requests early
2. **Handler-level `authorize!` macro** - Fine-grained resource-specific checks within handlers for context-aware authorization decisions
3. **Audit logging at both layers** - All authorization decisions (grants and denials) are logged for security monitoring and compliance

### Environment Variables

OAuth providers require the following environment variables:

| Provider | Variables |
|----------|-----------|
| GitHub | `LOOM_SERVER_GITHUB_CLIENT_ID`, `LOOM_SERVER_GITHUB_CLIENT_SECRET`, `LOOM_SERVER_GITHUB_REDIRECT_URI` |
| Google | `LOOM_SERVER_GOOGLE_CLIENT_ID`, `LOOM_SERVER_GOOGLE_CLIENT_SECRET`, `LOOM_SERVER_GOOGLE_REDIRECT_URI` |
| Okta | `LOOM_SERVER_OKTA_DOMAIN`, `LOOM_SERVER_OKTA_CLIENT_ID`, `LOOM_SERVER_OKTA_CLIENT_SECRET`, `LOOM_SERVER_OKTA_REDIRECT_URI` |

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Clients                                     │
├─────────────────────────────────────────────────────────────────────────┤
│    loom-cli          loom-web (Browser)         VS Code Extension       │
│    Bearer Token      Session Cookie              Bearer Token            │
└────────┬─────────────────────┬──────────────────────────┬───────────────┘
         │                     │                          │
         └─────────────────────┼──────────────────────────┘
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                           loom-server                                    │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐          │
│  │  Auth Routes    │  │ Auth Middleware │  │  ABAC Engine    │          │
│  │  /auth/*    │  │ Session/Token   │  │  Policy Eval    │          │
│  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘          │
│           │                    │                    │                    │
│           └────────────────────┼────────────────────┘                    │
│                                ▼                                         │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │                    Protected Resources                            │   │
│  │   Threads    Workspaces    LLM Proxy    Tools    Organizations   │   │
│  │   Weavers                                                         │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Database (SQLite)                                │
├─────────────────────────────────────────────────────────────────────────┤
│  users    identities    sessions    access_tokens    api_keys           │
│  organizations    org_memberships    teams    team_memberships          │
│  threads    audit_logs    support_access    share_links                 │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Identity Providers

### Supported Methods

| Provider | Type | Description |
|----------|------|-------------|
| GitHub OAuth | OAuth 2.0 | Login with GitHub account |
| Google OAuth | OAuth 2.0 | Login with Google account |
| Okta | OAuth 2.0 / OIDC | Enterprise SSO via Okta |
| Email Magic Link | Passwordless | 10-minute single-use link sent to email |

### OAuth Flow

```
User clicks "Login with GitHub"
  → Redirect to GitHub authorization
  → User approves
  → Callback to /auth/callback/github
  → Exchange code for tokens
  → Fetch user info (email, name, avatar)
  → Upsert user + identity
  → Create session
  → Set cookie (web) or return token (CLI)
```

### Magic Link Flow

```
User enters email
  → POST /auth/magic-link
  → Generate single-use token (stored hashed)
  → Send email with link: /auth/magic-link/verify?token=xxx
  → User clicks link within 10 minutes
  → Verify token, invalidate it
  → Upsert user
  → Create session
```

### Magic Link Rules

| Rule | Value |
|------|-------|
| Expiry | 10 minutes |
| Usage | Single-use (invalidated after click) |
| Concurrent requests | New link invalidates previous |

---

## 4. Account Linking & Email Handling

### Auto-Linking

Accounts with the same email are automatically linked:

```
Alice logs in via GitHub (alice@example.com)
  → User created

Later, Alice uses magic link with alice@example.com
  → Same user, new identity added
```

### Email Conflict

If an OAuth provider returns an email that belongs to a different account:

```
Response: "Email already in use by another account"
Action: Block login
```

### Email Verification

| Scenario | Behavior |
|----------|----------|
| OAuth provider says `verified: true` | Trust it, full access |
| OAuth provider says `verified: false` | Require email verification |
| Magic link | Inherently verified (clicked the link) |

### Primary Email

- Users may have multiple identities with different emails
- User chooses primary email in settings
- All notifications sent to primary email only
- Default email visibility: visible (user can hide)

---

## 5. Session Management

### Session Types

| Type | Mechanism | Lifetime |
|------|-----------|----------|
| Web session | HttpOnly, Secure, SameSite=Lax cookie | 60-day sliding |
| CLI token | Bearer token in Authorization header | 60-day sliding |
| VS Code token | Bearer token in Authorization header | 60-day sliding |

### Sliding Expiry

- Session/token expires 60 days after **last use**
- Each request extends the expiry
- Inactive for 60 days → must re-authenticate

### Session Metadata

Each session stores:

| Field | Description |
|-------|-------------|
| `id` | Unique session identifier |
| `user_id` | Owner of the session |
| `session_type` | `web`, `cli`, `vscode` |
| `created_at` | Creation timestamp |
| `last_used_at` | Last activity timestamp |
| `ip_address` | Client IP |
| `user_agent` | Browser/client info |
| `geo_location` | City/country from IP (MaxMind GeoLite2) |

### Session Limits

- **Unlimited** concurrent sessions allowed
- Users can view full session list with metadata
- Users can revoke individual sessions

---

## 6. CLI Authentication (Device Code Flow)

### Flow

```
1. CLI: POST /auth/device/start
   → Response: { device_code, user_code: "123-456-789", verification_url }

2. CLI displays: "Visit https://loom.example/device and enter code: 123-456-789"

3. User visits URL in browser (/device?code=XXX)
   → If not authenticated, redirected to /login?redirectTo=/device?code=XXX
   → After login, user is returned to /device with the code preserved
   → User submits code via POST /auth/device/complete (requires auth)

4. CLI polls: POST /auth/device/poll { device_code }
   → Polling interval: 1 second
   → On success: { access_token }
   → On pending: { status: "pending" }
   → On expired: { status: "expired" }

5. CLI stores token in system keychain (fallback: ~/.config/loom/credentials.json)
```

### Device Code Parameters

| Parameter | Value |
|-----------|-------|
| Code format | 9 digits: `123-456-789` |
| Expiry | 10 minutes |
| Polling interval | 1 second |

### Multi-Server Support

- CLI stores token per server URL
- First login sets default server
- Command to change default: `loom config set-default-server <url>`
- Override with flag: `--server <url>`

### Token Storage

1. Try system keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
2. Fallback to config file: `~/.config/loom/credentials.json`

**Implementation:** `loom-credentials` crate provides:
- `KeyringCredentialStore`: Platform keychain via `keyring` crate
- `FileCredentialStore`: JSON file with 0600 permissions
- `KeyringThenFileStore`: Composite store that tries keychain first, falls back to file

Credentials are stored as JSON-serialized `CredentialValue` (ApiKey or OAuth tokens) keyed by sanitized server URL.

---

## 7. WebSocket/SSE Authentication

### Dual Authentication Support

| Client | Mechanism |
|--------|-----------|
| Web browser | Session cookie (automatic with handshake) |
| CLI / VS Code | First-message auth after connection |

### First-Message Auth Flow

```
1. CLI connects: wss://loom.example/ws (unauthenticated)
2. Server starts 30-second timeout
3. CLI sends: { "type": "auth", "token": "bearer_xyz789" }
4. Server validates token
   → Success: connection authenticated
   → Failure/timeout: disconnect
```

### Timeout

- 30 seconds to send auth message
- Connection closed if auth not received

---

## 8. Organizations

### Model

Every thread belongs to an organization. Each user automatically gets a "Personal" org.

```
User: alice@example.com
├── Org: "Alice's Personal" (auto-created)
│   └── Threads...
└── Org: "Acme Corp" (shared)
    ├── Team: "Backend"
    ├── Team: "Frontend"
    └── Threads...
```

### Org Roles

| Role | Permissions |
|------|-------------|
| `owner` | Full control, delete org, transfer ownership, manage API keys |
| `admin` | Manage members, manage API keys, delete any thread |
| `member` | Read org threads, create/edit/delete own threads |

### Ownership Rules

- Multiple owners allowed (equal power)
- Cannot demote self if it would leave zero owners
- Must always have at least 1 owner
- Owner can transfer ownership to any member

### Org Visibility

| Setting | Behavior |
|---------|----------|
| `public` | Visible in org directory, anyone can request to join |
| `unlisted` | Not in directory, can request if they know org name |
| `private` | Invisible, only direct add works |

**Default:** Public (including personal orgs)

### Org Deletion

- Owner + admins can delete (with confirmation — type org name)
- 90-day soft-delete grace period
- Self-service restore during grace period
- After 90 days: hard delete

---

## 9. Teams

### Model

Teams are sub-groups within an organization.

```
Org: "Acme Corp"
├── Team: "Backend"
│   ├── alice (maintainer)
│   └── bob (member)
└── Team: "Frontend"
    └── carol (maintainer)
```

### Team Roles

| Role | Permissions |
|------|-------------|
| `maintainer` | Add/remove team members, manage team settings |
| `member` | Access team resources |

---

## 10. Org Membership

### Joining an Org

| Method | Flow |
|--------|------|
| Direct add | Admin adds user by email → verification email sent → user clicks to join |
| Request to join | User finds org → requests access → admin approves |

### Direct Add

- Admin enters user's email
- User receives verification email
- User must click to confirm and gain access
- Invitation expires after **30 days**

---

## 11. Thread Visibility

| Level | Who Can Access |
|-------|----------------|
| `private` | Owner only |
| `team` | Team members (if thread assigned to a team) |
| `organization` | All org members |
| `public` | Anyone (including anonymous) |

### Anonymous Access

- Public threads: **view only** (read thread content)
- Must login to: send messages, use LLM, execute tools

### Thread Ownership

- Threads cannot be transferred to another owner
- On owner account deletion: ownership → tombstone user (per-user, auditable)
- Org admins can manage orphaned threads

### Thread Forking

- Can fork/copy threads within same org only
- New thread owned by the copier

---

## 12. External Sharing (Read-Only Links)

### Shareable Links

- Generate read-only link for external users (not logged in or different org)
- Format: `/threads/T-{id}/share/{token}` where token is 48 hex chars (24 bytes)

### Link Rules

| Rule | Value |
|------|-------|
| Links per thread | 1 (regenerate replaces old) |
| Expiry | Owner chooses (or never) |
| Revocation | Explicit revoke button |
| Password protection | None (link is the secret) |
| Access level | Read-only |

---

## 13. Support Access

### Flow

1. Support requests access to a specific thread
2. User receives notification
3. User approves the request
4. Support can access thread for **31 days**
5. Access auto-expires after 31 days

### Tracking

- `is_shared_with_support` flag on thread
- `support_access` table with: thread_id, approved_at, expires_at, approved_by

---

## 14. API Keys

### Scope

- **Org-level only** (no user-level API keys)
- Created by org owners or admins

### Format

```
lk_7a3b9f2e1c4d8a5b6e0f3c2d1a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b
└──┘└────────────────────────────────────────────────────────────────────┘
prefix                    64 hex chars (32 bytes / 256 bits)
```

### Action-Based Scopes

| Scope | Allows |
|-------|--------|
| `threads:read` | Read threads |
| `threads:write` | Create/edit threads |
| `threads:delete` | Delete threads |
| `llm:use` | Make LLM requests |
| `tools:use` | Execute tools |

### Management

- Owner + admins can view/create/revoke
- Required: name (e.g., "CI Pipeline")
- Show once on creation, require copy before dismissing
- Full usage logging: timestamp, IP, endpoint

### Storage

- Token stored **hashed** (Argon2)
- Only shown once at creation

---

## 15. Global Roles

| Role | Permissions |
|------|-------------|
| `system_admin` | Full platform control, impersonate users, promote others |
| `support` | Access threads shared with support |
| `auditor` | Full read-only access (audit logs, users, orgs, threads) |

### System Admin Assignment

- First registered user becomes system_admin (bootstrap)
- Existing admins can promote other users
- Cannot demote self
- Must always have at least 1 system_admin

### Impersonation

- System admins can impersonate any user
- Every action logged as "admin X as user Y"
- User is **not notified** (audit log only)

---

## 16. ABAC Policy Model

### Subject Attributes

```rust
struct SubjectAttrs {
    user_id: UserId,
    org_memberships: Vec<OrgMembership>,  // { org_id, role }
    team_memberships: Vec<TeamMembership>, // { team_id, role }
    global_roles: Vec<GlobalRole>,         // system_admin, support, auditor
}
```

### Resource Attributes

```rust
struct ResourceAttrs {
    resource_type: ResourceType,  // Thread, Workspace, Tool, Organization, Team, User, ApiKey, Llm, Weaver
    owner_user_id: Option<UserId>,
    org_id: Option<OrgId>,
    team_id: Option<TeamId>,
    visibility: Visibility,       // Private, Team, Organization, Public
    is_shared_with_support: bool,
}
```

### Actions

```rust
enum Action {
    Read,
    Write,
    Delete,
    Share,
    UseTool,
    UseLlm,
    ManageOrg,
    ManageApiKeys,
    ManageTeam,   // Manage team settings and members
    Impersonate,  // Impersonate another user (system_admin only)
}
```

### Policy Engine

```rust
fn is_allowed(subject: &SubjectAttrs, action: Action, resource: &ResourceAttrs) -> bool {
    // Check global roles first (auditor, system_admin)
    // Then resource-specific policies
    match resource.resource_type {
        ResourceType::Thread => thread_policies(subject, action, resource),
        ResourceType::Org => org_policies(subject, action, resource),
        // ...
    }
}
```

### Thread Policies (Examples)

**Read:**
- Owner always allowed
- `visibility = Public` → anyone (including anonymous)
- `visibility = Organization` → org members
- `visibility = Team` → team members
- `is_shared_with_support` → users with `support` role
- `auditor` role → allowed

**Write/Delete:**
- Owner allowed
- Org admin allowed (for org threads)

**Share:**
- Owner allowed

### Weaver Policies

Weavers are user-owned compute resources that run agent sessions.

**Access Model:**
- System admins have full access to all weavers
- Owners have full access to their own weavers
- Support users have read-only access to all weavers (for debugging/assistance)
- Other users have no access to weavers they don't own

**Permissions Matrix:**

| Action | Owner | Support | System Admin |
|--------|-------|---------|--------------|
| Create | ✓ | ✓ | ✓ |
| List (own) | ✓ | ✓ (all) | ✓ (all) |
| Get | ✓ | ✓ | ✓ |
| View Logs | ✓ | ✓ | ✓ |
| Attach | ✓ | ✓ (read-only) | ✓ |
| Delete | ✓ | ✗ | ✓ |

**Policy Details:**

**Create:**
- Any authenticated user allowed

**List:**
- Owner can list their own weavers
- `support` role can list all weavers (for debugging)
- `system_admin` role can list all weavers

**Get/View Logs:**
- Owner allowed
- `support` role allowed (read-only access for debugging)
- `system_admin` role allowed

**Attach:**
- Owner allowed (full read/write)
- `support` role allowed (read-only mode for debugging)
- `system_admin` role allowed (full read/write)

**Delete:**
- Owner allowed
- `system_admin` role allowed
- `support` role **not** allowed (read-only)

**Cleanup (admin operation):**
- `system_admin` role only

### Tool/LLM Access

- **All or nothing:** if user can access LLM, they can use all tools
- All org members have LLM access

---

## 17. Audit Logging

### Events Logged

| Category | Events |
|----------|--------|
| Auth | Login, logout, failed login attempts |
| Session | Token created, revoked, expired |
| API Key | Created, used, revoked |
| Access | Thread accessed, permission denied |
| Admin | Member added/removed, role changed, impersonation |

### Retention

- **90 days** retention
- Auto-purge older logs

### Fields

```rust
struct AuditLogEntry {
    id: Uuid,
    timestamp: DateTime<Utc>,
    event_type: String,
    actor_user_id: Option<UserId>,
    impersonating_user_id: Option<UserId>,  // If impersonating
    resource_type: Option<String>,
    resource_id: Option<String>,
    action: String,
    ip_address: Option<String>,
    user_agent: Option<String>,
    details: JsonValue,  // Event-specific data
}
```

---

## 18. Security Notifications

### Events (Always On)

| Event | Notification |
|-------|--------------|
| New login from new device/location | Email |
| API key created | Email |
| API key revoked | Email |
| Added to org | Email |
| Removed from org | Email |
| Role changed | Email |

- Cannot be disabled
- Sent to primary email only

---

## 19. User Deletion

### Flow

1. User requests account deletion
2. Account deactivated (cannot login)
3. Personal org + threads marked deleted
4. Org threads: ownership → per-user tombstone
5. **90-day grace period**
6. During grace: user can self-service restore (login prompts reactivation)
7. After 90 days: hard purge of personal data

### Tombstone User

When a user deletes their account, a tombstone record preserves attribution:

```
owner_user_id: deleted-user-{original-uuid}
```

- Cannot login
- Preserves audit trail ("this was Alice's thread")
- Org admins manage orphaned threads

---

## 20. CSRF Protection

- **SameSite=Lax** cookies
- **CSRF tokens** for state-changing requests (belt + suspenders)

---

## 21. CORS Policy

- Specific allowed origins via environment variable
- `LOOM_SERVER_CORS_ORIGINS=https://app.loom.example,http://localhost:5173`

---

## 22. Email Configuration

### SMTP Settings

| Variable | Description |
|----------|-------------|
| `LOOM_SERVER_SMTP_HOST` | SMTP server hostname |
| `LOOM_SERVER_SMTP_PORT` | SMTP port (typically 587) |
| `LOOM_SERVER_SMTP_USERNAME` | SMTP username |
| `LOOM_SERVER_SMTP_PASSWORD` | SMTP password (via loom-secret) |
| `LOOM_SERVER_SMTP_FROM` | From address (e.g., noreply@loom.example) |
| `LOOM_SERVER_SMTP_TLS` | `true`, `starttls`, or `false` |

---

## 23. User Profiles

### Data Sources

| Field | Source |
|-------|--------|
| Display name | OAuth provider (editable) |
| Avatar | OAuth provider (Gravatar for magic link users) |
| Email | OAuth provider or magic link |

### Profile Visibility

- Any logged-in user can view profiles
- Visible fields: name, avatar, join date, public orgs, public threads
- Email visibility: user-controlled (visible by default)

---

## 24. Database Schema

### Users & Identity

```sql
-- users
CREATE TABLE users (
    id TEXT PRIMARY KEY,  -- UUID
    display_name TEXT NOT NULL,
    primary_email TEXT UNIQUE,
    avatar_url TEXT,
    email_visible BOOLEAN DEFAULT TRUE,
    is_system_admin BOOLEAN DEFAULT FALSE,
    is_support BOOLEAN DEFAULT FALSE,
    is_auditor BOOLEAN DEFAULT FALSE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT  -- Soft delete
);

-- identities (OAuth providers)
CREATE TABLE identities (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    provider TEXT NOT NULL,  -- 'github', 'google', 'magic_link'
    provider_user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    email_verified BOOLEAN DEFAULT FALSE,
    access_token TEXT,  -- Encrypted if stored
    refresh_token TEXT,
    token_expires_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(provider, provider_user_id)
);
```

### Sessions & Tokens

```sql
-- sessions (web)
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,  -- Random token
    user_id TEXT NOT NULL REFERENCES users(id),
    session_type TEXT NOT NULL,  -- 'web', 'cli', 'vscode'
    created_at TEXT NOT NULL,
    last_used_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    geo_city TEXT,
    geo_country TEXT
);

-- access_tokens (CLI/VS Code)
CREATE TABLE access_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    token_hash TEXT NOT NULL,  -- Argon2 hash
    label TEXT NOT NULL,
    session_type TEXT NOT NULL,  -- 'cli', 'vscode'
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    expires_at TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    geo_city TEXT,
    geo_country TEXT,
    revoked_at TEXT
);

-- device_codes (for device code flow)
CREATE TABLE device_codes (
    device_code TEXT PRIMARY KEY,
    user_code TEXT NOT NULL UNIQUE,  -- '123-456-789'
    user_id TEXT REFERENCES users(id),  -- Set when user completes auth
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    completed_at TEXT
);

-- magic_links
CREATE TABLE magic_links (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);
```

### Organizations

```sql
-- organizations
CREATE TABLE organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'public',  -- 'public', 'unlisted', 'private'
    is_personal BOOLEAN DEFAULT FALSE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

-- org_memberships
CREATE TABLE org_memberships (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL,  -- 'owner', 'admin', 'member'
    created_at TEXT NOT NULL,
    UNIQUE(org_id, user_id)
);

-- org_invitations
CREATE TABLE org_invitations (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    email TEXT NOT NULL,
    role TEXT NOT NULL,
    invited_by TEXT NOT NULL REFERENCES users(id),
    token_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT
);

-- org_join_requests
CREATE TABLE org_join_requests (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    handled_at TEXT,
    handled_by TEXT REFERENCES users(id),
    approved BOOLEAN
);
```

### Teams

```sql
-- teams
CREATE TABLE teams (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(org_id, slug)
);

-- team_memberships
CREATE TABLE team_memberships (
    id TEXT PRIMARY KEY,
    team_id TEXT NOT NULL REFERENCES teams(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL,  -- 'maintainer', 'member'
    created_at TEXT NOT NULL,
    UNIQUE(team_id, user_id)
);
```

### API Keys

```sql
-- api_keys
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id),
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,  -- Argon2 hash
    scopes TEXT NOT NULL,  -- JSON array: ["threads:read", "llm:use"]
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT,
    revoked_by TEXT REFERENCES users(id)
);

-- api_key_usage
CREATE TABLE api_key_usage (
    id TEXT PRIMARY KEY,
    api_key_id TEXT NOT NULL REFERENCES api_keys(id),
    timestamp TEXT NOT NULL,
    ip_address TEXT,
    endpoint TEXT,
    method TEXT
);
```

### Thread Extensions

```sql
-- Add to threads table
ALTER TABLE threads ADD COLUMN owner_user_id TEXT REFERENCES users(id);
ALTER TABLE threads ADD COLUMN org_id TEXT REFERENCES organizations(id);
ALTER TABLE threads ADD COLUMN team_id TEXT REFERENCES teams(id);
ALTER TABLE threads ADD COLUMN visibility TEXT DEFAULT 'private';
ALTER TABLE threads ADD COLUMN is_shared_with_support BOOLEAN DEFAULT FALSE;

-- share_links
CREATE TABLE share_links (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    token_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    expires_at TEXT,  -- NULL = never
    revoked_at TEXT,
    UNIQUE(thread_id)  -- One link per thread
);

-- support_access
CREATE TABLE support_access (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id),
    requested_by TEXT NOT NULL REFERENCES users(id),  -- Support user
    approved_by TEXT REFERENCES users(id),            -- Thread owner
    requested_at TEXT NOT NULL,
    approved_at TEXT,
    expires_at TEXT,  -- 31 days after approval
    revoked_at TEXT
);
```

### Audit Logs

```sql
-- audit_logs
CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    event_type TEXT NOT NULL,
    actor_user_id TEXT REFERENCES users(id),
    impersonating_user_id TEXT REFERENCES users(id),
    resource_type TEXT,
    resource_id TEXT,
    action TEXT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    details TEXT,  -- JSON
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX idx_audit_logs_actor ON audit_logs(actor_user_id);
CREATE INDEX idx_audit_logs_event_type ON audit_logs(event_type);
```

---

## 25. Environment Variables

### Authentication

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_SERVER_GITHUB_CLIENT_ID` | For GitHub OAuth | GitHub OAuth app client ID |
| `LOOM_SERVER_GITHUB_CLIENT_SECRET` | For GitHub OAuth | GitHub OAuth app client secret |
| `LOOM_SERVER_GOOGLE_CLIENT_ID` | For Google OAuth | Google OAuth client ID |
| `LOOM_SERVER_GOOGLE_CLIENT_SECRET` | For Google OAuth | Google OAuth client secret |

### Email

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_SERVER_SMTP_HOST` | For magic link | SMTP server hostname |
| `LOOM_SERVER_SMTP_PORT` | For magic link | SMTP port |
| `LOOM_SMTP_USERNAME` | For magic link | SMTP username |
| `LOOM_SMTP_PASSWORD` | For magic link | SMTP password |
| `LOOM_SMTP_FROM` | For magic link | From email address |
| `LOOM_SMTP_TLS` | No | TLS mode: `true`, `starttls`, `false` |

### Security

| Variable | Required | Description |
|----------|----------|-------------|
| `LOOM_CORS_ORIGINS` | Yes | Comma-separated allowed origins |
| `LOOM_SESSION_SECRET` | Yes | Secret for signing session cookies |

---

## 26. API Endpoints

### Authentication

| Method | Path | Description |
|--------|------|-------------|
| GET | `/auth/providers` | List available auth providers |
| GET | `/auth/login/github` | Initiate GitHub OAuth |
| GET | `/auth/login/google` | Initiate Google OAuth |
| GET | `/auth/callback/github` | GitHub OAuth callback |
| GET | `/auth/callback/google` | Google OAuth callback |
| POST | `/auth/magic-link` | Request magic link |
| GET | `/auth/magic-link/verify` | Verify magic link |
| POST | `/auth/device/start` | Start device code flow |
| POST | `/auth/device/poll` | Poll device code status |
| POST | `/auth/device/complete` | Complete device code (requires auth) |
| GET | `/device` | Device code entry page (requires auth, redirects to login) |
| POST | `/auth/logout` | Logout (invalidate session) |
| GET | `/auth/me` | Get current user |

### Sessions

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/sessions` | List user's sessions |
| DELETE | `/api/sessions/{id}` | Revoke a session |

### Organizations

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/orgs` | List orgs (directory or user's) |
| POST | `/api/orgs` | Create org |
| GET | `/api/orgs/{id}` | Get org details |
| PATCH | `/api/orgs/{id}` | Update org |
| DELETE | `/api/orgs/{id}` | Delete org (soft) |
| POST | `/api/orgs/{id}/restore` | Restore deleted org |
| GET | `/api/orgs/{id}/members` | List members |
| POST | `/api/orgs/{id}/members` | Add member (direct add) |
| DELETE | `/api/orgs/{id}/members/{user_id}` | Remove member |
| PATCH | `/api/orgs/{id}/members/{user_id}` | Change role |
| POST | `/api/orgs/{id}/join-requests` | Request to join |
| GET | `/api/orgs/{id}/join-requests` | List join requests (admin) |
| POST | `/api/orgs/{id}/join-requests/{id}/approve` | Approve request |
| POST | `/api/orgs/{id}/join-requests/{id}/reject` | Reject request |

### Teams

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/orgs/{org_id}/teams` | List teams |
| POST | `/api/orgs/{org_id}/teams` | Create team |
| GET | `/api/orgs/{org_id}/teams/{id}` | Get team |
| PATCH | `/api/orgs/{org_id}/teams/{id}` | Update team |
| DELETE | `/api/orgs/{org_id}/teams/{id}` | Delete team |
| GET | `/api/orgs/{org_id}/teams/{id}/members` | List team members |
| POST | `/api/orgs/{org_id}/teams/{id}/members` | Add team member |
| DELETE | `/api/orgs/{org_id}/teams/{id}/members/{user_id}` | Remove member |

### API Keys

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/orgs/{org_id}/api-keys` | List org API keys |
| POST | `/api/orgs/{org_id}/api-keys` | Create API key |
| DELETE | `/api/orgs/{org_id}/api-keys/{id}` | Revoke API key |
| GET | `/api/orgs/{org_id}/api-keys/{id}/usage` | Get usage log |

### User Profile

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/users/{id}` | Get user profile |
| PATCH | `/api/users/me` | Update own profile |
| GET | `/api/users/me/identities` | List linked identities |
| DELETE | `/api/users/me/identities/{id}` | Unlink identity |
| POST | `/api/users/me/delete` | Request account deletion |
| POST | `/api/users/me/restore` | Restore deleted account |

### Invitations

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/invitations/{token}` | Get invitation details |
| POST | `/api/invitations/{token}/accept` | Accept invitation |

### Share Links

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/threads/{id}/share` | Create share link |
| DELETE | `/api/threads/{id}/share` | Revoke share link |
| GET | `/api/threads/{id}/share/{token}` | Access shared thread (public) |

### Support Access

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/threads/{id}/support-access/request` | Request support access |
| POST | `/api/threads/{id}/support-access/approve` | Approve request |
| DELETE | `/api/threads/{id}/support-access` | Revoke support access |

### Admin

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/admin/users` | List all users |
| POST | `/api/admin/users/{id}/impersonate` | Start impersonation |
| POST | `/api/admin/impersonate/stop` | Stop impersonation |
| GET | `/api/admin/audit-logs` | Query audit logs |
| PATCH | `/api/admin/users/{id}/roles` | Update global roles |

---

## 27. Rust Crates

### Dependencies

| Crate | Purpose |
|-------|---------|
| `oauth2` | OAuth 2.0 flows |
| `argon2` | Password/token hashing |
| `tower-cookies` | Cookie management |
| `rand` | Secure random generation |
| `uuid` | UUID generation |
| `maxminddb` | GeoIP lookup |
| `lettre` | SMTP email sending |
| `time` / `chrono` | Timestamp handling |

### loom-auth Structure

```
crates/loom-auth/
├── src/
│   ├── lib.rs
│   ├── types.rs          # User, Session, Token types
│   ├── oauth/
│   │   ├── mod.rs
│   │   ├── github.rs
│   │   └── google.rs
│   ├── magic_link.rs
│   ├── device_code.rs
│   ├── session.rs        # Session management
│   ├── token.rs          # Access token management
│   ├── api_key.rs        # API key management
│   ├── abac/
│   │   ├── mod.rs
│   │   ├── types.rs      # SubjectAttrs, ResourceAttrs, Action
│   │   ├── engine.rs     # is_allowed()
│   │   └── policies/
│   │       ├── thread.rs
│   │       ├── org.rs
│   │       └── ...
│   ├── audit.rs          # Audit logging
│   └── email.rs          # SMTP client
└── Cargo.toml
```

---

## 28. Implementation Phases

### Phase 0: Foundation (1-2 days)

- [ ] Create `loom-auth` crate structure
- [ ] Add database migrations for users, sessions, identities
- [ ] Implement dev mode (auto-create dev user when `LOOM_AUTH_DEV_MODE=1`)
- [ ] Add auth middleware skeleton

### Phase 1: Basic Web Auth (3-4 days)

- [ ] GitHub OAuth login
- [ ] Google OAuth login
- [ ] Session cookie management
- [ ] `/auth/me` endpoint
- [ ] `/auth/logout` endpoint
- [ ] Auto-create personal org on signup

### Phase 2: Magic Link (2-3 days)

- [ ] SMTP email client
- [ ] Magic link generation and verification
- [ ] Account linking (same email = same user)

### Phase 3: CLI Auth (2-3 days)

- [ ] Device code flow endpoints
- [ ] Device verification web page
- [ ] CLI token storage (keychain + fallback)
- [ ] Bearer token middleware

### Phase 4: Organizations (3-4 days)

- [ ] Org CRUD endpoints
- [ ] Membership management
- [ ] Role-based permissions
- [ ] Join requests

### Phase 5: Teams (2 days)

- [ ] Team CRUD endpoints
- [ ] Team membership
- [ ] Team-based thread visibility

### Phase 6: ABAC (3-4 days)

- [ ] Subject/Resource attribute loading
- [ ] Policy engine implementation
- [ ] Apply to all protected endpoints
- [ ] Thread visibility enforcement

### Phase 7: API Keys (2-3 days)

- [ ] API key generation and hashing
- [ ] Scopes enforcement
- [ ] Usage logging

### Phase 8: Audit & Security (2-3 days)

- [ ] Audit log infrastructure
- [ ] CSRF token generation/validation
- [ ] Security notifications
- [ ] Session metadata (GeoIP)

### Phase 9: Admin Features (2 days)

- [ ] System admin bootstrap
- [ ] Admin promotion/demotion
- [ ] Impersonation with audit

### Phase 10: Sharing & Support (2 days)

- [ ] Share link generation
- [ ] Support access workflow
- [ ] External read-only access

---

## Appendix A: Security Considerations

### Token Security

- All tokens (session, access, API key, magic link, share) stored as Argon2 hashes
- Tokens only shown once at creation
- Use `loom-secret` for all secret handling

### Cookie Security

- `HttpOnly` — not accessible via JavaScript
- `Secure` — only sent over HTTPS
- `SameSite=Lax` — CSRF protection
- Additional CSRF token for state-changing requests

### Session Security

- 60-day sliding expiry
- Full session visibility with revocation
- GeoIP tracking for anomaly detection
- Security notifications on new device/location

### API Key Security

- Org-level only (not user-level)
- Action-based scopes (least privilege)
- Full usage logging
- Explicit revocation

---

## Appendix B: Future Considerations

| Feature | Description |
|---------|-------------|
| Rate limiting | Per-user, per-org, per-IP limits |
| SSO/SAML | Enterprise SSO integration |
| 2FA/MFA | Time-based OTP or WebAuthn |
| Password auth | Traditional email/password (if needed) |
| External policy DSL | Cedar or Casbin for complex policies |
| Webhook notifications | Push security events to external systems |
