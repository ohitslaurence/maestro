<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Weaver CLI Specification

**Version:** 1.0\
**Last Updated:** 2025-01-30

---

## 1. Overview

### Purpose

This specification describes the CLI commands for provisioning and managing remote weaver sessions. Weavers are ephemeral Kubernetes pods that run the Loom REPL in an isolated environment, enabling sandboxed code execution with optional git repository cloning.

### Goals

- **Remote REPL**: Run Loom sessions in isolated K8s pods instead of locally
- **Git integration**: Clone public repositories into weaver workspace
- **Attach/detach**: Connect to running weavers like SSH/tmux sessions
- **Ephemeral**: Weavers auto-cleanup via TTL (default 4h, max 48h)

### Non-Goals

- Private repository authentication (future work)
- Persistent storage across weaver sessions
- Multi-user shared weavers

---

## 2. Command Reference

### 2.1 Command Summary

| Command | Description |
|---------|-------------|
| `loom new` | Provision weaver, clone repo, attach to REPL |
| `loom attach <id>` | Attach to running weaver |
| `loom weaver new` | Same as `loom new` |
| `loom weaver attach <id>` | Same as `loom attach <id>` |
| `loom weaver ps` | List running weavers |
| `loom weaver delete <id>` | Delete a weaver |

### 2.2 `loom new` / `loom weaver new`

Provision a new weaver and attach to its REPL.

```
loom new [OPTIONS]
loom weaver new [OPTIONS]

OPTIONS:
    -i, --image <IMAGE>      Container image (default: ghcr.io/ghuntley/loom:latest)
        --repo <URL>         Git repository to clone (public https URL)
        --branch <NAME>      Branch to checkout (default: repository default)
    -e, --env <KEY=VALUE>    Environment variable (repeatable)
        --ttl <HOURS>        Lifetime in hours (default: 4, max: 48)
```

**Examples:**

```bash
# Start weaver with default image
loom new

# Clone a repository
loom new --repo https://github.com/org/repo.git

# Full example
loom new \
  --image ghcr.io/ghuntley/loom:v1.0.0 \
  --repo https://github.com/org/repo.git \
  --branch develop \
  -e API_URL=https://api.example.com \
  -e DEBUG=true \
  --ttl 8
```

**Behavior:**

1. Call `POST /api/weaver` to provision pod
2. Stream pod logs (clone progress, startup)
3. Once pod is running, attach to REPL via WebSocket
4. User interacts with remote Loom REPL
5. Detach with `Ctrl+P, Ctrl+Q` (pod keeps running)

**Output:**

```
Creating weaver...
  ID:    018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g
  Image: ghcr.io/ghuntley/loom:latest
  TTL:   4 hours

Cloning into '/workspace'...
remote: Enumerating objects: 1234, done.
remote: Counting objects: 100% (1234/1234), done.
Receiving objects: 100% (1234/1234), 2.5 MiB | 10.0 MiB/s, done.
Cloning complete.

Starting loom...
[attached to weaver 018f6b2a]

loom>
```

### 2.3 `loom attach` / `loom weaver attach`

Attach to a running weaver's REPL.

```
loom attach <WEAVER_ID>
loom weaver attach <WEAVER_ID>

ARGS:
    <WEAVER_ID>    Weaver ID to attach to
```

**Examples:**

```bash
loom attach 018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g
loom attach 018f6b2a  # Prefix matching
```

**Behavior:**

1. Connect to `/api/weaver/{id}/attach` WebSocket endpoint
2. Relay stdin/stdout bidirectionally
3. Detach with `Ctrl+P, Ctrl+Q`

**Output on detach:**

```
[detached from weaver 018f6b2a]
Weaver still running. Reattach with: loom attach 018f6b2a
```

### 2.4 `loom weaver ps`

List running weavers.

```
loom weaver ps [OPTIONS]

OPTIONS:
        --json    Output as JSON
```

**Output:**

```
ID                                        IMAGE                           STATUS    AGE     TTL
018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g      ghcr.io/ghuntley/loom:latest   running   2h      4h
018f6b2b-1234-5678-90ab-cdef12345678      ghcr.io/ghuntley/loom:v1.0.0   running   30m     8h
```

