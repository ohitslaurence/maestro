<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Distribution System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2024-12-17

---

## 1. Overview

### Purpose

This specification describes how Loom CLI binaries are built, distributed, and served to enable
self-updating across platforms.

### Goals

- **Multi-platform support**: Build for Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows
  (x86_64)
- **Self-update**: CLI can update itself from the server
- **Automated builds**: CI/CD builds all platforms automatically
- **Version tracking**: Build info embedded in binaries

---

## 2. Platform Naming Convention

Platform strings follow the format `{os}-{arch}`:

| Platform            | Rust Target                 | Platform String  |
| ------------------- | --------------------------- | ---------------- |
| Linux x64           | `x86_64-unknown-linux-gnu`  | `linux-x86_64`   |
| Linux ARM64         | `aarch64-unknown-linux-gnu` | `linux-aarch64`  |
| macOS Intel         | `x86_64-apple-darwin`       | `macos-x86_64`   |
| macOS Apple Silicon | `aarch64-apple-darwin`      | `macos-aarch64`  |
| Windows x64         | `x86_64-pc-windows-msvc`    | `windows-x86_64` |

The platform string is derived at compile time:

```rust
concat!(env!("CARGO_CFG_TARGET_OS"), "-", env!("CARGO_CFG_TARGET_ARCH"))
```

---

## 3. Build System

### 3.1 Local Build Script

The `scripts/build-cli-binaries.sh` script builds CLI binaries for specified platforms:

```bash
# Build all platforms (requires cross-compilation toolchains)
./scripts/build-cli-binaries.sh

# Build specific platform
./scripts/build-cli-binaries.sh linux-x86_64

# Build multiple platforms
./scripts/build-cli-binaries.sh linux-x86_64 macos-aarch64
```

Output binaries are placed in `$LOOM_SERVER_BIN_DIR` (default: `./bin/`).

### 3.2 Build Info Embedding

Each CLI binary embeds build information via `shadow-rs`:

- Package version (from Cargo.toml)
- Git commit SHA (short)
- Build timestamp (RFC3339)
- Target platform

This info is displayed by `loom version` and sent as HTTP headers.

---

## 4. Server Distribution

### 4.1 Binary Serving

The loom-server serves CLI binaries at `/bin/{platform}`:

| Endpoint                  | File                                  |
| ------------------------- | ------------------------------------- |
| `GET /bin/linux-x86_64`   | `$LOOM_SERVER_BIN_DIR/linux-x86_64`   |
| `GET /bin/linux-aarch64`  | `$LOOM_SERVER_BIN_DIR/linux-aarch64`  |
| `GET /bin/macos-x86_64`   | `$LOOM_SERVER_BIN_DIR/macos-x86_64`   |
| `GET /bin/macos-aarch64`  | `$LOOM_SERVER_BIN_DIR/macos-aarch64`  |
| `GET /bin/windows-x86_64` | `$LOOM_SERVER_BIN_DIR/windows-x86_64` |

### 4.2 Server Configuration

| Environment Variable  | Default | Description                            |
| --------------------- | ------- | -------------------------------------- |
| `LOOM_SERVER_BIN_DIR` | `./bin` | Directory containing platform binaries |

### 4.3 Directory Layout

```
loom-server-deployment/
├── loom-server          # Server binary
└── bin/
    ├── linux-x86_64     # Linux x64 CLI
    ├── linux-aarch64    # Linux ARM64 CLI
    ├── macos-x86_64     # macOS Intel CLI
    ├── macos-aarch64    # macOS Apple Silicon CLI
    └── windows-x86_64   # Windows x64 CLI
```

---

## 5. Self-Update Flow

### 5.1 Update Command

```bash
loom update
```

### 5.2 Update Process

1. CLI determines current platform string
2. Constructs URL: `{base_url}/bin/{platform}`
3. Downloads binary with version headers
4. Writes to temporary file
5. Sets executable permissions (Unix)
6. Atomically replaces current binary (backup created as `.old`)

### 5.3 Update Configuration

| Environment Variable   | Description                                             |
| ---------------------- | ------------------------------------------------------- |
| `LOOM_UPDATE_BASE_URL` | Base URL for updates (e.g., `https://loom.example.com`) |
| `LOOM_THREAD_SYNC_URL` | Fallback: derives base URL from sync URL                |

---

## 6. CI/CD Pipeline

