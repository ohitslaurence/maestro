# Task: Implement Terminal (PTY + xterm.js)

## Objective

Port CodexMonitor's terminal implementation to Maestro. This provides PTY management on the Rust side and xterm.js rendering on the frontend.

## Reference

- `reference/codex-monitor/src-tauri/src/terminal.rs` - PTY management
- `reference/codex-monitor/src/features/terminal/hooks/useTerminalSession.ts` - Frontend integration
- `reference/codex-monitor/src/features/terminal/components/TerminalPanel.tsx` - UI component

## Current State

### Already exists:
- **Backend stubs** in `app/src-tauri/src/sessions.rs:133-167` - Commands registered but no-op
- **Commands registered** in `app/src-tauri/src/lib.rs:30-33`
- **Frontend wrappers** in `app/src/services/tauri.ts:50-89` - `openTerminal`, `writeTerminal`, `resizeTerminal`, `closeTerminal`
- **Event subscription** in `app/src/services/events.ts` - `subscribeTerminalOutput`, `TerminalOutputEvent`

### Decision: File organization
Extract terminal implementation to `app/src-tauri/src/terminal.rs` for cleaner separation. Move existing stubs from `sessions.rs` and update `lib.rs` imports.

## Output

### Backend (Rust)
- Create `app/src-tauri/src/terminal.rs` - extract and implement PTY logic
- Remove terminal stubs from `sessions.rs`, update `lib.rs` imports
- Add `portable-pty` dependency to `Cargo.toml`

### Frontend (React)
- `app/src/features/terminal/hooks/useTerminalSession.ts` - xterm.js integration
- `app/src/features/terminal/components/TerminalPanel.tsx` - Presentational component
- Add `@xterm/xterm` and `@xterm/addon-fit` dependencies

## Implementation Details

### Backend: PTY Management

```rust
pub struct TerminalSession {
    pub id: String,
    pub master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    pub writer: Mutex<Box<dyn Write + Send>>,
    pub child: Mutex<Box<dyn portable_pty::Child + Send>>,
}
```

**Commands:**
- `terminal_open(session_id, terminal_id, cwd)` - Spawn PTY, start reader thread
- `terminal_write(session_id, terminal_id, data)` - Write to PTY stdin
- `terminal_resize(session_id, terminal_id, rows, cols)` - Resize PTY
- `terminal_close(session_id, terminal_id)` - Kill shell, cleanup

**Reader Thread:**
- Read PTY output in 8KB chunks
- Emit `terminal-output` event with `{ session_id, terminal_id, data }`
- Block until shell exits

### Frontend: xterm.js Integration

```typescript
const terminal = new Terminal({
  cursorBlink: true,
  fontSize: 12,
  fontFamily: "Menlo, Monaco, Courier New, monospace",
  theme: { background: "transparent", foreground: "#d9dee7" },
  scrollback: 5000,
});

const fitAddon = new FitAddon();
terminal.loadAddon(fitAddon);
terminal.open(containerRef.current);
fitAddon.fit();
```

**Key Features:**
- Output buffering (cap at 200KB per session)
- Session switching (restore buffer on switch)
- Resize observation (ResizeObserver → fitAddon.fit() → PTY resize)
- Input handling (terminal.onData → terminal_write)

### Dependencies

Add to `app/package.json`:
```json
"@xterm/xterm": "^5.x",
"@xterm/addon-fit": "^0.x"
```

Add to `app/src-tauri/Cargo.toml`:
```toml
portable-pty = "0.8"
```

## Constraints

- PTY reads must be in background thread (blocks until EOF)
- Buffer management to prevent memory leaks
- Graceful handling of session death (detect "not found" errors)
- Shell environment: `TERM=xterm-256color`

## Gotchas

1. PTY resize must happen after container is measured
2. UTF-8 lossy conversion for terminal data
3. Session cleanup on unexpected shell exit
