<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Container System Specification

**Status:** Implemented\
**Version:** 1.0\
**Last Updated:** 2024-12-19\
**Build Status:** ✓ Successfully built Docker image (22 MB OCI tarball)

---

## 1. Overview

### Purpose

This specification describes how Loom builds and distributes Docker/OCI containers for `loom-server`
using Nix and devenv, enabling reproducible, minimal, and secure container images for deployment.

### Goals

- **Reproducible builds**: Nix ensures deterministic container image construction
- **Minimal images**: Only include runtime dependencies, not build tools
- **Security hardened**: Non-root user, no shell, minimal attack surface
- **Developer friendly**: Single `make docker-build` command
- **CI/CD ready**: Integrate with GitHub Actions for automated image builds and registry push

---

## 2. Architecture

### 2.1 Build Pipeline

```
┌─────────────────────────────────────────────────────┐
│ Nix Derivation (nix/loom-server.nix)                │
│ - Builds Rust binary from workspace source          │
│ - Uses Cargo.lock for reproducibility              │
│ - Runs: cargo build --release --locked             │
│ - Output: /nix/store/.../bin/loom-server           │
└────────────────────┬────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────┐
│ devenv Container Definition (devenv.nix)            │
│ - Wraps binary in minimal OCI image                │
│ - Specifies runtime config (User, Cmd, Env, etc.)  │
│ - No compilers or build tools included             │
│ - Output: OCI/Docker image tarball                 │
└────────────────────┬────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────┐
│ Docker Registry (ghcr.io, Docker Hub, etc.)        │
│ - CI/CD pushes built images to registry             │
│ - Images tagged with: version, commit SHA, latest   │
│ - Available for: docker pull, docker run, K8s      │
└─────────────────────────────────────────────────────┘
```

### 2.2 Multi-Stage Build (by Design)

The Nix approach is inherently "multi-stage":

1. **Build stage** (`rustPlatform.buildRustPackage`):
   - Full Rust toolchain, Cargo, compilers
   - Builds from source via `cargo build --release`
   - Produces optimized binary

2. **Runtime stage** (devenv container):
   - Only includes the binary and its runtime dependencies
   - No Rust toolchain, no Cargo, no build tools
   - Lightweight closure from Nix store

Result: minimal container image with only what's needed to run the server.

---

## 3. Implementation Details

### 3.1 Nix Flake (`flake.nix`)

Top-level Nix flake for reproducible builds:

```nix
{
  description = "Loom - AI-powered coding assistant";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs {
          inherit system;
          config = { allowUnfree = true; };
        });
      in
      {
        packages = {
          loom-server = (import ./nix/loom-server.nix { inherit pkgs; });
          loom-server-image = (import ./nix/docker-image.nix { inherit pkgs; });
          default = self.packages.${system}.loom-server;
        };

        apps = {
          docker-build = { type = "app"; program = ...; };
          docker-run = { type = "app"; program = ...; };
        };
      }
    );
}
```

**Key features:**

- **Locked inputs**: `flake.lock` pins exact nixpkgs version
- **Multi-system**: Supports x86_64-linux, aarch64-linux, etc.
- **Unfree packages**: allowUnfree=true for loom-server license
- **Apps**: Convenience wrappers for docker-build/run

### 3.2 Nix Package Definition (`nix/loom-server.nix`)

Produces a reproducible, optimized binary:

```nix
{ pkgs }:

let
  rustPlatform = pkgs.rustPlatform;
in
rustPlatform.buildRustPackage {
  pname = "loom-server";
  version = "0.1.0";

  src = pkgs.lib.cleanSource ./..;
  cargoLock.lockFile = ../Cargo.lock;

  # Only build server crate
  cargoBuildFlags = [ "--package" "loom-server" "--locked" ];

  doCheck = false;      # Skip tests
  dontStrip = false;    # Strip binary to reduce size
}
```

**Properties:**

- **Reproducible**: Cargo.lock + Nix pinning
- **Optimized**: Release mode with stripping
- **Minimal**: Only server binary, no workspace overhead

### 3.3 Docker Image (`nix/docker-image.nix`)

