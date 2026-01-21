<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Weaver Provisioner Specification

**Version:** 1.0\
**Last Updated:** 2025-01-29

---

## 1. Overview

### Purpose

The Weaver Provisioner is a Rust-based infrastructure component for creating, managing, and monitoring
ephemeral, isolated execution environments using Kubernetes Pods. It provides a REST API for
provisioning weavers with automatic TTL-based cleanup.

### Goals

- **Ephemerality**: All workloads are short-lived (default TTL: 4 hours, max: 48 hours)
- **Isolation**: Pods run with security-hardened contexts (non-root, dropped capabilities)
- **Observability**: Real-time SSE log streaming, Prometheus metrics
- **Simplicity**: K8s is source of truth, no database for weaver state
- **Testability**: Separate crates for K8s abstraction and business logic

### Non-Goals

- Persistent storage for weavers
- Multi-cluster support
- Custom network policies (use cluster defaults)
- Image registry authentication (handled by cluster)

---

## 2. Architecture

### 2.1 Crate Structure

```
crates/
├── loom-k8s/                # K8s client trait + kube implementation
├── loom-weaver/             # Business logic, cleanup, webhooks
└── loom-server/             # HTTP endpoints (/api/weaver*)
```

### 2.2 Dependency Graph

```
┌─────────────────┐
│   loom-server   │
│  (HTTP routes)  │
└────────┬────────┘
         │
         ▼
┌─────────────────────────┐
│      loom-weaver        │
│   (business logic)      │
└────────┬────────────────┘
         │
         ▼
┌─────────────────┐
│    loom-k8s     │
│  (K8s client)   │
└─────────────────┘
```

### 2.3 K8s Client Abstraction

The `loom-k8s` crate provides a trait-based abstraction for testability:

```rust
#[async_trait]
pub trait K8sClient: Send + Sync {
    async fn create_pod(&self, spec: PodSpec) -> Result<Pod, K8sError>;
    async fn delete_pod(&self, name: &str, namespace: &str, grace_period: u32) -> Result<(), K8sError>;
    async fn list_pods(&self, namespace: &str, label_selector: &str) -> Result<Vec<Pod>, K8sError>;
    async fn get_pod(&self, name: &str, namespace: &str) -> Result<Pod, K8sError>;
    async fn get_namespace(&self, name: &str) -> Result<Namespace, K8sError>;
    async fn stream_logs(&self, name: &str, namespace: &str, opts: LogOptions) -> Result<LogStream, K8sError>;
}
```

Real implementation uses the `kube` crate. Tests use mock implementations.

---

## 3. Weaver Identification

### 3.1 UUID7

Weavers are identified using UUID7 (time-ordered, globally unique):

```rust
use uuid7::uuid7;

pub struct WeaverId(uuid7::Uuid);

impl WeaverId {
    pub fn new() -> Self {
        Self(uuid7())
    }
}
```

Add `uuid7` as a workspace dependency.

### 3.2 Pod Naming

Pod name format: `weaver-{uuid7}`

Example: `weaver-018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g`

### 3.3 K8s Labels and Annotations

```yaml
metadata:
  name: weaver-018f6b2a-...
  labels:
    loom.dev/managed: "true"
    loom.dev/weaver-id: "018f6b2a-..."
    loom.dev/owner-user-id: "user-uuid-here"
  annotations:
    loom.dev/tags: '{"project":"ai-worker","env":"prod"}'
    loom.dev/lifetime-hours: "4"
```

---

## 4. State Management

### 4.1 K8s as Source of Truth

- **No database** for weaver state
- All operations query K8s directly
- Labels enable filtering (`loom.dev/managed=true`)
- Annotations store metadata (tags, lifetime)

### 4.2 Weaver Status

Mapped from K8s Pod phase:

```rust
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum WeaverStatus {
    Pending,    // Pod created, containers starting
    Running,    // Containers running
    Succeeded,  // Completed successfully (exit 0)
    Failed,     // Container failed (non-zero exit)
}
```

---

## 5. API Endpoints

