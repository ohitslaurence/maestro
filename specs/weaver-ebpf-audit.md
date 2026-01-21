<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Weaver eBPF Audit Sidecar Specification

**Version:** 1.0\
**Last Updated:** 2025-01-03

---

## 1. Overview

### Purpose

The Weaver eBPF Audit Sidecar provides comprehensive syscall-level auditing for weaver pods. Using eBPF
(via Aya), it captures process execution, file operations, network activity, and privilege changes,
streaming enriched audit events to loom-server for compliance and security monitoring.

### Goals

- **Complete visibility**: Capture all security-relevant actions within weaver containers
- **Low overhead**: eBPF runs in kernel, minimal userspace processing
- **Reliable delivery**: Local buffer when server unreachable, flush when available
- **Integration**: Events flow through existing AuditService pipeline
- **Correlation**: Track socket lifecycles, correlate DNS lookups with connections

### Non-Goals

- File content capture (metadata only)
- Blocking/enforcement (audit-only, not LSM)
- Cross-pod correlation (per-pod sidecar scope)
- Real-time alerting (handled by downstream sinks)

---

## 2. Architecture

### 2.1 Pod Structure

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Weaver Pod                                 │
│  shareProcessNamespace: true                                        │
│                                                                      │
│  ┌─────────────────────┐      ┌─────────────────────────────────┐   │
│  │   weaver (main)     │      │   audit-sidecar (native)        │   │
│  │   loom REPL         │◄────►│   Aya eBPF loader               │   │
│  │   user 1000         │ PID  │   user 0                        │   │
│  │   non-privileged    │ NS   │   CAP_BPF + CAP_PERFMON         │   │
│  └─────────────────────┘      └──────────────┬──────────────────┘   │
│                                              │                       │
│  restartPolicy: Always (sidecar)             │ HTTP POST             │
│  Weaver blocked until sidecar ready          │ (batch every 100ms)   │
└──────────────────────────────────────────────┼───────────────────────┘
                                               │
                                               ▼
                                    loom-server
                                    POST /internal/weaver-audit/events
                                               │
                                               ▼
                                    AuditService pipeline
                                    (enrich → filter → redact → sinks)
```

### 2.2 Crate Structure

```
crates/
├── loom-weaver-ebpf/                  # eBPF programs (Aya, bpfel-unknown-none)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Module exports
│       ├── common.rs                  # Shared types between eBPF and userspace
│       ├── exec.rs                    # execve/execveat tracepoints
│       ├── file.rs                    # File operation tracepoints
│       ├── network.rs                 # Socket syscall tracepoints
│       ├── privilege.rs               # Privilege change tracepoints
│       └── process.rs                 # Fork/clone/exit tracepoints
│
├── loom-weaver-ebpf-common/           # Shared types (no_std compatible)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                     # Event structs, enums
│
└── loom-weaver-audit-sidecar/         # Userspace binary
    ├── Cargo.toml
    └── src/
        ├── main.rs                    # Entry point, eBPF loader
        ├── config.rs                  # Configuration from env vars
        ├── events.rs                  # Event processing pipeline
        ├── filter.rs                  # Path/process filtering
        ├── buffer.rs                  # Local file buffer
        ├── client.rs                  # HTTP client to loom-server
        ├── connection_tracker.rs      # Socket lifecycle tracking
        ├── dns_cache.rs               # DNS hostname correlation
        ├── metrics.rs                 # Prometheus metrics
        └── health.rs                  # Health endpoint
```

### 2.3 Dependency Graph

```
┌──────────────────────────────┐
│  loom-weaver-audit-sidecar   │
│  (userspace binary)          │
└──────────────┬───────────────┘
               │
       ┌───────┴───────┐
       │               │
       ▼               ▼
┌──────────────┐  ┌──────────────────────┐
│ loom-weaver  │  │ loom-weaver-ebpf-    │
│ -ebpf        │  │ common               │
│ (eBPF progs) │  │ (shared types)       │
└──────┬───────┘  └──────────────────────┘
       │                    ▲
       └────────────────────┘
