<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Loom Specifications

Design documentation for Loom, an AI-powered coding agent in Rust.

## Core Architecture

| Spec | Code | Purpose |
|------|------|---------|
| [architecture.md](./architecture.md) | [crates/](../crates/) | Crate structure, server-side LLM proxy design |
| [state-machine.md](./state-machine.md) | [loom-core](../crates/loom-core/) | Agent state machine for conversation flow |
| [tool-system.md](./tool-system.md) | [loom-tools](../crates/loom-tools/) | Tool registry and execution framework |
| [thread-system.md](./thread-system.md) | [loom-thread](../crates/loom-thread/) | Thread persistence and sync |
| [streaming.md](./streaming.md) | [loom-llm-service](../crates/loom-llm-service/) | SSE streaming for real-time LLM responses |
| [error-handling.md](./error-handling.md) | [loom-core](../crates/loom-core/) | Error types using `thiserror` |

## Observability Suite

Loom's integrated observability platform: analytics, crash tracking, cron monitoring, and session health.

| Spec | Code | Purpose |
|------|------|---------|
| [analytics-system.md](./analytics-system.md) | [loom-analytics-core](../crates/loom-analytics-core/), [loom-analytics](../crates/loom-analytics/), [loom-server-analytics](../crates/loom-server-analytics/) | Product analytics with PostHog-style identity resolution |
| [analytics-implementation-plan.md](./analytics-implementation-plan.md) | — | Implementation checklist with citations |
| [crash-system.md](./crash-system.md) | [loom-crash-core](../crates/loom-crash-core/), [loom-crash](../crates/loom-crash/), [loom-crash-symbolicate](../crates/loom-crash-symbolicate/), [loom-server-crash](../crates/loom-server-crash/) | Crash analytics with source maps, regression detection |
| [crons-system.md](./crons-system.md) | [loom-crons-core](../crates/loom-crons-core/), [loom-crons](../crates/loom-crons/), [loom-server-crons](../crates/loom-server-crons/) | Cron/job monitoring with ping URLs and SDK check-ins |
| [sessions-system.md](./sessions-system.md) | [loom-sessions-core](../crates/loom-sessions-core/), [loom-server-sessions](../crates/loom-server-sessions/) | Session analytics with release health and crash-free rate |
| [observability-ui.md](./observability-ui.md) | [web/loom-web](../web/loom-web/) | Unified web UI for all observability features |

**Implementation Plan:** [hidave.md](../hidave.md) — Detailed phased implementation with citations

## LLM Integration

| Spec | Code | Purpose |
|------|------|---------|
| [llm-client.md](./llm-client.md) | [loom-llm-anthropic](../crates/loom-llm-anthropic/), [loom-llm-openai](../crates/loom-llm-openai/), [loom-server-llm-zai](../crates/loom-server-llm-zai/) | `LlmClient` trait for providers |
| [anthropic-oauth-pool.md](./anthropic-oauth-pool.md) | [loom-llm-anthropic](../crates/loom-llm-anthropic/) | Claude subscription pooling with failover |
| [anthropic-max-pool-management.md](./anthropic-max-pool-management.md) | [loom-server](../crates/loom-server/) | Admin UI for OAuth pool management |
| [claude-subscription-auth.md](./claude-subscription-auth.md) | [loom-llm-anthropic](../crates/loom-llm-anthropic/) | OAuth 2.0 PKCE for Claude Pro/Max |
| [server-query-phase-2.md](./server-query-phase-2.md) | [loom-llm-proxy](../crates/loom-llm-proxy/) | LLM query detection and context injection |
| [phase3_websocket_planning.md](./phase3_websocket_planning.md) | [loom-server](../crates/loom-server/) | WebSocket upgrade planning |

## Configuration & Security

| Spec | Code | Purpose |
|------|------|---------|
| [configuration-system.md](./configuration-system.md) | [loom-config](../crates/loom-config/) | Layered config with XDG paths |
| [configuration.md](./configuration.md) | [loom-common-config](../crates/loom-common-config/) | CLI args and env vars |
| [secret-system.md](./secret-system.md) | [loom-secret](../crates/loom-secret/) | `Secret<T>` wrapper for sensitive values |
| [redact-system.md](./redact-system.md) | [loom-redact](../crates/loom-redact/) | Secret detection using gitleaks patterns |
| [auth-abac-system.md](./auth-abac-system.md) | [loom-auth](../crates/loom-auth/), [loom-auth-*](../crates/) | OAuth, magic links, ABAC |
| [audit-system.md](./audit-system.md) | [loom-server-audit](../crates/loom-server-audit/) | Audit logging with SIEM integration |
| [feature-flags-system.md](./feature-flags-system.md) | [loom-flags-core](../crates/loom-flags-core/), [loom-flags](../crates/loom-flags/), [loom-server-flags](../crates/loom-server-flags/) | Feature flags, experiments, kill switches with SSE |

## Identity & Provisioning

| Spec | Code | Purpose |
|------|------|---------|
| [scim-system.md](./scim-system.md) | [loom-scim](../crates/loom-scim/), [loom-server-scim](../crates/loom-server-scim/) | RFC 7643/7644 SCIM for IdP user provisioning |

