# agent-loop-tui

Small Ratatui wrapper for `scripts/agent-loop.sh`. It runs the loop with `--no-gum`,
`--summary-json`, and `--no-wait`, then renders live state from
`logs/agent-loop/run-<id>/`.

## Usage

Single word wrapper (recommended, with interactive picker if no spec is provided):

```bash
./agent-loop-tui specs/my-spec.md
```

From the repo root (direct cargo):

```bash
cargo run --manifest-path tools/agent-loop-tui/Cargo.toml -- specs/my-spec.md
```

To pass flags to `scripts/agent-loop.sh`, use `--`:

```bash
cargo run --manifest-path tools/agent-loop-tui/Cargo.toml -- -- specs/my-spec.md --iterations 10
```

Override the log directory (this is forwarded to the script):

```bash
cargo run --manifest-path tools/agent-loop-tui/Cargo.toml -- --log-dir /tmp/agent-loop -- specs/my-spec.md
```

Controls:
- `q` to quit (sends SIGINT if the loop is still running)