```

---

## 3. Syscall Coverage

### 3.1 Process Events

| Syscall | Event Type | Data Captured |
|---------|------------|---------------|
| `execve`, `execveat` | `WeaverProcessExec` | path, argv, envp (redacted), cwd, uid/gid, ppid |
| `clone`, `clone3`, `fork`, `vfork` | `WeaverProcessFork` | parent_pid, child_pid, clone_flags |
| `exit_group` | `WeaverProcessExit` | exit_code |

### 3.2 File Events

| Syscall | Event Type | Condition |
|---------|------------|-----------|
| `openat`, `openat2` | `WeaverFileWrite` | O_WRONLY, O_RDWR, O_CREAT, O_TRUNC |
| `openat`, `openat2` | `WeaverFileRead` | O_RDONLY (sensitive paths only) |
| `write`, `pwrite64`, `writev` | `WeaverFileWrite` | Always |
| `rename`, `renameat`, `renameat2` | `WeaverFileWrite` | Always |
| `unlink`, `unlinkat` | `WeaverFileWrite` | Always |
| `truncate`, `ftruncate` | `WeaverFileWrite` | Always |
| `chmod`, `fchmod`, `fchmodat` | `WeaverFileMetadata` | Always |
| `chown`, `lchown`, `fchown`, `fchownat` | `WeaverFileMetadata` | Always |
| `setxattr`, `lsetxattr`, `fsetxattr` | `WeaverFileMetadata` | Always |
| `mknod`, `mknodat` | `WeaverFileWrite` | Always (high priority) |

### 3.3 Network Events

| Syscall | Event Type | Data Captured |
|---------|------------|---------------|
| `socket` | `WeaverNetworkSocket` | domain, type, protocol |
| `connect` | `WeaverNetworkConnect` | remote_ip, remote_port, hostname (via DNS cache) |
| `bind` | `WeaverNetworkListen` | local_ip, local_port |
| `listen` | `WeaverNetworkListen` | backlog |
| `accept`, `accept4` | `WeaverNetworkAccept` | remote_ip, remote_port |
| `sendto` (UDP port 53) | `WeaverDnsQuery` | query payload |
| `recvfrom` (UDP port 53) | `WeaverDnsResponse` | response payload, parsed A/AAAA records |

**Filtering:**
- Egress only (no localhost `127.0.0.1` / `::1`)
- Include `AF_UNIX` sockets (IPC detection)

### 3.4 Privilege Events

| Syscall | Event Type | Data Captured |
|---------|------------|---------------|
| `setuid`, `setgid`, `setresuid`, `setresgid` | `WeaverPrivilegeChange` | old/new uid/gid |
| `capset` | `WeaverPrivilegeChange` | capabilities changed |
| `prctl` | `WeaverPrivilegeChange` | option, arg (filtered) |
| `ptrace` | `WeaverPrivilegeChange` | request, target_pid |

### 3.5 Memory Events

| Syscall | Event Type | Condition |
|---------|------------|-----------|
| `mmap`, `mmap2` | `WeaverMemoryExec` | PROT_EXEC flag |
| `mprotect` | `WeaverMemoryExec` | Adding PROT_EXEC |

### 3.6 Sandbox Escape Vectors

| Syscall | Event Type | Priority |
|---------|------------|----------|
| `unshare` | `WeaverSandboxEscape` | Critical |
| `setns` | `WeaverSandboxEscape` | Critical |
| `mount`, `umount2` | `WeaverSandboxEscape` | Critical |
| `init_module`, `finit_module` | `WeaverSandboxEscape` | Critical |
| `bpf` | `WeaverSandboxEscape` | Critical |
| `perf_event_open` | `WeaverSandboxEscape` | Warning |

---

## 4. Event Types

### 4.1 New AuditEventType Variants

Add to `crates/loom-server-audit/src/event.rs`:

```rust
// Weaver syscall audit events
WeaverProcessExec,
WeaverProcessFork,
WeaverProcessExit,
WeaverFileWrite,
WeaverFileRead,
WeaverFileMetadata,
WeaverNetworkSocket,
WeaverNetworkConnect,
WeaverNetworkListen,
WeaverNetworkAccept,
WeaverDnsQuery,
WeaverDnsResponse,
WeaverPrivilegeChange,
WeaverMemoryExec,
WeaverSandboxEscape,
```

### 4.2 Event Payload Structure

All events include a common header:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaverAuditEvent {
    pub weaver_id: String,
    pub org_id: String,
    pub owner_user_id: String,
    pub timestamp_ns: u64,
    pub pid: u32,
    pub tid: u32,
    pub comm: String,         // Process name (16 chars max)
    pub event_type: WeaverAuditEventType,
    pub details: serde_json::Value,
}
```

### 4.3 Event Details Examples

**WeaverProcessExec:**
```json
{
  "path": "/usr/bin/git",
  "argv": ["git", "clone", "https://github.com/org/repo"],
  "envp": ["HOME=/home/loom", "PATH=...", "[REDACTED:github-pat]"],
  "cwd": "/workspace",
  "uid": 1000,
  "gid": 1000,
  "ppid": 1234
}
```

