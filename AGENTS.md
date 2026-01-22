# Maestro Agent Guide

## Project Summary

Maestro is a macOS Tauri app that orchestrates AI coding agents (Claude Code, Open Code) across local and remote workspaces. The frontend is React + Vite; the backend is a Tauri Rust process that manages agent sessions over JSON-RPC.

Unlike CodexMonitor (which we reference heavily), Maestro is agent-harness agnostic: it abstracts over multiple agent runtimes rather than being tied to a single protocol.

## Key Paths

### Frontend (React)
- `app/src/App.tsx`: composition root
- `app/src/main.tsx`: entry point
- `app/src/styles.css`: global styles

### Backend (Rust/Tauri)
- `app/src-tauri/src/lib.rs`: Tauri command definitions
- `app/src-tauri/src/main.rs`: app entry point
- `app/src-tauri/src/sessions.rs`: agent session management

### Daemon (Rust + Bun)
- `daemon/src/`: Rust daemon (JSON-RPC server, session management, git ops)
- `daemon/claude-server/`: Claude SDK server (Bun) - **single-file implementation**
  - `src/server.ts`: Complete server with SQLite storage, SSE events, permissions
  - Spawned by daemon per workspace
  - Do NOT create alternative implementations

### Specs & Reference
- `specs/VISION.md`: product vision and north star
- `specs/ROADMAP.md`: development phases and current status
- `reference/codex-monitor/`: CodexMonitor subtree (read-only reference)

## Architecture Guidelines

### Core Principles
- **Composition root**: keep orchestration in `App.tsx`; avoid logic in components
- **Components**: presentational only; props in, UI out; no Tauri IPC calls
- **Hooks**: own state, side-effects, and event wiring
- **Services**: all Tauri IPC goes through a dedicated service layer
- **Types**: shared UI data types in a central types file

### Agent Harness Abstraction
Each agent harness (Claude Code, Open Code, etc.) implements:
```rust
trait AgentHarness {
    fn spawn(&self, project_path: &Path, config: Config) -> Session;
    fn attach(&self, session_id: &str) -> Stream;
    fn send_input(&self, session_id: &str, input: &str);
    fn stop(&self, session_id: &str);
    fn get_status(&self, session_id: &str) -> Status;
}
```

When adding new harness support, implement this trait and register in session manager.

### Remote Daemon Architecture
The VPS daemon exposes JSON-RPC over TCP:
- Session lifecycle (spawn, attach, stop)
- Terminal PTY streaming
- Git operations
- Agent process management

Tailscale provides network security; daemon uses token auth.

## Package Manager

**Always use `bun` for TypeScript/JavaScript operations.** Do not use `npm`, `yarn`, or `pnpm`.

```bash
bun install      # not npm install
bun run <script> # not npm run
bun add <pkg>    # not npm install <pkg>
```

## Running Locally

```bash
cd app
bun install
bun run tauri:dev
```

## Agent UI Runbook (Web + Playwright)

Use this to let an agent drive the UI in a browser (not Tauri) and validate behavior.

One-time setup (if needed):

```bash
cd app
bun install
bunx playwright install chromium
```

```bash
cd daemon
cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth
```

```bash
cd app
bun run dev -- --host 127.0.0.1 --port 1420
```

```bash
cd app
bun run ui:smoke
```

Feature validation (required for UI specs):
1. Duplicate the smoke script and add steps/assertions for the feature.
2. Run the new script against a live daemon + web UI.

```bash
cp app/scripts/ui-smoke.ts app/scripts/ui-<feature>.ts
```

```bash
cd daemon
cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth
```

```bash
cd app
bun run dev -- --host 127.0.0.1 --port 1420
```

```bash
cd app
bun scripts/ui-<feature>.ts
```

Keep feature scripts narrow: one flow, clear assertions, no shared state.

Overrides (only if you change host/port):
- `MAESTRO_DAEMON_HOST`, `MAESTRO_DAEMON_PORT`, `MAESTRO_DAEMON_TOKEN`, `MAESTRO_UI_URL`
- `MAESTRO_HEADLESS=false` to watch the browser

`ui:smoke` defaults to `MAESTRO_DAEMON_TOKEN=dev` (daemon ignores it with `--insecure-no-auth`).

## Building

```bash
cd app
bun run tauri build
```

## Type Checking

```bash
cd app
bun run typecheck
```

## Validation

Before completing a task:
1. Run `bun run lint` (once we add linting)
2. Run `bun run test` (once we add tests)
3. Run `bun run typecheck`

## Reference: CodexMonitor

We include CodexMonitor as a git subtree under `reference/codex-monitor/`. When implementing features, consult:

- `reference/codex-monitor/AGENTS.md` - their agent guide
- `reference/codex-monitor/src-tauri/src/` - Rust backend patterns
- `reference/codex-monitor/src/features/` - React component patterns
- `reference/codex-monitor/src/services/` - Tauri IPC wrapper patterns

**Do not modify files under `reference/`** - it's read-only reference material.

To update the reference:
```bash
git subtree pull --prefix=reference/codex-monitor https://github.com/Dimillian/CodexMonitor.git main --squash
```

## Common Changes

### Adding a new Tauri command
1. Add command in `app/src-tauri/src/lib.rs`
2. Register in `.invoke_handler()`
3. Add TypeScript wrapper in frontend service layer
4. Wire to UI via hook

### Adding a new agent harness
1. Implement `AgentHarness` trait in `sessions.rs`
2. Register in harness registry
3. Add UI for harness selection
4. Document in specs

### Adding UI features
1. Create component in feature folder
2. Hook for state/effects
3. Service calls for IPC
4. CSS in styles or CSS modules

## Notes

- The app uses Tauri's window overlay style for native macOS feel
- Agent sessions are isolated per workspace/project
- Remote daemon mode allows running agents on VPS while controlling from Mac
- Future: mobile web UI via same daemon API

## Anti-Patterns to Avoid

### No Duplicate Implementations
Before creating a new module/service:
1. Search for existing implementations (`Glob`, `Grep`)
2. Check specs for canonical locations
3. If similar code exists, extend it rather than creating alternatives

**Bad**: Creating `daemon/claude-server-v2/` when `daemon/claude-server/` already exists
**Good**: Extending `daemon/claude-server/src/server.ts` with new features

### No Modular Rewrites Without Consolidation
If refactoring a single-file implementation into modules:
1. Delete the old implementation
2. Update all path references in specs
3. Verify daemon/tauri code points to new location

### Check Path Resolution
The daemon resolves the Claude server via:
```rust
// daemon/src/claude_sdk.rs
let candidates = [cwd.join("daemon/claude-server"), cwd.join("claude-server")];
```
Any new server location must match these patterns or update the resolution logic.