### 6.1 Workflow Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        GitHub Actions                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐              │
│  │ build-cli   │  │ build-cli   │  │ build-cli   │  ...         │
│  │ linux-x86   │  │ macos-arm   │  │ windows     │              │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘              │
│         │                │                │                      │
│         └────────────────┼────────────────┘                      │
│                          ▼                                       │
│                   ┌─────────────┐                                │
│                   │   package   │                                │
│                   │   bundle    │                                │
│                   └──────┬──────┘                                │
│                          │                                       │
│                          ▼                                       │
│                   ┌─────────────┐                                │
│                   │   release   │                                │
│                   │   upload    │                                │
│                   └─────────────┘                                │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 6.2 Build Matrix

| Platform       | Runner               | Target                      |
| -------------- | -------------------- | --------------------------- |
| linux-x86_64   | `ubuntu-latest`      | `x86_64-unknown-linux-gnu`  |
| linux-aarch64  | `ubuntu-24.04-arm64` | `aarch64-unknown-linux-gnu` |
| macos-x86_64   | `macos-13`           | `x86_64-apple-darwin`       |
| macos-aarch64  | `macos-14`           | `aarch64-apple-darwin`      |
| windows-x86_64 | `windows-latest`     | `x86_64-pc-windows-msvc`    |

### 6.3 Artifacts

| Artifact              | Contents                          | Retention |
| --------------------- | --------------------------------- | --------- |
| `cli-{platform}`      | Single platform CLI binary        | 7 days    |
| `server-linux-x86_64` | Server binary                     | 7 days    |
| `loom-bundle`         | Server + all CLI binaries tarball | 30 days   |

---

## 7. User-Agent Header

All HTTP requests from CLI to server include a User-Agent header:

```
User-Agent: loom/{platform}/{git_sha}
```

Example: `loom/linux-x86_64/abc1234`

This is set automatically by `loom_http::new_client()` or `loom_http::builder()` and enables:

- Server-side version analytics
- Compatibility checks (future)
- Targeted update recommendations (future)

---

## 8. Docker/OCI Container Images

### 8.1 Container Build

Production-ready Docker containers for `loom-server` are built via Nix/devenv:

- **Build tool**: devenv + Nix (reproducible, minimal, secure)
- **Output**: OCI/Docker image
- **Base**: Nix-provided minimal runtime
- **Ports**: 8080/tcp (HTTP)

See [container-system.md](./container-system.md) for full details.

### 8.2 Container Distribution

Containers are published to registry (e.g., ghcr.io) with tags:

| Tag      | Audience            | Availability    |
| -------- | ------------------- | --------------- |
| `latest` | End users           | On main branch  |
| `v0.1.0` | Release subscribers | On version tags |
| `<sha>`  | CI/traceability     | All commits     |

---

## 9. Software Bill of Materials (SBOM)

### 9.1 SBOM Generation

As part of the release pipeline, SBOMs are generated for supply chain transparency:

- **Tool:** `cargo-sbom` (v0.10.0, pinned)
- **Formats:** SPDX JSON 2.3, CycloneDX JSON 1.4
- **Coverage:** All Rust crates in the workspace
- **Location:** `target/sbom/loom.{spdx,cyclonedx}.json`

See [sbom-system.md](./sbom-system.md) for full details.

### 9.2 Release Distribution

SBOMs are attached to GitHub releases alongside binaries and container images:

| Artifact              | Format                                   |
| --------------------- | ---------------------------------------- |
| `loom.spdx.json`      | SPDX 2.3 (Linux Foundation standard)     |
| `loom.cyclonedx.json` | CycloneDX 1.4 (DevOps/container tooling) |

---

## 10. Future Considerations

### 10.1 Signed Binaries

- Sign binaries with a release key
- Verify signatures before applying updates

### 10.2 Delta Updates

- Download only changed bytes
- Reduce bandwidth for minor updates

### 10.3 Update Channels

- `stable`, `beta`, `nightly` channels
- `loom update --channel beta`

### 10.4 Version Manifest

- `GET /bin/manifest.json` returns available versions
- CLI can show "update available" notifications

### 10.5 Multi-Architecture Containers

- Build containers for x86_64 + aarch64
- Use `docker buildx` or separate Nix builds per architecture
- Push as manifest list for automatic platform selection

### 10.6 Container Image SBOMs

- Generate image-level SBOMs with `syft` for Docker containers
- Attach both source-level and image-level SBOMs to releases

### 10.7 Kubernetes Deployment

- Publish Helm charts for easy K8s deployment
- Include resource limits, liveness/readiness probes
- Support StatefulSets for server persistence