**WeaverNetworkConnect:**
```json
{
  "socket_id": 42,
  "family": "AF_INET",
  "protocol": "TCP",
  "remote_ip": "140.82.112.4",
  "remote_port": 443,
  "hostname": "github.com",
  "success": true
}
```

**WeaverFileWrite:**
```json
{
  "path": "/workspace/src/main.rs",
  "operation": "write",
  "flags": ["O_WRONLY", "O_TRUNC"],
  "bytes": 1024,
  "fd": 5
}
```

---

## 5. Filtering Strategy

### 5.1 Kernel-Side Filtering

Minimal filtering in eBPF for performance:

- **Scope**: Only capture events from the pod's cgroup/PID namespace
- **Syscall selection**: Only attach to the syscalls listed above
- **Flag filtering**: For `openat`, check flags to distinguish read/write

### 5.2 Userspace Filtering

Detailed filtering in Rust for flexibility:

**Always capture:**
- All `execve`/`execveat` events
- All network events (socket, connect, bind, listen, accept)
- All privilege change events
- All sandbox escape attempts
- File writes outside `/workspace`
- Any access to `/dev`, `/proc/*/mem`, `/proc/*/exe`, `/proc/*/environ`

**Filter out (low signal):**
- File reads from `/usr/**`, `/lib/**`, `/lib64/**`
- File reads from `/etc/ld.so.cache`, `/etc/localtime`, `/etc/nsswitch.conf`
- File reads from `/dev/urandom`, `/dev/random`, `/dev/null`, `/dev/zero`
- File reads from `/proc/self/status`, `/proc/self/stat`, `/proc/self/maps`

**Sample (high volume):**
- File reads under `/workspace/**` (sample 1 in 10, or rate limit)

### 5.3 Configuration

Filtering rules configurable via environment variables:

```
LOOM_AUDIT_FILTER_PATHS_IGNORE=/usr,/lib,/lib64
LOOM_AUDIT_FILTER_PATHS_ALWAYS=/dev,/proc/*/mem,/proc/*/exe
LOOM_AUDIT_FILTER_READ_SAMPLE_RATE=10
```

---

## 6. Connection State Tracking

### 6.1 Overview

Track socket lifecycle to correlate `send`/`recv` with connection metadata:

```
socket() → connect()/bind() → send/recv → close()
    │            │                │           │
    ▼            ▼                ▼           ▼
SocketCreate  ConnectEvent    SendEvent   CloseEvent
    │            │                │           │
    └────────────┴────────────────┴───────────┘
                          │
                          ▼
              Userspace Connection Tracker
              (fd→socket map, socket→metadata)
```

### 6.2 Data Structures

**FD Table** (per-PID file descriptor tracking):

```rust
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct FdKey {
    pid: u32,
    fd: i32,
}

struct FdState {
    socket_id: SocketId,
    generation: u64,    // Incremented on fd reuse
    last_updated_ns: u64,
}
```

**Socket Table** (connection metadata):

```rust
struct SocketState {
    socket_id: SocketId,
    pid_creator: u32,
    protocol: Protocol,
    family: AddressFamily,
    local_addr: Option<SocketAddr>,
    remote_addr: Option<SocketAddr>,
    hostname: Option<String>,  // From DNS cache
    first_seen_ns: u64,
    last_seen_ns: u64,
    is_closed: bool,
    refcount: u32,
}
```

### 6.3 DNS Cache

**Structure:**

```rust
struct DnsKey {
    family: AddressFamily,
    ip: IpAddr,
}

struct DnsEntry {
    hostname: String,
    expiry_ns: u64,
    last_used_ns: u64,
}
```

**Policy:**
- TTL: Use DNS response TTL, clamped to 5s min, 600s max
- Default TTL: 60s if missing
- Max entries: 10,000
- Eviction: Expired first, then LRU

### 6.4 Handling Edge Cases

**FD Reuse:**
- Track `generation` per `(pid, fd)`
- On `socket()`: Create new entry or bump generation
- On `close()`: Mark closed, don't delete immediately

**Fork:**
- On `ForkEvent`: Copy parent's FD table to child
- Increment socket refcounts for shared fds

**SCM_RIGHTS (fd passing):**
- Parse `sendmsg`/`recvmsg` control messages
- Transfer socket references to receiving process
- Increment refcounts appropriately

---

## 7. Local Buffer

### 7.1 Behavior

When loom-server is unreachable:
1. Write events to `/tmp/audit-buffer.jsonl`
2. Continue capturing (never block eBPF pipeline)
3. On server reconnect, flush buffer before new events

### 7.2 Format

