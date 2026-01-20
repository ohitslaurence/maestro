# Task: Implement Terminal (PTY + xterm.js)

## Objective

Port CodexMonitor's terminal implementation to Maestro. This provides PTY management on the Rust side and xterm.js rendering on the frontend.

## Reference

- `reference/codex-monitor/src-tauri/src/terminal.rs` - PTY management
- `reference/codex-monitor/src/features/terminal/hooks/useTerminalSession.ts` - Frontend integration
- `reference/codex-monitor/src/features/terminal/components/TerminalPanel.tsx` - UI component

## Output

### Backend (Rust)
- `app/src-tauri/src/terminal.rs` - PTY session management

### Frontend (React)
- `app/src/features/terminal/hooks/useTerminalSession.ts` - xterm.js integration
- `app/src/features/terminal/components/TerminalPanel.tsx` - Presentational component

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
- `terminal_open(workspace_id, terminal_id, cwd)` - Spawn PTY, start reader thread
- `terminal_write(workspace_id, terminal_id, data)` - Write to PTY stdin
- `terminal_resize(workspace_id, terminal_id, rows, cols)` - Resize PTY
- `terminal_close(workspace_id, terminal_id)` - Kill shell, cleanup

**Reader Thread:**
- Read PTY output in 8KB chunks
- Emit `terminal-output` event with `{ workspace_id, terminal_id, data }`
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
