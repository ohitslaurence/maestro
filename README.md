# Maestro

Native macOS app for orchestrating AI coding agents (Claude Code, Open Code) across local and remote workspaces.

## Vision

Agent orchestration is the future. Building your own tooling = staying at the forefront, testing new techniques as they emerge, not waiting for vendors.

Key goals:
- **Agent-harness agnostic**: Support multiple runtimes (Claude Code, Open Code, future harnesses)
- **Remote-first**: Run agents on a VPS, control from anywhere
- **Multi-project orchestration**: Coordinate work across repos
- **Ralph Wiggins pattern**: Fresh context per task, avoid degradation

## Architecture

```
┌─────────────────────┐         ┌─────────────────────────────────┐
│   macOS (Tauri)     │         │         VPS (Tailscale)         │
│                     │         │                                 │
│  React Frontend     │◄──TCP──►│  Agent Daemon (Rust)            │
│  + Rust Shell       │ JSON-RPC│    ├── Session Manager          │
│                     │         │    ├── Terminal PTY             │
│                     │         │    ├── Git Operations           │
└─────────────────────┘         │    └── Agent Processes          │
                                └─────────────────────────────────┘
```

## Acknowledgments

This project draws heavy inspiration from [CodexMonitor](https://github.com/Dimillian/CodexMonitor) by Thomas Ricouard ([@Dimillian](https://github.com/Dimillian)). We've included it as a reference subtree and borrowed liberally from its:

- Tauri project structure
- React component patterns
- `@pierre/diffs` integration approach
- Terminal handling (portable-pty)
- Git operations implementation
- Remote backend daemon POC

Key difference: CodexMonitor is built around Codex's `app-server` protocol. Maestro adapts these patterns for Claude Code and Open Code from day one.

## Project Structure

```
maestro/
├── app/                    # Tauri application
│   ├── src/               # React frontend
│   ├── src-tauri/         # Rust backend
│   └── package.json
├── specs/                  # Design documents
│   ├── orchestrator.md    # Main spec
│   └── ROADMAP.md         # Development phases
└── reference/
    └── codex-monitor/     # CodexMonitor subtree (reference only)
```

## Development

### Prerequisites

- Node.js / Bun
- Rust toolchain (stable)
- Xcode Command Line Tools (macOS)

### Running Locally

```bash
cd app
bun install
bun run tauri:dev
```

### Building

```bash
cd app
bun run tauri build
```

## Roadmap

See [specs/ROADMAP.md](specs/ROADMAP.md) for current status and planned phases.

## License

MIT