### 2.5 `loom weaver delete`

Delete a weaver (graceful termination).

```
loom weaver delete <WEAVER_ID>

ARGS:
    <WEAVER_ID>    Weaver ID to delete
```

**Output:**

```
Deleting weaver 018f6b2a...
Weaver deleted.
```

---

## 3. Architecture

### 3.1 System Diagram

```
┌─────────────────┐        WebSocket         ┌─────────────────┐        K8s exec        ┌─────────────────┐
│    Local CLI    │◄───────stdin/stdout─────►│   loom-server   │◄──────attach──────────►│     Weaver      │
│   (terminal)    │                          │    (proxy)      │                        │   (loom REPL)   │
└─────────────────┘                          └────────┬────────┘                        └────────┬────────┘
                                                      │                                          │
                                                      │                                          │
                                                      ▼                                          ▼
                                                 LLM Providers ◄─────────────────────────────────┘
                                            (Anthropic, OpenAI)        (LLM calls from pod)
```

### 3.2 Connection Flow

1. **CLI** connects to **loom-server** via WebSocket (`/api/weaver/{id}/attach`)
2. **loom-server** connects to **K8s API** via exec/attach
3. Bidirectional relay: CLI stdin → pod stdin, pod stdout → CLI stdout
4. LLM calls: pod → loom-server → LLM providers (CLI not in loop)

### 3.3 Pod Lifecycle

```
┌─────────┐    create    ┌─────────┐    clone    ┌─────────┐    attach    ┌─────────┐
│ Pending │────────────►│ Running │────────────►│  Ready  │─────────────►│Attached │
└─────────┘              └─────────┘              └─────────┘              └─────────┘
     │                        │                       │                        │
     │                        │                       │                        │
     ▼                        ▼                       ▼                        ▼
  (failed)                (failed)              (TTL expired)            (detached)
     │                        │                       │                        │
     └────────────────────────┴───────────────────────┴────────────────────────┘
                                         │
                                         ▼
                                    ┌─────────┐
                                    │ Deleted │
                                    └─────────┘
```

---

## 4. Implementation

### 4.1 Crate Changes

| Crate | Change |
|-------|--------|
| `loom-k8s` | Add `exec_attach()` method to `K8sClient` trait |
| `loom-weaver` | Add `repo`, `branch` to `CreateWeaverRequest`; update pod spec |
| `loom-server` | Add `/api/weaver/{id}/attach` WebSocket endpoint |
| `loom-cli` | Add `new`, `attach`, `weaver` commands; WebSocket client |
| `docker/` | Weaver entrypoint script for repo clone + loom start |

### 4.2 loom-k8s: Exec/Attach

```rust
#[async_trait]
pub trait K8sClient: Send + Sync {
    // ... existing methods ...

    /// Attach to a running container's stdin/stdout.
    async fn exec_attach(
        &self,
        name: &str,
        namespace: &str,
        container: &str,
    ) -> Result<AttachedProcess, K8sError>;
}

/// Bidirectional stream for container I/O.
pub struct AttachedProcess {
    stdin: Box<dyn AsyncWrite + Send + Unpin>,
    stdout: Box<dyn AsyncRead + Send + Unpin>,
    status: Box<dyn Future<Output = Result<i32, K8sError>> + Send + Unpin>,
}
```

### 4.3 loom-weaver: Request Changes

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWeaverRequest {
    pub image: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub resources: ResourceSpec,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    pub lifetime_hours: Option<u32>,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub workdir: Option<String>,

    // New fields
    /// Git repository URL to clone (public https)
    pub repo: Option<String>,
    /// Git branch to checkout
    pub branch: Option<String>,
}
```

### 4.4 loom-server: WebSocket Endpoint

```
GET /api/weaver/{id}/attach
Upgrade: websocket

Headers:
  Authorization: Bearer <token>

WebSocket frames:
  - Binary: stdin/stdout data
  - Text (JSON): control messages (resize, ping)
```

### 4.5 loom-cli: Command Structure

```rust
#[derive(Subcommand, Debug)]
enum Command {
    // ... existing commands ...