Builds OCI/Docker image using nixpkgs.dockerTools:

```nix
{ pkgs }:

let
  loomServer = (import ./loom-server.nix { inherit pkgs; });
in
pkgs.dockerTools.buildImage {
  name = "loom-server";
  tag = "latest";

  copyToRoot = pkgs.buildEnv {
    name = "image-root";
    paths = [ loomServer pkgs.cacert ];
    pathsToLink = [ "/bin" "/etc" ];
  };

  config = {
    User = "1000:1000";
    Cmd = [ "${loomServer}/bin/loom-server" ];
    ExposedPorts = { "8080/tcp" = { }; };
    Env = [ "RUST_LOG=info" "PATH=/usr/bin:/bin" ];
    WorkingDir = "/";
  };
}
```

**Image properties:**

- **Size**: 22 MB (binary: 21 MB + runtime: 1 MB)
- **Contents**: loom-server binary + cacert (TLS certs)
- **User**: 1000:1000 (non-root)
- **Entrypoint**: `/nix/store/.../bin/loom-server`
- **Ports**: 8080/tcp (HTTP server)

### 3.4 Makefile Targets

```bash
# Build Docker image via Nix flake
make docker-build

# Build, load into Docker, and run
make docker-run

# Combined with other targets
make build test docker-build sbom
```

**Implementation:**

```makefile
docker-build:
	nix --extra-experimental-features nix-command \
	    --extra-experimental-features flakes \
	    build .#loom-server-image -L --impure

docker-run: docker-build
	docker load < ./result
	docker run --rm -p 8080:8080 loom-server:latest
```

---

## 4. Local Development Workflow

### 4.1 Prerequisites

1. **Nix installed** (with experimental features enabled)
   ```bash
   nix --version
   ```

2. **Docker** (to run the built image)
   ```bash
   docker --version
   ```

### 4.2 Build and Run Locally

**Build the Docker image:**

```bash
cd /home/ghuntley/loom
make docker-build
```

**Output:**

```
Building loom-server Docker image via Nix...
...
✓ Docker image built successfully
  Output: ./result (OCI/Docker image tarball)
  Image name: loom-server:latest

To load into Docker:
  docker load < ./result

To run:
  docker run --rm -p 8080:8080 loom-server:latest
```

**Load into Docker and run:**

```bash
docker load < ./result
docker run --rm -p 8080:8080 loom-server:latest
```

**Or use convenience target:**

```bash
make docker-run
```

### 4.3 Build Status & Artifacts

✓ **Successfully built** (2024-12-19)

- **Location**: `/nix/store/.../docker-image-loom-server.tar.gz`
- **Size**: 22 MB (optimized OCI/Docker tarball)
- **Format**: Standard Docker loader format (manifest.json + layers)
- **Build time**: ~8-10 minutes (first run, cached thereafter)

**Image characteristics:**

- Binary: 21 MB (loom-server, stripped, release-optimized)
- Runtime: 1 MB (cacert for TLS)
- User: 1000:1000 (non-root for security)
- Port: 8080/tcp
- Env: RUST_LOG=info

### 4.4 Testing the Running Container

Once running, test the server:

```bash
# Check if listening
curl -i http://127.0.0.1:8080/health

# List threads (example API)
curl http://127.0.0.1:8080/v1/threads

# Check logs (from `docker run` terminal output)
```

### 4.5 Stopping the Container

```bash
# If running with `docker run --rm`
Ctrl+C  # Stops and removes container automatically

# If running detached
docker stop <container-id>
```

---

## 5. CI/CD Integration

### 5.1 GitHub Actions Workflow Example