### 5.1 Endpoint Summary

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/weaver` | Provision new weaver |
| GET | `/api/weavers` | List managed weavers |
| GET | `/api/weaver/{id}` | Get weaver details |
| DELETE | `/api/weaver/{id}` | Delete weaver |
| GET | `/api/weaver/{id}/logs` | SSE log stream |
| POST | `/api/weavers/cleanup` | Manual cleanup trigger |

### 5.2 POST /api/weaver

Provision a new weaver.

**Request:**

```json
{
  "image": "python:3.12",
  "env": {
    "TASK_ID": "abc123",
    "API_URL": "https://api.example.com"
  },
  "resources": {
    "memory_limit": "8Gi",
    "cpu_limit": "4"
  },
  "tags": {
    "project": "ai-worker",
    "env": "prod"
  },
  "lifetime_hours": 8,
  "command": ["/bin/sh", "-c"],
  "args": ["python worker.py"],
  "workdir": "/app",
  "repo": "https://github.com/org/repo.git",
  "branch": "main"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `image` | string | Yes | Container image |
| `env` | object | No | Environment variables |
| `resources` | object | No | Resource limits |
| `tags` | object | No | User-defined metadata |
| `lifetime_hours` | u32 | No | TTL override (max: 48) |
| `command` | string[] | No | Override ENTRYPOINT |
| `args` | string[] | No | Override CMD |
| `workdir` | string | No | Override WORKDIR |
| `repo` | string | No | Git repository URL to clone (public https) |
| `branch` | string | No | Git branch to checkout |

**Response (201 Created):**

```json
{
  "id": "018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "pod_name": "weaver-018f6b2a-3b4c-7d8e-9f0a-1b2c3d4e5f6g",
  "status": "running",
  "created_at": "2025-01-15T12:34:56Z",
  "owner_user_id": "user-uuid-here"
}
```

**Behavior:**
- Waits until Pod reaches `Running` or error state (timeout configurable)
- Returns `429 Too Many Requests` if max concurrent limit reached
- Returns `400 Bad Request` if lifetime exceeds max

### 5.3 GET /api/weavers

List all managed weavers.

**Query Parameters:**

| Param | Type | Description |
|-------|------|-------------|
| `tag` | string | Filter by tag (e.g., `project:ai-worker`). Multiple allowed. |

**Response:**

```json
{
  "weavers": [
    {
      "id": "018f6b2a-...",
      "pod_name": "weaver-018f6b2a-...",
      "status": "running",
      "image": "python:3.12",
      "tags": {"project": "ai-worker"},
      "created_at": "2025-01-15T12:34:56Z",
      "expires_at": "2025-01-15T16:34:56Z"
    }
  ]
}
```

### 5.4 GET /api/weaver/{id}

Get weaver details.

**Response:**

```json
{
  "id": "018f6b2a-...",
  "pod_name": "weaver-018f6b2a-...",
  "status": "running",
  "image": "python:3.12",
  "tags": {"project": "ai-worker"},
  "created_at": "2025-01-15T12:34:56Z",
  "expires_at": "2025-01-15T16:34:56Z",
  "resources": {
    "memory_limit": "8Gi",
    "cpu_limit": "4"
  }
}
```

### 5.5 DELETE /api/weaver/{id}

Delete a weaver.

**Response (204 No Content)**

### 5.6 GET /api/weaver/{id}/logs

SSE stream of container logs.

**Query Parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `tail` | u32 | 100 | Initial lines to return |
| `timestamps` | bool | false | Include log timestamps |

**Response (SSE):**

```
data: {"line": "Starting worker...", "timestamp": "2025-01-15T12:35:00Z"}

data: {"line": "Processing task abc123", "timestamp": "2025-01-15T12:35:01Z"}
```

### 5.7 POST /api/weavers/cleanup

Trigger manual cleanup of expired weavers.

**Response:**

```json
{
  "deleted_count": 3,
  "deleted_ids": ["018f6b2a-...", "018f6b2b-...", "018f6b2c-..."]
}
```

### 5.8 Authentication

All weaver endpoints require authentication via:
- Session cookie (web)
- Bearer token (CLI/API)

Authorization is based on ownership:
- Users can only access their own weavers
- System administrators can access all weavers

---

## 6. Error Responses

### 6.1 Format

```json
{
  "error": {
    "code": "weaver_not_found",
    "message": "Weaver with ID 018f6b2a-... not found"
  }
}
```

### 6.2 Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `weaver_not_found` | 404 | Weaver ID doesn't exist |
| `too_many_weavers` | 429 | Max concurrent limit reached |
| `invalid_lifetime` | 400 | TTL exceeds max (48h) |
| `weaver_failed` | 500 | Pod failed to start |
| `weaver_timeout` | 504 | Pod didn't reach running state in time |
| `k8s_error` | 502 | K8s API failure |
| `unauthorized` | 401 | Missing/invalid API key |

---

## 7. Configuration

### 7.1 Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `LOOM_SERVER_WEAVER_K8S_NAMESPACE` | `loom-weavers` | Target namespace |
| `LOOM_SERVER_WEAVER_API_KEY` | (required) | API key for authentication |
| `LOOM_SERVER_WEAVER_CLEANUP_INTERVAL_SECS` | `1800` | Cleanup task interval (30 min) |
| `LOOM_SERVER_WEAVER_DEFAULT_TTL_HOURS` | `4` | Default weaver lifetime |
| `LOOM_SERVER_WEAVER_MAX_TTL_HOURS` | `48` | Maximum lifetime override |
| `LOOM_SERVER_WEAVER_MAX_CONCURRENT` | `64` | Maximum running weavers |
| `LOOM_SERVER_WEAVER_READY_TIMEOUT_SECS` | `60` | Timeout waiting for running state |
| `LOOM_SERVER_WEAVER_WEBHOOKS` | `[]` | JSON array of webhook configs |

### 7.2 Webhook Configuration

```bash
LOOM_SERVER_WEAVER_WEBHOOKS='[
  {
    "url": "https://billing.example.com/hooks",
    "events": ["weaver.created", "weaver.deleted"],
    "secret": "whsec_xxxxx"
  }
]'
```

Webhooks are admin-only (configured at deploy time, no CRUD API).

---

## 8. Resource Defaults

### 8.1 Pod Resources

```yaml
resources:
  requests: {}           # None - allows overcommit
  limits:
    memory: "16Gi"       # Default max
    # No CPU limit - can use all available cores
```

Override via request:

```json
{
  "resources": {
    "memory_limit": "8Gi",
    "cpu_limit": "4"
  }
}
```

### 8.2 Pod Configuration

| Setting | Value |
|---------|-------|
| Restart Policy | `Never` |
| Grace Period | `5s` |
| Container Name | `weaver` |
| Service Account | `default` (namespace default) |

---

## 9. Security Context

All weaver Pods run with hardened security:

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 1000
  runAsGroup: 1000
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  capabilities:
    drop:
      - ALL
```

### 9.1 Not Supported

- Volume mounts (Secrets, ConfigMaps, PVCs)
- Node selection (nodeSelector, tolerations)
- Priority classes
- Custom service accounts
- Custom network policies (use cluster defaults)
- Custom DNS (use cluster defaults)

---

## 10. Cleanup System

### 10.1 Automatic Cleanup

Background task runs every `CLEANUP_INTERVAL_SECS`:

1. List Pods with `loom.dev/managed=true`
2. Calculate age from `creationTimestamp`
3. Compare against `loom.dev/lifetime-hours` annotation
4. Delete expired Pods (grace period: 5s)

### 10.2 Startup Behavior

On server start:
1. Validate namespace exists (fail if not)
2. Run cleanup immediately (reconcile orphaned weavers)
3. Start interval-based cleanup task

### 10.3 Shutdown Behavior

Leave weavers running. Cleanup resumes when server restarts.

---

## 11. Webhooks

### 11.1 Events

| Event | Trigger |
|-------|---------|
| `weaver.created` | POST `/api/weaver` success |
| `weaver.deleted` | DELETE or cleanup |
| `weaver.failed` | Pod enters failed state |
| `weavers.cleanup` | Cleanup task completes |

### 11.2 Payload Format

```json
{
  "event": "weaver.created",
  "timestamp": "2025-01-15T12:34:56Z",
  "weaver": {
    "id": "018f6b2a-...",
    "image": "python:3.12",
    "tags": {"project": "ai-worker"}
  }
}
```

### 11.3 Delivery

- HMAC-SHA256 signature in `X-Webhook-Signature` header (if secret configured)
- Fire-and-forget (no retries)

---

## 12. Prometheus Metrics

Added to existing `/metrics` endpoint:

| Metric | Type | Description |
|--------|------|-------------|
| `loom_weavers_created_total` | Counter | Weavers provisioned |
| `loom_weavers_deleted_total` | Counter | Weavers deleted (manual + cleanup) |
| `loom_weavers_failed_total` | Counter | Weavers that entered failed state |
| `loom_weavers_cleanup_total` | Counter | Cleanup runs completed |
| `loom_weavers_cleanup_deleted_total` | Counter | Weavers deleted by cleanup |
| `loom_weavers_active` | Gauge | Currently running weavers |

---

## 13. Health Check

K8s connectivity added to `/health` response:

```json
{
  "status": "healthy",
  "components": {
    "kubernetes": {
      "status": "healthy",
      "latency_ms": 45,
      "namespace": "loom-weavers",
      "reachable": true
    }
  }
}
```

K8s unreachable → overall status `unhealthy` (critical component).

---

## 14. Testing Strategy

### 14.1 Unit Tests (loom-weaver)

Mock `K8sClient` trait:

```rust
struct MockK8sClient {
    pods: Arc<Mutex<Vec<Pod>>>,
}

impl K8sClient for MockK8sClient {
    async fn create_pod(&self, spec: PodSpec) -> Result<Pod, K8sError> {
        // Return canned response
    }
}
```

### 14.2 Integration Tests

Use Minikube or kind for real K8s:

```rust
#[tokio::test]
#[ignore] // Requires K8s cluster
async fn test_full_weaver_lifecycle() {
    let client = KubeClient::new().await;
    let provisioner = Provisioner::new(client);
    
    let weaver = provisioner.create_weaver(req).await.unwrap();
    assert_eq!(weaver.status, WeaverStatus::Running);
    
    provisioner.delete_weaver(&weaver.id).await.unwrap();
}
```

---

## 15. NixOS Deployment

### 15.1 K3s Module Options

The `k3s.nix` module configures K3s for weaver workloads:

```nix
services.loom-k3s = {
  enable = true;
  role = "server";
  clusterInit = true;
  disableTraefik = true;
};
```

### 15.2 Loom-Server Weaver Options

The `loom-server.nix` module includes weaver provisioner configuration:

```nix
services.loom-server.weaver = {
  enable = true;
  namespace = "loom-weavers";
  cleanupIntervalSecs = 1800;
  defaultTtlHours = 4;
  maxTtlHours = 48;
  maxConcurrent = 64;
  readyTimeoutSecs = 60;
};
```

### 15.3 Service Dependencies

- K3s must be running before loom-server starts
- The `loom-weavers` namespace is created automatically by the module

### 15.4 Files Created/Updated

| File | Description |
|------|-------------|
| `infra/nixos-modules/k3s.nix` | New module for K3s configuration |
| `infra/nixos-modules/loom-server.nix` | Updated with weaver options |
| `infra/machines/loom.nix` | Updated to enable both modules |

---

## 16. WireGuard Tunnel Support

### 16.1 Overview

Weavers can be configured to register with the wgtunnel server, enabling SSH access to weaver pods via a WireGuard VPN tunnel. This allows secure remote access to weaver containers without exposing SSH ports publicly.

### 16.2 Configuration

Enable WireGuard tunnel support in the weaver provisioner config:

```nix
services.loom-server.weaver = {
  wgEnabled = true;
};
```

### 16.3 Environment Variables

The following environment variables are **always** injected into weaver pods:

| Variable | Description |
|----------|-------------|
| `LOOM_SERVER_URL` | URL to the loom server (for LLM proxy, secrets, etc.) |
| `LOOM_WEAVER_ID` | The weaver's UUID7 identifier |

When `wg_enabled` is true, the following additional variable is injected:

| Variable | Description |
|----------|-------------|
| `LOOM_WG_ENABLED` | Set to `"true"` when WireGuard is enabled |

### 16.4 Pod Labels

WireGuard-enabled pods include an additional label for identification:

```yaml
labels:
  loom.dev/wg-enabled: "true"
```

### 16.5 Container Image Requirements

For WireGuard tunnel SSH access to work, the weaver container image **MUST** include:

1. **OpenSSH server (`sshd`)** - installed and configured
2. **WireGuard client integration** - the weaver entrypoint should initialize the WG tunnel

Example additions to `Dockerfile.weaver`:

```dockerfile
# Install sshd for remote access via WireGuard tunnel
RUN apt-get update && apt-get install -y \
    openssh-server \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd

# Configure sshd to listen on internal port (accessed via WG tunnel)
RUN echo "Port 22" >> /etc/ssh/sshd_config \
    && echo "PermitRootLogin no" >> /etc/ssh/sshd_config \
    && echo "PasswordAuthentication no" >> /etc/ssh/sshd_config \
    && echo "PubkeyAuthentication yes" >> /etc/ssh/sshd_config
```

The entrypoint script should:
1. Check if `LOOM_WG_ENABLED=true`
2. Initialize the WireGuard tunnel client to register with `$LOOM_SERVER_URL`
3. Start `sshd` in the background
4. Continue with the normal weaver workload

---

## 17. Future Considerations

### 17.1 Potential Extensions

- Multi-namespace support
- Weaver exec (interactive shell)
- Resource usage metrics per weaver
- Weaver logs persistence
- Webhook retry with backoff
- Weaver templates/presets

### 17.2 Not Planned

- Multi-cluster support
- Persistent volumes
- Sidecar containers
- Init containers