## Server & API

| Spec | Code | Purpose |
|------|------|---------|
| [api-documentation.md](./api-documentation.md) | [loom-server](../crates/loom-server/) | OpenAPI docs with Swagger UI |
| [health-check.md](./health-check.md) | [loom-server](../crates/loom-server/) | `/health` endpoint |
| [retry-strategy.md](./retry-strategy.md) | [loom-http](../crates/loom-http/) | Exponential backoff |
| [job-scheduler-system.md](./job-scheduler-system.md) | [loom-jobs](../crates/loom-jobs/) | Background job system |

## Terminal UI (TUI)

| Spec | Code | Purpose |
|------|------|---------|
| [tui-system.md](./tui-system.md) | [loom-tui-*](../crates/) | Ratatui 0.30 component system with visual snapshot testing |

## Editor Integration

| Spec | Code | Purpose |
|------|------|---------|
| [acp-system.md](./acp-system.md) | [loom-acp](../crates/loom-acp/) | Agent Client Protocol for editors |
| [vscode-extension.md](./vscode-extension.md) | [ide/vscode](../ide/vscode/) | VS Code extension via ACP |

## SCM (Git Hosting)

| Spec | Code | Purpose |
|------|------|---------|
| [scm-system.md](./scm-system.md) | [loom-scm](../crates/loom-server-scm/), [loom-scm-mirror](../crates/loom-scm-mirror/) | Git hosting, mirroring, webhooks, branch protection |

## Spool (Version Control)

| Spec | Code | Purpose |
|------|------|---------|
| [spool-system.md](./spool-system.md) | [loom-common-spool](../crates/loom-common-spool/), [loom-cli-spool](../crates/loom-cli-spool/) | jj-based VCS with tapestry naming (stitch, pin, tangle) |

## Git & Search

| Spec | Code | Purpose |
|------|------|---------|
| [git-metadata.md](./git-metadata.md) | [loom-git](../crates/loom-git/) | Git context tracking in threads |
| [auto-commit-system.md](./auto-commit-system.md) | [loom-auto-commit](../crates/loom-auto-commit/) | Auto-commit after tool execution |
| [search-system.md](./search-system.md) | [loom-thread](../crates/loom-thread/) | FTS5 search for threads |
| [web-search-system.md](./web-search-system.md) | [loom-google-cse](../crates/loom-google-cse/) | Google CSE integration |
| [github-app-system.md](./github-app-system.md) | [loom-github-app](../crates/loom-github-app/) | GitHub App for API access |

## Weaver (Remote Execution)

| Spec | Code | Purpose |
|------|------|---------|
| [weaver-provisioner.md](./weaver-provisioner.md) | [loom-server-weaver](../crates/loom-server-weaver/), [loom-server-k8s](../crates/loom-server-k8s/) | K8s pod provisioning |
| [weaver-cli.md](./weaver-cli.md) | [loom-cli](../crates/loom-cli/) | CLI for weaver management |
| [weaver-secrets-system.md](./weaver-secrets-system.md) | [loom-server-secrets](../crates/loom-server-secrets/), [loom-weaver-secrets](../crates/loom-weaver-secrets/) | SPIFFE-style identity and secret management |
| [wgtunnel-system.md](./wgtunnel-system.md) | [loom-wgtunnel-*](../crates/), [loom-server-wgtunnel](../crates/loom-server-wgtunnel/) | WireGuard tunnels with DERP relay for SSH/TCP access to weavers |
| [weaver-ebpf-audit.md](./weaver-ebpf-audit.md) | [loom-weaver-ebpf](../crates/loom-weaver-ebpf/), [loom-weaver-audit-sidecar](../crates/loom-weaver-audit-sidecar/) | eBPF syscall auditing sidecar |

## Build & Performance

| Spec | Code | Purpose |
|------|------|---------|
| [server-split.md](./server-split.md) | [loom-db](../crates/loom-db/), [loom-server-api](../crates/loom-server-api/) | Server crate splitting for faster builds |

## Web, Distribution & Other

| Spec | Code | Purpose |
|------|------|---------|
| [loom-web.md](./loom-web.md) | [web/loom-web](../web/loom-web/) | Svelte 5 web frontend |
| [docs-system.md](./docs-system.md) | [web/loom-web/src/routes/docs](../web/loom-web/src/routes/docs/) | Documentation system with Diátaxis, MDX, Pagefind |
| [distribution.md](./distribution.md) | [loom-version](../crates/loom-version/) | Binary builds and self-update |
| [container-system.md](./container-system.md) | [docker/](../docker/), [flake.nix](../flake.nix) | Docker/OCI via Nix |
| [sbom-system.md](./sbom-system.md) | [.github/](../.github/) | SBOM generation (SPDX/CycloneDX) |
| [i18n-system.md](./i18n-system.md) | [loom-i18n](../crates/loom-i18n/) | Internationalization with gettext |
| [testing.md](./testing.md) | [crates/](../crates/) | Property-based testing with proptest |