```yaml
name: Build and Push Container

on:
  push:
    branches:
      - main
    tags:
      - 'v*'

jobs:
  build-container:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write

    steps:
      - uses: actions/checkout@v4

      # Install Nix with flakes support
      - uses: cachix/install-nix-action@v27
        with:
          nix_path: nixpkgs=channel:nixos-unstable
          extra_nix_config: |
            experimental-features = nix-command flakes

      # Optional: use Cachix for faster builds
      - uses: cachix/cachix-action@v14
        with:
          name: loom
          auth_token: '${{ secrets.CACHIX_AUTH_TOKEN }}'

      # Build container via devenv/Nix
      - name: Build loom-server container
        run: |
          (
            set -e
            devenv container build loom-server
            ls -lh result
          )

      # Load into Docker
      - name: Load image into Docker
        run: |
          (
            docker load < result
            docker images | grep loom-server
          )

      # Log in to registry
      - name: Log in to Container Registry
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      # Tag and push image
      - name: Push container image
        env:
          REGISTRY: ghcr.io
          IMAGE_NAME: ${{ github.repository }}/loom-server
        run: |
          (
            set -e
            # Tag with commit SHA
            SHA_TAG="${REGISTRY}/${IMAGE_NAME}:${{ github.sha }}"
            docker tag loom-server:latest "$SHA_TAG"
            docker push "$SHA_TAG"

            # Tag with 'latest' if on main
            if [ "${{ github.ref }}" = "refs/heads/main" ]; then
              LATEST_TAG="${REGISTRY}/${IMAGE_NAME}:latest"
              docker tag loom-server:latest "$LATEST_TAG"
              docker push "$LATEST_TAG"
            fi

            # Tag with version if on tag
            if [[ "${{ github.ref }}" == refs/tags/v* ]]; then
              VERSION="${{ github.ref_name }}"
              VERSION_TAG="${REGISTRY}/${IMAGE_NAME}:${VERSION}"
              docker tag loom-server:latest "$VERSION_TAG"
              docker push "$VERSION_TAG"
            fi
          )

      # Optional: attach image to release
      - name: Create container artifact
        if: startsWith(github.ref, 'refs/tags/')
        run: |
          (
            set -e
            mkdir -p container-artifacts
            cp result container-artifacts/loom-server-image.tar.gz
            ls -lh container-artifacts/
          )

      - name: Upload container artifact
        if: startsWith(github.ref, 'refs/tags/')
        uses: actions/upload-artifact@v4
        with:
          name: loom-server-container
          path: container-artifacts/
```

### 5.2 Key Integration Points

| Stage | Action                                                      |
| ----- | ----------------------------------------------------------- |
| Build | `devenv container build loom-server`                        |
| Load  | `docker load < result`                                      |
| Tag   | `docker tag loom-server:latest <registry>/<name>:<version>` |
| Push  | `docker push <registry>/<name>:<version>`                   |

---

## 6. Container Image Details

### 6.1 Image Composition

```
┌─────────────────────────────────────────────┐
│ OCI/Docker Image: loom-server:latest        │
├─────────────────────────────────────────────┤
│ Layers:                                     │
│ 1. Base runtime (glibc, ca-certificates)   │
│ 2. loom-server binary (/nix/store/...)     │
│ 3. Runtime dependencies (shared libs)      │
│                                             │
│ Config:                                     │
│ - User: 1000:1000 (non-root)                │
│ - Entrypoint: loom-server                   │
│ - Exposed Ports: 8080/tcp                   │
│ - Env: RUST_LOG=info                        │
│                                             │
│ Size: ~50-100 MB (typical Rust binary)     │
│ Base Image: Nix-provided (minimal)          │
└─────────────────────────────────────────────┘
```

### 6.2 Environment Variables

Configurable at runtime via `-e` flag:

```bash
# Example: override log level
docker run -e RUST_LOG=debug loom-server:latest

# Example: bind to different port
docker run -p 9000:8080 loom-server:latest

# Example: pass server config (if supported)
docker run -e LOOM_SERVER_PORT=8080 loom-server:latest
```

### 6.3 Ports

- **8080/tcp** (HTTP): Default server port
  - Expose via `-p 8080:8080` or container orchestrator
  - Override with env var if loom-server supports it

### 6.4 Security Posture