JSON Lines (one JSON object per line):

```jsonl
{"weaver_id":"abc123","timestamp_ns":1234567890,"event_type":"WeaverProcessExec",...}
{"weaver_id":"abc123","timestamp_ns":1234567891,"event_type":"WeaverFileWrite",...}
```

### 7.3 Limits

| Setting | Value |
|---------|-------|
| Max buffer size | 256 MB |
| Overflow policy | Drop oldest events |
| Flush batch size | 1000 events |
| Flush interval | 100ms (same as normal batching) |

### 7.4 Recovery

On startup:
1. Check for existing buffer file
2. If exists, read and flush to server
3. Delete buffer file after successful flush
4. Begin normal operation

---

## 8. Internal API

### 8.1 Endpoint

```
POST /internal/weaver-audit/events
Authorization: Bearer <weaver-svid>
Content-Type: application/json

{
  "weaver_id": "018f6b2a-...",
  "org_id": "org-uuid",
  "events": [
    { "timestamp_ns": 1234567890, "event_type": "WeaverProcessExec", ... },
    { "timestamp_ns": 1234567891, "event_type": "WeaverFileWrite", ... }
  ]
}
```

### 8.2 Response

```json
{
  "accepted": 42,
  "rejected": 0
}
```

### 8.3 Authentication

Use existing SPIFFE flow (same as `loom-weaver-secrets`):
1. Read K8s service account token from `/var/run/secrets/kubernetes.io/serviceaccount/token`
2. Exchange for weaver SVID via `POST /internal/weaver-auth/token`
3. Use SVID as Bearer token for audit API

---

## 9. Configuration

### 9.1 Environment Variables

Injected by loom-server-weaver provisioner:

| Variable | Description | Example |
|----------|-------------|---------|
| `LOOM_WEAVER_ID` | Weaver identifier | `018f6b2a-3b4c-...` |
| `LOOM_ORG_ID` | Organization ID | `org-uuid` |
| `LOOM_OWNER_USER_ID` | Owner user ID | `user-uuid` |
| `LOOM_SERVER_URL` | Server base URL | `https://loom.example.com` |

### 9.2 Sidecar Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `LOOM_AUDIT_BATCH_INTERVAL_MS` | `100` | Batch flush interval |
| `LOOM_AUDIT_BUFFER_MAX_BYTES` | `268435456` | 256 MB buffer limit |
| `LOOM_AUDIT_METRICS_PORT` | `9090` | Prometheus metrics port |
| `LOOM_AUDIT_HEALTH_PORT` | `9091` | Health endpoint port |
| `LOOM_AUDIT_LOG_LEVEL` | `info` | Logging level |

---

## 10. Security Context

### 10.1 Sidecar Container

Hardened configuration (not privileged):

```yaml
securityContext:
  runAsUser: 0
  runAsNonRoot: false
  readOnlyRootFilesystem: true
  allowPrivilegeEscalation: false
  capabilities:
    drop:
      - ALL
    add:
      - BPF
      - PERFMON
  seccompProfile:
    type: Unconfined  # Required for bpf() syscall
```

### 10.2 Pod Configuration

```yaml
spec:
  shareProcessNamespace: true
  
  initContainers: []  # None needed with native sidecars
  
  containers:
    - name: weaver
      # ... main weaver container
      
    - name: audit-sidecar
      image: ghcr.io/ghuntley/loom-audit-sidecar:latest
      restartPolicy: Always  # Native sidecar (K8s 1.28+)
      # ... security context above
```

### 10.3 Startup Ordering

Using Kubernetes 1.28+ native sidecar containers:

1. Sidecar starts first (has `restartPolicy: Always`)
2. Sidecar loads eBPF programs
3. Sidecar exposes health endpoint
4. Weaver container starts (depends on sidecar readiness)
5. If sidecar crashes, weaver is terminated

---

## 11. Prometheus Metrics

### 11.1 Event Counters

```
loom_weaver_audit_events_captured_total{event_type="exec|file_write|..."}
loom_weaver_audit_events_sent_total{event_type="..."}
loom_weaver_audit_events_dropped_total{stage="ebpf|ring_buffer|...",reason="overflow|..."}
loom_weaver_audit_events_buffered_total{event_type="..."}
```

### 11.2 Pipeline Metrics

```
loom_weaver_audit_batches_sent_total{outcome="success|failure"}
loom_weaver_audit_batch_events (histogram)
loom_weaver_audit_batch_size_bytes (histogram)
loom_weaver_audit_pipeline_events_in_flight (gauge)
```

### 11.3 Buffer Metrics

