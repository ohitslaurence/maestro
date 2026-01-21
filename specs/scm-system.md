<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# SCM (Source Code Management) System

Git hosting for Loom, enabling weavers to clone/push repositories and users to browse code via the web UI.

## Overview

- **Protocol**: HTTPS only (no SSH)
- **Implementation**: Pure Rust via [gitoxide](https://github.com/Byron/gitoxide) + git CLI for unsupported operations
- **Storage**: Local filesystem with UUID-based paths
- **Authentication**: Existing Loom auth, session tokens via credential helper

## Implementation Status

| Feature | Status | Crate |
|---------|--------|-------|
| Repository CRUD | ✅ Implemented | loom-server-scm, loom-server |
| Git HTTP Protocol | ✅ Implemented | loom-server (git CLI for protocol) |
| Branch Protection | ✅ Implemented | loom-server-scm |
| Webhooks | ✅ Implemented | loom-server-scm |
| Push Mirroring | ✅ Implemented | loom-scm-mirror |
| Pull Mirroring | ✅ Implemented | loom-scm-mirror |
| On-demand Mirroring | ✅ Implemented | loom-server |
| Team-based Access | ✅ Implemented | loom-server-scm |
| Web UI | ✅ Implemented | loom-web |
| Credential Helper | ✅ Implemented | loom-cli |

### Gitoxide Usage

| Operation | Implementation |
|-----------|---------------|
| Clone/Fetch | gitoxide (gix) |
| Merge-base | gitoxide (gix) |
| Push | git CLI (gix push not implemented) |
| upload-pack/receive-pack | git CLI (gix server protocol not supported) |
| gc/prune/fsck/repack | git CLI (gix maintenance not implemented) |

## Repository Ownership

Repos belong to either **users** or **organizations**.

```
repos table:
  id              UUID PRIMARY KEY
  owner_type      'user' | 'org'
  owner_id        UUID (references users or orgs)
  name            TEXT (unique per owner)
  visibility      'private' | 'public'
  default_branch  TEXT DEFAULT 'cannon'
  deleted_at      TIMESTAMP NULL (soft delete)
  created_at      TIMESTAMP
  updated_at      TIMESTAMP
```

### Visibility

- **Private** (default): Only users with explicit access can clone/view
- **Public**: Anyone can clone without authentication

## Filesystem Layout

Use UUIDs for paths to decouple from human-readable names (renames don't affect filesystem):

```
$LOOM_DATA_DIR/repos/
├── ab/                              # first 2 chars of UUID (sharding)
│   └── ab3f8c92-1234-5678-9abc-.../
│       └── git/                     # bare git repo
│           ├── HEAD
│           ├── config
│           ├── objects/
│           ├── refs/
│           └── hooks/
```

**Benefits:**
- Renames are instant (database-only)
- No special character escaping issues
- Sharding prevents too many entries in one directory

## URL Scheme

```
https://loom.ghuntley.com/git/{owner}/{repo}.git
```

Examples:
- `https://loom.ghuntley.com/git/ghuntley/my-project.git`
- `https://loom.ghuntley.com/git/acme-corp/backend.git`
- `https://loom.ghuntley.com/git/mirrors/github/torvalds/linux.git`

## Authentication

### Human Users: Credential Helper

Git credential helper in `loom-cli` that provides the user's session token:

```bash
# One-time setup
git config --global credential.https://loom.ghuntley.com.helper 'loom credential-helper'

# Git internally calls:
# echo "protocol=https\nhost=loom.ghuntley.com" | loom credential-helper get
# Returns username=oauth2, password={session_token}
```

The credential helper reads the stored session token from `loom login`.

### Weavers: Auto-Injected Credentials

When a weaver is launched, short-lived credentials are automatically injected:
- Scoped to the specific repo(s) the weaver needs
- Configured in weaver's git config automatically
- Expire when weaver terminates

## Access Control

### Repo-Level Roles

| Role | Permissions |
|------|-------------|
| `repo:read` | Clone, fetch, browse in web UI |
| `repo:write` | Push commits (to unprotected branches) |
| `repo:admin` | Manage settings, delete repo, manage collaborators, branch protection |

### Who Can Grant Access

- Repo admins
- Org owners
- Org admins

### Team-Based Access

Org teams can be granted repo access. All team members inherit the team's role.

```
repo_team_access table:
  repo_id    UUID
  team_id    UUID
  role       'read' | 'write' | 'admin'
```

## Branch Protection

Protect branches from direct pushes, force-pushes, and deletion.

```
branch_protection_rules table:
  id                    UUID PRIMARY KEY
  repo_id               UUID
  pattern               TEXT (e.g., 'cannon', 'release/*')
  block_direct_push     BOOLEAN DEFAULT true
  block_force_push      BOOLEAN DEFAULT true
  block_deletion        BOOLEAN DEFAULT true
  created_at            TIMESTAMP
```

When a push targets a protected branch:
1. Check if pusher has `repo:admin` role (admins can bypass)
2. If protected, reject with error message

## Repository Lifecycle

### Creation

- Via web UI or API only
- No push-to-create support
- Creates empty bare repo with default branch `cannon`

```
POST /api/repos
{
  "owner_type": "org",
  "owner_id": "uuid",
  "name": "my-repo",
  "visibility": "private"
}
```

### Soft Delete

Repos are soft-deleted (recoverable):

```
DELETE /api/repos/{id}
-> Sets deleted_at = NOW()
```

- Soft-deleted repos hidden from UI/API
- Scheduled job permanently deletes after N days
- Admin can restore before permanent deletion

## Mirroring

### Push Mirroring (Loom → External)

Mirror repo to external remotes (GitHub, GitLab, etc.).

```
repo_mirrors table:
  id              UUID PRIMARY KEY
  repo_id         UUID
  remote_url      TEXT
  credential_key  TEXT (key in loom-credentials, e.g., 'mirror:{repo_id}:{name}')
  enabled         BOOLEAN
  last_pushed_at  TIMESTAMP
  last_error      TEXT NULL
```

**Triggers:**
- On every push (via internal event)
- Periodic sync (scheduled job)

**Branch-specific triggers:**
```
mirror_branch_rules table:
  mirror_id   UUID
  pattern     TEXT (e.g., 'cannon', 'release/*')
  enabled     BOOLEAN
```

### Pull Mirroring (External → Loom)

Auto-mirror public GitHub/GitLab repos when a weaver is launched against them.

**Namespace:**
```
mirrors/github/{owner}/{repo}.git
mirrors/gitlab/{owner}/{repo}.git
```

**Behavior:**
- System-owned, read-only for all users
- Deduplicated (one mirror per external repo)
- Pull-only (no divergent pushes)
- Auto-cleanup if not accessed for 3 months (scheduled job)

```
external_mirrors table:
  id              UUID PRIMARY KEY
  platform        'github' | 'gitlab'
  external_owner  TEXT
  external_repo   TEXT
  repo_id         UUID (references repos)
  last_synced_at  TIMESTAMP
  last_accessed_at TIMESTAMP
  UNIQUE(platform, external_owner, external_repo)
```

**Sync jobs:**
- On weaver launch (if stale or missing)
- Periodic refresh (e.g., hourly for active mirrors)

### On-Demand Mirroring

When a client clones a mirror URL that doesn't exist:

1. Detect mirror path pattern (`mirrors/{platform}/{owner}/{repo}`)
2. Verify remote repository exists via API
3. Create repository and external_mirror entries
4. Clone from GitHub/GitLab using gitoxide
5. Serve the cloned data to the waiting client

```
GET /git/mirrors/github/torvalds/linux.git/info/refs
  → Mirror doesn't exist
  → Check github.com/torvalds/linux exists (API call)
  → Create repo in 'mirrors' org
  → Clone via gitoxide
  → Return refs to client
```

### Mirror Sync with Force-Push Recovery

When fetching updates:
1. Attempt `gix fetch`
2. If divergence detected (force-push upstream):
   - Delete local repository
   - Re-clone fresh
3. Update `last_synced_at`

### Mirror Cleanup Safety

Before deleting a stale mirror:
1. Check if remote still exists (GitHub/GitLab API)
2. If remote gone (404): delete local mirror
3. If remote exists: keep mirror, just mark stale
4. Log cleanup decisions for audit

## Webhooks

### Configuration

Per-repo or org-level webhooks.

```
webhooks table:
  id              UUID PRIMARY KEY
  owner_type      'repo' | 'org'
  owner_id        UUID
  url             TEXT
  secret          TEXT (for HMAC)
  payload_format  'github-compat' | 'loom-v1'
  events          TEXT[] (e.g., ['push', 'repo.created'])
  enabled         BOOLEAN
  created_at      TIMESTAMP
```

### Events

| Event | Description |
|-------|-------------|
| `push` | Commits pushed to a branch |
| `repo.created` | Repository created |
| `repo.deleted` | Repository deleted |

### Payload Formats

**GitHub-compatible (`github-compat`):**
```json
{
  "ref": "refs/heads/cannon",
  "before": "abc123...",
  "after": "def456...",
  "repository": {
    "id": 123,
    "name": "my-repo",
    "full_name": "org/my-repo",
    "clone_url": "https://loom.ghuntley.com/git/org/my-repo.git"
  },
  "pusher": { "name": "ghuntley", "email": "..." },
  "commits": [...]
}
```

**Loom format (`loom-v1`):**
```json
{
  "event": "push",
  "repo": { "uuid": "...", "owner": "org", "name": "my-repo" },
  "branch": "cannon",
  "before": "abc123",
  "after": "def456",
  "commits": [...],
  "actor": { "id": "...", "username": "ghuntley" }
}
```

### Security

- HMAC-SHA256 signature in `X-Loom-Signature-256` header
- Signature: `sha256=hex(HMAC(secret, body))`

### Delivery

- Retry on failure (3 attempts with exponential backoff) via job scheduler
- Delivery log stored for debugging:

```
webhook_deliveries table:
  id              UUID PRIMARY KEY
  webhook_id      UUID
  event           TEXT
  payload         JSONB
  response_code   INT NULL
  response_body   TEXT NULL
  delivered_at    TIMESTAMP NULL
  attempts        INT
  next_retry_at   TIMESTAMP NULL
  status          'pending' | 'success' | 'failed'
```

## Web UI

cgit-style interface for browsing repositories.

### Features

| Feature | Description |
|---------|-------------|
| File browser | Navigate directory tree at any ref |
| README rendering | GitHub-flavored markdown with emoji support |
| Commit history | Log view with pagination |
| Commit detail | Diff viewer for single commit |
| Blame | Line-by-line annotation |
| Branch comparison | Diff between two refs |
| Syntax highlighting | Language-aware code display |

### Markdown Rendering

README files are rendered below the file tree using GitHub-flavored markdown (GFM).

**Features:**
- Full GFM support via `marked` library
- GitHub emoji shortcode support (`:smile:` → 😄)
- Custom GitHub emojis rendered as images (`:octocat:`, `:shipit:`, etc.)
- Syntax highlighting for code blocks
- Tables, task lists, and other GFM extensions

**Implementation:**
- `MarkdownContent` component in `web/loom-web/src/lib/components/repos/`
- Uses `gemoji` package for Unicode emoji database (~1900 emojis)
- Custom GitHub emojis fetched from GitHub's CDN

### Routes

```
/repos/{owner}/{repo}                    # Repo home (file browser at HEAD)
/repos/{owner}/{repo}/tree/{ref}/{path}  # File browser
/repos/{owner}/{repo}/blob/{ref}/{path}  # File view
/repos/{owner}/{repo}/commits/{ref}      # Commit log
/repos/{owner}/{repo}/commit/{sha}       # Single commit
/repos/{owner}/{repo}/blame/{ref}/{path} # Blame view
/repos/{owner}/{repo}/compare/{a}...{b}  # Branch comparison
/repos/{owner}/{repo}/settings           # Repo settings (admin)
```

### Weaver Launch

"Open in Weaver" button in code browser:
- Creates weaver with repo pre-cloned
- Credentials auto-injected

## Git Maintenance Jobs

Scheduled jobs for repository health.

### Tasks

| Task | Command | Purpose |
|------|---------|---------|
| `gc` | `git gc` | Pack objects, remove loose objects |
| `prune` | `git prune` | Remove unreachable objects |
| `repack` | `git repack -a -d` | Optimize pack files |
| `fsck` | `git fsck` | Integrity check, alert on corruption |

### Scheduling

- **Per-repo**: Trigger ad-hoc or scheduled for specific repo
- **Global sweep**: Iterate all repos, run maintenance on each
- **Staggered execution**: Avoid IO spikes by spacing jobs
- **Off-peak preference**: Run during low-activity periods if configured

```
repo_maintenance_jobs table:
  id          UUID PRIMARY KEY
  repo_id     UUID NULL (NULL = global sweep)
  task        'gc' | 'prune' | 'repack' | 'fsck' | 'all'
  status      'pending' | 'running' | 'success' | 'failed'
  started_at  TIMESTAMP NULL
  finished_at TIMESTAMP NULL
  error       TEXT NULL
```

### Mirror Cleanup Job

Scheduled job to delete unused external mirrors:
- Query `external_mirrors WHERE last_accessed_at < NOW() - INTERVAL '3 months'`
- Delete repo from filesystem
- Remove database records

## Crate Structure

```
crates/
├── loom-server-scm/              # Core SCM logic
│   ├── types.rs           # Repository, RepoRole, Visibility
│   ├── repo.rs            # Repository CRUD, name validation
│   ├── git.rs             # gitoxide wrapper
│   ├── protection.rs      # Branch protection rules
│   ├── webhook.rs         # Webhook types and delivery
│   ├── maintenance.rs     # gc, prune, fsck jobs
│   └── schema.rs          # SQLite migrations
├── loom-scm-mirror/       # Mirroring logic
│   ├── types.rs           # PushMirror, ExternalMirror
│   ├── pull.rs            # Pull from GitHub/GitLab (gitoxide)
│   ├── push.rs            # Push to external (git CLI)
│   ├── cleanup.rs         # Stale mirror cleanup
│   └── store.rs           # Mirror stores
└── loom-cli/
    └── credential_helper.rs  # Git credential helper
```

## API Endpoints

### Repositories

```
POST   /api/repos                     # Create repo
GET    /api/repos/{id}                # Get repo
PATCH  /api/repos/{id}                # Update repo
DELETE /api/repos/{id}                # Soft delete repo
GET    /api/users/{id}/repos          # List user's repos
GET    /api/orgs/{id}/repos           # List org's repos
```

### Branch Protection

```
GET    /api/repos/{id}/protection     # List rules
POST   /api/repos/{id}/protection     # Create rule
DELETE /api/repos/{id}/protection/{rule_id}  # Delete rule
```

### Mirrors

```
GET    /api/repos/{id}/mirrors        # List push mirrors
POST   /api/repos/{id}/mirrors        # Create push mirror
DELETE /api/repos/{id}/mirrors/{mid}  # Delete push mirror
POST   /api/repos/{id}/mirrors/{mid}/sync  # Trigger sync
```

### Team Access

```
GET    /api/repos/{id}/teams                # List teams with access
POST   /api/repos/{id}/teams                # Grant team access
DELETE /api/repos/{id}/teams/{tid}          # Revoke team access
```

### Webhooks

```
GET    /api/repos/{id}/webhooks       # List repo webhooks
POST   /api/repos/{id}/webhooks       # Create webhook
DELETE /api/repos/{id}/webhooks/{wid} # Delete webhook
GET    /api/orgs/{id}/webhooks        # List org webhooks
POST   /api/orgs/{id}/webhooks        # Create org webhook
```

### Git HTTP

```
GET    /git/{owner}/{repo}.git/info/refs?service=git-upload-pack   # Clone/fetch
POST   /git/{owner}/{repo}.git/git-upload-pack                      # Clone/fetch
GET    /git/{owner}/{repo}.git/info/refs?service=git-receive-pack  # Push (auth required)
POST   /git/{owner}/{repo}.git/git-receive-pack                     # Push (auth required)
```

## Internal Events

On push, emit internal events for:
- Mirror sync (trigger push mirror job)
- Search index update
- Cache invalidation
- Webhook delivery

```rust
enum ScmEvent {
    Push { repo_id: Uuid, branch: String, before: String, after: String, commits: Vec<Commit> },
    RepoCreated { repo_id: Uuid },
    RepoDeleted { repo_id: Uuid },
}
```

## Security

### Repository Name Validation

Names must match: `^[a-zA-Z0-9][a-zA-Z0-9._-]*$`
- 1-100 characters
- Cannot start with `.` or `-`
- Cannot contain `..` (path traversal)
- Cannot contain `/`, `\`, or shell metacharacters

### Webhook Security

- Secrets stored as `SecretString` (auto-redacted in logs)
- HMAC-SHA256 signature verification
- SSRF protection: blocks localhost, private IPs, link-local, cloud metadata

### RBAC

Role hierarchy: `Admin >= Write >= Read`

Access resolution:
1. Check direct ownership (user owns repo)
2. Check org membership role (Owner/Admin → repo Admin)
3. Check team-based access (highest role from any team)
4. Return highest effective role

### Authorization Checks

| Endpoint | Required Role |
|----------|--------------|
| Clone (public) | None |
| Clone (private) | Read |
| Push | Write |
| Branch protection | Admin |
| Webhooks | Admin |
| Repo settings | Admin |
| Maintenance | Admin |
| Team access | Admin |

### Additional Security Measures

- All git operations over HTTPS with TLS
- Credentials never logged (use `loom-secret`)
- Rate limiting on git operations (future consideration)
- Audit log for admin actions (delete, protection changes)

## Dependencies

- [gitoxide](https://github.com/Byron/gitoxide) - Pure Rust git implementation
- `loom-auth` - Authentication
- `loom-credentials` - Credential storage for mirror remotes
- `loom-jobs` - Scheduled jobs (mirror sync, maintenance, cleanup)
- `loom-server` - HTTP endpoints