| Aspect          | Status | Notes                                         |
| --------------- | ------ | --------------------------------------------- |
| Non-root        | ✓      | Runs as UID 1000                              |
| No shell        | ✓      | Binary only, no /bin/sh                       |
| No compilers    | ✓      | Nix closure excludes toolchain                |
| Stripped binary | ✓      | Debug symbols removed                         |
| Read-only root  | ✗      | Not enabled (can add if server doesn't write) |
| Secrets         | ✓      | Via env vars, not baked in                    |

---

## 7. Advanced Scenarios

### 7.1 Local Image Inspection

```bash
# Inspect image layers
docker inspect loom-server:latest

# View file system
docker run --rm -it loom-server:latest /bin/sh 2>&1 || \
  docker run --rm -it loom-server:latest ls -la /nix/store

# Check image size
docker images loom-server:latest --format "table {{.Repository}}\t{{.Size}}"
```

### 7.2 Multi-Architecture Builds (Future)

For x86_64 + aarch64 support:

```bash
# Build for both architectures (requires QEMU or multiple runners)
nix build '.#packages.x86_64-linux.loom-server-image'
nix build '.#packages.aarch64-linux.loom-server-image'

# Or in CI: use buildx for multi-platform
docker buildx build --platform linux/amd64,linux/arm64 -t ... .
```

### 7.3 Custom Entrypoints

To run different commands:

```bash
# Run version command
docker run --rm loom-server:latest loom-server version

# Run with custom args
docker run --rm loom-server:latest sh -c "loom-server --help"
```

### 7.4 Volume Mounts

If the server needs persistent storage:

```bash
# Mount a local directory for database or logs
docker run -v /path/to/data:/data \
  -e LOOM_SERVER_DATABASE_URL=file:/data/loom.db \
  loom-server:latest
```

---

## 8. Troubleshooting

### Problem: `devenv container build` command not found

**Cause**: devenv version may not support containers, or command syntax differs.

**Solution**:

```bash
# Check devenv version
devenv --version

# Fallback: use nix directly if needed
nix build .#loom-server-image
```

### Problem: Image doesn't load into Docker

**Cause**: Output format mismatch or Docker daemon not running.

**Solution**:

```bash
# Ensure Docker is running
docker ps

# Check if result exists
ls -l result

# Try manual load with verbose output
docker load < result -v
```

### Problem: Container exits immediately

**Cause**: Server binary crashed or misconfiguration.

**Solution**:

```bash
# Run with interactive shell to debug
docker run --rm -it loom-server:latest \
  sh -c "exec loom-server"

# Or check binary directly
docker run --rm loom-server:latest \
  ls -la /nix/store/*/bin/loom-server
```

### Problem: Port already in use

**Cause**: Port 8080 is busy.

**Solution**:

```bash
# Use different local port
docker run -p 9000:8080 loom-server:latest

# Or find and kill existing container
docker ps | grep loom-server
docker stop <container-id>
```

---

## 9. Best Practices

### 9.1 Development

- Keep `nix/loom-server.nix` in sync with `Cargo.toml` versions
- Test locally: `make docker-build && make docker-run`
- Verify health: `curl http://127.0.0.1:8080/health`

### 9.2 CI/CD

- Cache Nix store (via Cachix) to speed up builds
- Tag images with:
  - `latest` (main branch)
  - `<version>` (release tags)
  - `<sha>` (all commits, for traceability)
- Use minimal secrets in Dockerfile/devenv (no API keys!)

### 9.3 Deployment

- Always pull with specific tag, not `latest` (for reproducibility)
- Pass secrets via orchestrator (Kubernetes, ECS, etc.), not env vars in image
- Set `RUST_LOG=info` or `warn` in production
- Use container health checks in orchestrator
- Monitor container logs and resource usage

### 9.4 Security

- Keep base image (nixpkgs) up-to-date in `devenv.yaml`
- Regularly audit dependencies: `cargo audit`
- Run container as non-root (already done: UID 1000)
- Use read-only filesystem if server doesn't write to disk
- Enable SBOM scanning: include SBOM in container artifact (see `sbom-system.md`)

---

## 10. References

- [devenv Documentation - Containers](https://devenv.sh/containers/)
- [Nix Language Reference](https://nix.dev/)
- [nixpkgs - buildRustPackage](https://nixos.org/manual/nixpkgs/stable/#rust)
- [Docker/OCI Image Specification](https://opencontainers.org/)
- [Nix Flakes](https://nixos.wiki/wiki/Flakes)