```
loom_weaver_audit_buffer_events (gauge)
loom_weaver_audit_buffer_bytes (gauge)
loom_weaver_audit_buffer_oldest_event_age_seconds (gauge)
```

### 11.4 eBPF Metrics

```
loom_weaver_audit_ebpf_programs_attached{program="exec|file|network|..."}
loom_weaver_audit_ebpf_ring_buffer_dropped_events_total
loom_weaver_audit_ebpf_ring_buffer_utilization_ratio (gauge)
```

### 11.5 HTTP Client Metrics

```
loom_weaver_audit_http_requests_total{endpoint="events",code="2xx|4xx|5xx|timeout"}
loom_weaver_audit_http_request_duration_seconds (histogram)
loom_weaver_audit_http_retries_total{reason="network_error|timeout|server_error"}
```

---

## 12. Health Endpoint

### 12.1 Endpoint

```
GET /health
```

### 12.2 Response

```json
{
  "status": "healthy",
  "components": {
    "ebpf": {
      "status": "healthy",
      "programs_attached": 5,
      "programs_expected": 5
    },
    "server": {
      "status": "healthy",
      "last_successful_send": "2025-01-03T12:34:56Z",
      "buffered_events": 0
    },
    "buffer": {
      "status": "healthy",
      "size_bytes": 0,
      "utilization_percent": 0
    }
  }
}
```

### 12.3 Status Codes

| Status | HTTP Code | Meaning |
|--------|-----------|---------|
| healthy | 200 | All systems operational |
| degraded | 200 | Buffering events, server unreachable |
| unhealthy | 503 | eBPF not loaded or critical failure |

---

## 13. NixOS Configuration

### 13.1 Module Options

Add to `infra/nixos-modules/loom-server.nix`:

```nix
weaver.audit = {
  enable = mkEnableOption "eBPF audit sidecar for weavers";
  
  batchIntervalMs = mkOption {
    type = types.int;
    default = 100;
    description = "Event batch interval in milliseconds.";
  };
  
  bufferMaxBytes = mkOption {
    type = types.int;
    default = 268435456;  # 256 MB
    description = "Maximum local buffer size in bytes.";
  };
  
  metricsPort = mkOption {
    type = types.port;
    default = 9090;
    description = "Prometheus metrics port.";
  };
  
  healthPort = mkOption {
    type = types.port;
    default = 9091;
    description = "Health endpoint port.";
  };
};
```

### 13.2 Container Image

Add to `flake.nix`:

```nix
loom-audit-sidecar-c2n = (rustPkgs.workspace.loom-weaver-audit-sidecar {});

audit-sidecar-image = pkgsWithCargo2nix.callPackage ./infra/pkgs/audit-sidecar-image.nix {
  loom-audit-sidecar = loom-audit-sidecar-c2n;
};
```

---

## 14. Testing Strategy

### 14.1 Unit Tests

- Event serialization/deserialization
- Filtering logic (path matching, sampling)
- Connection tracker state machine
- DNS cache TTL and eviction
- Buffer overflow handling

### 14.2 Property-Based Tests (proptest)

- FD table consistency under random fork/close sequences
- DNS cache never exceeds max entries
- Buffer size never exceeds limit
- Event ordering preserved through pipeline

### 14.3 Integration Tests

- eBPF program loading (requires CAP_BPF)
- End-to-end event capture and delivery
- Server reconnection and buffer flush
- Metrics accuracy

---

## 15. Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Create `loom-weaver-ebpf-common` crate (shared types)
- [ ] Create `loom-weaver-ebpf` crate (eBPF programs)
- [ ] Create `loom-weaver-audit-sidecar` crate (userspace)
- [ ] Basic exec/file/network event capture

### Phase 2: Event Pipeline
- [ ] Add new `AuditEventType` variants
- [ ] Implement internal API endpoint
- [ ] HTTP client with batching
- [ ] Local file buffer

### Phase 3: Advanced Features
- [ ] Connection state tracking
- [ ] DNS hostname correlation
- [ ] Prometheus metrics
- [ ] Health endpoint

### Phase 4: Integration
- [ ] Update weaver pod spec with sidecar
- [ ] Container image build
- [ ] NixOS module configuration
- [ ] Documentation

---

## 16. Future Considerations

### 16.1 Potential Extensions

- File content sampling (first N bytes for forensics)
- LSM hooks for policy enforcement
- Per-org/per-user audit policies
- Real-time alerting for critical events
- Integration with SIEM systems

### 16.2 Not Planned

- Multi-container per-pod auditing
- Host-level auditing (DaemonSet)
- Kernel module loading
- Custom seccomp profiles per weaver