    /// Create a new remote weaver session (alias: weaver new)
    New {
        #[arg(long, short)]
        image: Option<String>,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, short = 'e', value_name = "KEY=VALUE")]
        env: Vec<String>,
        #[arg(long)]
        ttl: Option<u32>,
    },

    /// Attach to a running weaver (alias: weaver attach)
    Attach {
        weaver_id: String,
    },

    /// Weaver management commands
    Weaver {
        #[command(subcommand)]
        command: WeaverCommand,
    },
}

#[derive(Subcommand, Debug)]
enum WeaverCommand {
    /// Create a new remote weaver session
    New { /* same fields as Command::New */ },
    /// Attach to a running weaver
    Attach { weaver_id: String },
    /// List running weavers
    Ps {
        #[arg(long)]
        json: bool,
    },
    /// Delete a weaver
    Delete { weaver_id: String },
}
```

### 4.6 Weaver Entrypoint

```bash
#!/bin/sh
# /entrypoint.sh - Weaver pod entrypoint

set -e

# Clone repository if specified
if [ -n "$LOOM_REPO" ]; then
    echo "Cloning $LOOM_REPO..."
    if [ -n "$LOOM_BRANCH" ]; then
        git clone --branch "$LOOM_BRANCH" --single-branch "$LOOM_REPO" /workspace
    else
        git clone "$LOOM_REPO" /workspace
    fi
    cd /workspace
    echo "Cloning complete."
fi

# Start loom REPL
exec loom
```

---

## 5. API Reference

### 5.1 POST /api/weaver (updated)

**Request:**

```json
{
  "image": "ghcr.io/ghuntley/loom:latest",
  "env": {
    "API_URL": "https://api.example.com"
  },
  "repo": "https://github.com/org/repo.git",
  "branch": "main",
  "lifetime_hours": 8
}
```

**Response (201 Created):**

```json
{
  "id": "018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "pod_name": "weaver-018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "status": "pending",
  "created_at": "2025-01-30T12:34:56Z"
}
```

### 5.2 GET /api/weaver/{id}/attach

Upgrade to WebSocket for terminal I/O.

**Headers:**

```
Connection: Upgrade
Upgrade: websocket
Authorization: Bearer <token>
```

**WebSocket Protocol:**

| Frame Type | Direction | Description |
|------------|-----------|-------------|
| Binary | Client → Server | stdin data |
| Binary | Server → Client | stdout data |
| Text (JSON) | Client → Server | `{"type": "resize", "cols": 80, "rows": 24}` |
| Text (JSON) | Server → Client | `{"type": "status", "status": "attached"}` |

---

## 6. Security

### 6.1 Authentication

- All weaver endpoints require valid JWT token
- Token passed in `Authorization` header (HTTP) or WebSocket handshake
- Server validates token and authorizes weaver access
- CLI: use `--token <token>` flag or `LOOM_TOKEN` env var

### 6.2 Authorization (ABAC)

Weavers use Attribute-Based Access Control:

| Action | Owner | System Admin | Support |
|--------|-------|--------------|---------|
| Create | ✓ | ✓ | ✗ |
| List own | ✓ | ✓ (sees all) | ✓ (sees all) |
| Get | ✓ | ✓ | ✓ |
| Attach | ✓ | ✓ | ✓ (read-only) |
| Delete | ✓ | ✓ | ✗ |
| Cleanup | ✗ | ✓ | ✗ |

- Each weaver has an `owner_user_id` stored as K8s label
- Users can only manage their own weavers
- System admins can manage any weaver
- Support users have read-only access: they can view weaver output but cannot send input when attached

### 6.3 Pod Security

- Weavers run with security context from weaver-provisioner.md
- Non-root user (1000:1000)
- No privilege escalation
- Dropped capabilities

### 6.4 Git Clone

- Only public HTTPS URLs accepted initially
- No SSH keys or credentials injected
- Private repo support: future work (GitHub App tokens)

---

## 7. Future Work

- Private repository authentication via GitHub App
- Terminal resize support (SIGWINCH)
- Session multiplexing (multiple attachments)
- Weaver snapshots/checkpoints
- Custom resource limits via CLI flags
