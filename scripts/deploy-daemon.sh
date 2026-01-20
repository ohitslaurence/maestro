#!/usr/bin/env bash
set -euo pipefail

# Config
DAEMON_NAME="maestro-daemon"
INSTALL_DIR="${MAESTRO_INSTALL_DIR:-$HOME/.local/bin}"
DATA_DIR="${MAESTRO_DATA_DIR:-$HOME/.local/share/maestro}"
PID_FILE="$DATA_DIR/daemon.pid"
LOG_FILE="$DATA_DIR/daemon.log"
LISTEN="${MAESTRO_LISTEN:-0.0.0.0:4733}"
TOKEN="${MAESTRO_DAEMON_TOKEN:-secret}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

log() { echo -e "${GREEN}==>${NC} $1"; }
warn() { echo -e "${YELLOW}==>${NC} $1"; }
err() { echo -e "${RED}==>${NC} $1" >&2; }

# Find repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    cat <<EOF
Usage: $(basename "$0") [command]

Commands:
  deploy    Build and (re)start daemon (default)
  start     Start daemon (must be built)
  stop      Stop running daemon
  restart   Stop then start
  status    Check if daemon is running
  logs      Tail daemon logs

Environment:
  MAESTRO_DAEMON_TOKEN   Auth token (default: secret)
  MAESTRO_LISTEN         Bind address (default: 0.0.0.0:4733)
  MAESTRO_DATA_DIR       Data directory (default: ~/.local/share/maestro)
  MAESTRO_INSTALL_DIR    Binary install dir (default: ~/.local/bin)
EOF
}

is_running() {
    if [[ -f "$PID_FILE" ]]; then
        local pid
        pid=$(cat "$PID_FILE")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    return 1
}

stop_daemon() {
    if is_running; then
        local pid
        pid=$(cat "$PID_FILE")
        log "Stopping daemon (PID $pid)..."
        kill "$pid" 2>/dev/null || true

        # Wait for graceful shutdown
        for _ in {1..10}; do
            if ! kill -0 "$pid" 2>/dev/null; then
                break
            fi
            sleep 0.5
        done

        # Force kill if still running
        if kill -0 "$pid" 2>/dev/null; then
            warn "Force killing daemon..."
            kill -9 "$pid" 2>/dev/null || true
            sleep 0.2
        fi

        if kill -0 "$pid" 2>/dev/null; then
            err "Daemon did not stop (PID $pid still running)"
            ps -p "$pid" -o pid,ppid,stat,etime,cmd || true
            return 1
        fi

        rm -f "$PID_FILE"
        log "Daemon stopped"
    else
        log "Daemon not running"
    fi
}

start_daemon() {
    if is_running; then
        err "Daemon already running (PID $(cat "$PID_FILE"))"
        exit 1
    fi

    log "Using token: $TOKEN"

    if [[ ! -x "$INSTALL_DIR/$DAEMON_NAME" ]]; then
        err "Daemon not found at $INSTALL_DIR/$DAEMON_NAME"
        err "Run '$0 deploy' to build and install"
        exit 1
    fi

    mkdir -p "$DATA_DIR"

    log "Starting daemon on $LISTEN..."
    nohup "$INSTALL_DIR/$DAEMON_NAME" \
        --listen "$LISTEN" \
        --token "$TOKEN" \
        --data-dir "$DATA_DIR" \
        >> "$LOG_FILE" 2>&1 &

    local pid=$!
    echo "$pid" > "$PID_FILE"

    # Verify it started
    sleep 1
    if is_running; then
        log "Daemon started (PID $pid)"
        log "Logs: $LOG_FILE"
    else
        err "Daemon failed to start. Check logs:"
        tail -20 "$LOG_FILE"
        exit 1
    fi
}

build_daemon() {
    log "Building daemon (release)..."
    cd "$REPO_ROOT/daemon"
    cargo build --release

    mkdir -p "$INSTALL_DIR"
    local tmp_path
    tmp_path="$(mktemp "$INSTALL_DIR/$DAEMON_NAME.XXXXXX")"
    cp "target/release/$DAEMON_NAME" "$tmp_path"
    chmod +x "$tmp_path"
    mv -f "$tmp_path" "$INSTALL_DIR/$DAEMON_NAME"
    log "Installed to $INSTALL_DIR/$DAEMON_NAME"
}

status() {
    if is_running; then
        local pid
        pid=$(cat "$PID_FILE")
        log "Daemon running (PID $pid)"
        log "Listen: $LISTEN"
        log "Data dir: $DATA_DIR"
    else
        log "Daemon not running"
    fi
}

show_logs() {
    if [[ -f "$LOG_FILE" ]]; then
        tail -f "$LOG_FILE"
    else
        err "No log file at $LOG_FILE"
        exit 1
    fi
}

# Main
case "${1:-deploy}" in
    deploy)
        stop_daemon
        build_daemon
        start_daemon
        ;;
    start)
        start_daemon
        ;;
    stop)
        stop_daemon
        ;;
    restart)
        stop_daemon
        start_daemon
        ;;
    status)
        status
        ;;
    logs)
        show_logs
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        err "Unknown command: $1"
        usage
        exit 1
        ;;
esac
