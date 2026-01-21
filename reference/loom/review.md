# SCM System Implementation Review

Review of the SCM implementation against the specification at `specs/scm-system.md`.

## Summary

| Category | Implemented | Partial | Missing |
|----------|-------------|---------|---------|
| Core Features | 7 | 2 | 1 |
| Total | 7 | 2 | 1 |

---

## 1. Repository Ownership ✅ Fully Implemented

- **Repos can belong to users or orgs**: ✅ Implemented via `OwnerType::User | Org` in [types.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/types.rs)
- **Visibility (private/public)**: ✅ `Visibility::Private | Public` with private as default
- **UUID-based filesystem layout with sharding**: ✅ Implemented in [git.rs routes](file:///home/ghuntley/loom/crates/loom-server/src/routes/git.rs#L90-L97) using `{shard}/{uuid}/git/` pattern
- **Database schema**: ✅ Matches spec exactly in [schema.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/schema.rs)

---

## 2. URL Scheme ✅ Fully Implemented

- **Clone URL format**: ✅ `https://loom.ghuntley.com/git/{owner}/{repo}.git`
- Routes in [git.rs](file:///home/ghuntley/loom/crates/loom-server/src/routes/git.rs#L493-L497):
  - `GET /git/{owner}/{repo}/info/refs`
  - `POST /git/{owner}/{repo}/git-upload-pack`
  - `POST /git/{owner}/{repo}/git-receive-pack`

---

## 3. Authentication ✅ Fully Implemented

- **Credential helper in loom-cli**: ✅ Full implementation in [credential_helper.rs](file:///home/ghuntley/loom/crates/loom-cli/src/credential_helper.rs)
  - Supports `get`, `store`, `erase` operations
  - Returns `username=oauth2, password={session_token}`
  - Uses stored token from `loom login`
- **Weaver auto-injected credentials**: ⚠️ **NOT VERIFIED** - No code found that injects credentials into weavers for SCM. The spec says credentials should be auto-injected when a weaver is launched, but no implementation was found linking weaver launch to SCM credential injection.

---

## 4. Access Control ⚠️ Partially Implemented

- **Repo-level roles (read, write, admin)**: ✅ `RepoRole` enum defined in [types.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/types.rs#L62-L68)
- **Team-based access**: ⚠️ **Schema only** - `repo_team_access` table exists in [schema.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/schema.rs#L38-L45) and `RepoTeamAccess` type exists, but:
  - No `RepoTeamAccessStore` trait or implementation
  - No API endpoints for managing team access
  - Not used in access control checks in the server
- **Who can grant access**: ⚠️ Partially - repo owner checks exist, but formal role-based access grant API is missing

**Missing**:
- API endpoints: `GET/POST/DELETE /api/v1/repos/{id}/access`
- Team access management endpoints
- Integration of team access in permission checks

---

## 5. Branch Protection ✅ Fully Implemented

- **Pattern matching (e.g., 'cannon', 'release/*')**: ✅ [protection.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/protection.rs#L161-L175)
- **Block direct push, force-push, deletion**: ✅ All three options supported
- **Admins can bypass**: ✅ `user_is_admin` check in `check_push_allowed`
- **API endpoints**: ✅ All three endpoints implemented in [protection.rs routes](file:///home/ghuntley/loom/crates/loom-server/src/routes/protection.rs)
- **Git receive-pack integration**: ✅ Protection rules checked during push in [git.rs](file:///home/ghuntley/loom/crates/loom-server/src/routes/git.rs#L440-L475)

---

## 6. Repository Lifecycle ✅ Fully Implemented

- **Create via API (not push-to-create)**: ✅ `POST /api/v1/repos` in [repos.rs](file:///home/ghuntley/loom/crates/loom-server/src/routes/repos.rs)
- **Soft delete (recoverable)**: ✅ `DELETE /api/v1/repos/{id}` sets `deleted_at`
- **Default branch is 'cannon'**: ✅ Hardcoded in [types.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/types.rs#L114)
- **Creates bare git repo on disk**: ✅ Uses `GitRepository::init_bare()` via gitoxide

---

## 7. Mirroring ⚠️ Partially Implemented

### Push Mirroring (Loom → External): ✅ Implemented
- [push.rs](file:///home/ghuntley/loom/crates/loom-scm-mirror/src/push.rs) - Full implementation
- Branch rules with pattern matching
- Credential integration via `loom-credentials`
- **Missing API endpoints**: No routes found for:
  - `GET /api/v1/repos/{id}/mirrors`
  - `POST /api/v1/repos/{id}/mirrors`
  - `DELETE /api/v1/repos/{id}/mirrors/{mid}`
  - `POST /api/v1/repos/{id}/mirrors/{mid}/sync`

### Pull Mirroring (External → Loom): ✅ Core Implemented
- [pull.rs](file:///home/ghuntley/loom/crates/loom-scm-mirror/src/pull.rs) - Cloning and fetching
- Supports GitHub and GitLab platforms
- `check_repo_exists()` API check

### Mirror Cleanup: ✅ Implemented
- [cleanup.rs](file:///home/ghuntley/loom/crates/loom-scm-mirror/src/cleanup.rs) - Stale mirror cleanup
- 3-month threshold (configurable via `stale_after` duration)
- Job integration in [mirror_cleanup.rs](file:///home/ghuntley/loom/crates/loom-server/src/jobs/mirror_cleanup.rs)

**Missing**:
- Mirror management API endpoints (CRUD operations)
- Integration with weaver launch for auto-mirroring
- `mirrors/github/{owner}/{repo}` namespace handling

---

## 8. Webhooks ✅ Fully Implemented

- **Per-repo webhooks**: ✅ [webhooks.rs routes](file:///home/ghuntley/loom/crates/loom-server/src/routes/webhooks.rs)
- **Org-level webhooks**: ✅ Same file, org endpoints
- **HMAC-SHA256 signature**: ✅ [delivery module](file:///home/ghuntley/loom/crates/loom-server-scm/src/webhook.rs#L640-L654)
- **GitHub-compat and Loom-v1 payload formats**: ✅ [payload module](file:///home/ghuntley/loom/crates/loom-server-scm/src/webhook.rs#L400-L638)
- **Retry with job scheduler**: ✅ `webhook_deliveries` table with retry logic
- **Events**: ✅ `push`, `repo.created`, `repo.deleted`

---

## 9. Web UI ⚠️ Partially Implemented

Based on the directory structure at `web/loom-web/src/routes/(app)/repos/[owner]/[repo]/`:

| Feature | Status | Route |
|---------|--------|-------|
| File browser | ✅ | `tree/` |
| Commit history | ✅ | `commits/` |
| Commit detail | ✅ | `commit/` |
| Blame | ✅ | `blame/` |
| Branch comparison | ✅ | `compare/` |
| File view | ✅ | `blob/` |
| Settings | ✅ | `settings/` |
| Branches | ✅ | `branches/` |
| Syntax highlighting | ❓ | Not verified |
| Open in Weaver button | ❓ | Not verified (needs code review) |

**Likely complete** but would need to inspect individual component files to confirm syntax highlighting and "Open in Weaver" button.

---

## 10. Git Maintenance ✅ Fully Implemented

- **Tasks (gc, prune, repack, fsck)**: ✅ [maintenance.rs](file:///home/ghuntley/loom/crates/loom-server-scm/src/maintenance.rs)
- **Per-repo maintenance**: ✅ `POST /api/v1/repos/{id}/maintenance`
- **Global sweep**: ✅ `POST /api/v1/admin/maintenance/sweep`
- **Staggered execution**: ✅ `stagger_delay` parameter in `run_global_sweep`
- **Job tracking**: ✅ `repo_maintenance_jobs` table with full status tracking
- **API endpoints**: ✅ [maintenance.rs routes](file:///home/ghuntley/loom/crates/loom-server/src/routes/maintenance.rs)

---

## Deviations from Spec

1. **Route pattern**: Spec says `/git/{owner}/{repo}.git/info/refs` but implementation uses `/git/{owner}/{repo}/info/refs` (without `.git` suffix in path). The `.git` suffix is stripped from the repo name parameter instead.

2. **User resolution in Git routes**: The spec implies user repos can be resolved via username, but the current implementation only resolves org repos via slug. User repos by username may not work.

---

## Missing Features Summary

### Critical (Core functionality gaps)

1. **Team-based access control API** - Schema exists but no endpoints or integration
2. **Mirror management API endpoints** - Core mirroring works but no CRUD API

### Important (Spec features not implemented)

3. **Weaver auto-credential injection** - No code linking weaver launch to SCM credentials
4. **User repo access via username in Git routes** - Only org slug resolution implemented

### Minor (May need verification)

5. **Open in Weaver button** - Web UI route exists but button presence not verified
6. **Syntax highlighting** - File browser exists but highlighting not verified

---

## Recommendations

1. **High Priority**: Implement mirror management API endpoints to allow users to configure push mirrors
2. **High Priority**: Implement team access management endpoints and integrate into permission checks
3. **Medium Priority**: Add weaver credential injection for SCM access
4. **Low Priority**: Add user repo resolution by username in Git HTTP routes
5. **Low Priority**: Verify Web UI features (syntax highlighting, Open in Weaver)
