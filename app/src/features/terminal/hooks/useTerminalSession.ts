import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { TerminalStatus } from "../../../types";
import {
  subscribeTerminalOutput,
  type TerminalOutputEvent,
} from "../../../services/events";
import {
  openTerminal,
  resizeTerminal,
  writeTerminal,
} from "../../../services/tauri";

const MAX_BUFFER_CHARS = 200_000;

type UseTerminalSessionOptions = {
  sessionId: string | null;
  terminalId: string | null;
  isVisible: boolean;
};

export type TerminalSessionState = {
  status: TerminalStatus;
  message: string;
  containerRef: RefObject<HTMLDivElement | null>;
  hasSession: boolean;
  cleanupTerminalSession: (sessionId: string, terminalId: string) => void;
};

function appendBuffer(existing: string | undefined, data: string): string {
  const next = (existing ?? "") + data;
  if (next.length <= MAX_BUFFER_CHARS) {
    return next;
  }
  return next.slice(next.length - MAX_BUFFER_CHARS);
}

function shouldIgnoreTerminalError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("Terminal session not found");
}

export function useTerminalSession({
  sessionId,
  terminalId,
  isVisible,
}: UseTerminalSessionOptions): TerminalSessionState {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const inputDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const openedSessionsRef = useRef<Set<string>>(new Set());
  const outputBuffersRef = useRef<Map<string, string>>(new Map());
  const activeKeyRef = useRef<string | null>(null);
  const renderedKeyRef = useRef<string | null>(null);
  const sessionIdRef = useRef<string | null>(null);
  const terminalIdRef = useRef<string | null>(null);
  const [status, setStatus] = useState<TerminalStatus>("idle");
  const [message, setMessage] = useState("Open a terminal to start a session.");
  const [hasSession, setHasSession] = useState(false);

  const cleanupTerminalSession = useCallback(
    (sid: string, tid: string) => {
      const key = `${sid}:${tid}`;
      outputBuffersRef.current.delete(key);
      openedSessionsRef.current.delete(key);
      if (activeKeyRef.current === key) {
        terminalRef.current?.reset();
      }
    },
    [],
  );

  const activeKey = useMemo(() => {
    if (!sessionId || !terminalId) {
      return null;
    }
    return `${sessionId}:${terminalId}`;
  }, [sessionId, terminalId]);

  useEffect(() => {
    activeKeyRef.current = activeKey;
    sessionIdRef.current = sessionId;
    terminalIdRef.current = terminalId;
  }, [activeKey, sessionId, terminalId]);

  const writeToTerminal = useCallback((data: string) => {
    terminalRef.current?.write(data);
  }, []);

  const refreshTerminal = useCallback(() => {
    const terminal = terminalRef.current;
    if (!terminal) {
      return;
    }
    const lastRow = Math.max(0, terminal.rows - 1);
    terminal.refresh(0, lastRow);
    terminal.focus();
  }, []);

  const syncActiveBuffer = useCallback(
    (key: string) => {
      const term = terminalRef.current;
      if (!term) {
        return;
      }
      term.reset();
      const buffered = outputBuffersRef.current.get(key);
      if (buffered) {
        term.write(buffered);
      }
      refreshTerminal();
    },
    [refreshTerminal],
  );

  // Subscribe to terminal output events
  useEffect(() => {
    const unlisten = subscribeTerminalOutput(
      (payload: TerminalOutputEvent) => {
        const { sessionId: sid, terminalId: tid, data } = payload;
        const key = `${sid}:${tid}`;
        const next = appendBuffer(outputBuffersRef.current.get(key), data);
        outputBuffersRef.current.set(key, next);
        if (activeKeyRef.current === key) {
          writeToTerminal(data);
        }
      },
      {
        onError: (error) => {
          console.error("[terminal] listen error", error);
        },
      },
    );
    return () => {
      unlisten();
    };
  }, [writeToTerminal]);

  // Initialize/destroy xterm instance
  useEffect(() => {
    if (!isVisible) {
      inputDisposableRef.current?.dispose();
      inputDisposableRef.current = null;
      if (terminalRef.current) {
        terminalRef.current.dispose();
        terminalRef.current = null;
      }
      fitAddonRef.current = null;
      renderedKeyRef.current = null;
      return;
    }

    if (!terminalRef.current && containerRef.current) {
      const terminal = new Terminal({
        cursorBlink: true,
        fontSize: 12,
        fontFamily: 'Menlo, Monaco, "Courier New", monospace',
        allowTransparency: true,
        theme: {
          background: "transparent",
          foreground: "#d9dee7",
          cursor: "#d9dee7",
        },
        scrollback: 5000,
      });
      const fitAddon = new FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(containerRef.current);
      fitAddon.fit();
      terminalRef.current = terminal;
      fitAddonRef.current = fitAddon;

      inputDisposableRef.current = terminal.onData((data: string) => {
        const sid = sessionIdRef.current;
        const tid = terminalIdRef.current;
        if (!sid || !tid) {
          return;
        }
        const key = `${sid}:${tid}`;
        if (!openedSessionsRef.current.has(key)) {
          return;
        }
        void writeTerminal(sid, tid, data).catch((error) => {
          if (shouldIgnoreTerminalError(error)) {
            openedSessionsRef.current.delete(key);
            return;
          }
          console.error("[terminal] write error", error);
        });
      });
    }
  }, [isVisible]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      inputDisposableRef.current?.dispose();
      inputDisposableRef.current = null;
      if (terminalRef.current) {
        terminalRef.current.dispose();
        terminalRef.current = null;
      }
      fitAddonRef.current = null;
    };
  }, []);

  // Open session when sessionId/terminalId change
  useEffect(() => {
    if (!isVisible) {
      setHasSession(false);
      return;
    }
    if (!sessionId || !terminalId) {
      setStatus("idle");
      setMessage("Open a terminal to start a session.");
      setHasSession(false);
      return;
    }
    if (!terminalRef.current || !fitAddonRef.current) {
      setStatus("idle");
      setMessage("Preparing terminal...");
      return;
    }
    const key = `${sessionId}:${terminalId}`;
    const fitAddon = fitAddonRef.current;
    fitAddon.fit();

    const cols = terminalRef.current.cols;
    const rows = terminalRef.current.rows;
    const doOpen = async () => {
      setStatus("connecting");
      setMessage("Starting terminal session...");
      if (!openedSessionsRef.current.has(key)) {
        await openTerminal(sessionId, terminalId, cols, rows);
        openedSessionsRef.current.add(key);
      }
      setStatus("ready");
      setMessage("Terminal ready.");
      setHasSession(true);
      if (renderedKeyRef.current !== key) {
        syncActiveBuffer(key);
        renderedKeyRef.current = key;
      } else {
        refreshTerminal();
      }
    };

    doOpen().catch((error) => {
      setStatus("error");
      setMessage("Failed to start terminal session.");
      console.error("[terminal] open error", error);
    });
  }, [sessionId, terminalId, isVisible, refreshTerminal, syncActiveBuffer]);

  // Refit when active key changes
  useEffect(() => {
    if (!isVisible || !activeKey || !terminalRef.current || !fitAddonRef.current) {
      return;
    }
    fitAddonRef.current.fit();
    refreshTerminal();
  }, [activeKey, isVisible, refreshTerminal]);

  // ResizeObserver for container
  useEffect(() => {
    if (!isVisible || !terminalRef.current || !sessionId || !terminalId || !hasSession) {
      return;
    }
    const fitAddon = fitAddonRef.current;
    const terminal = terminalRef.current;
    if (!fitAddon) {
      return;
    }

    const resize = () => {
      fitAddon.fit();
      const key = `${sessionId}:${terminalId}`;
      resizeTerminal(sessionId, terminalId, terminal.cols, terminal.rows).catch(
        (error) => {
          if (shouldIgnoreTerminalError(error)) {
            openedSessionsRef.current.delete(key);
            return;
          }
          console.error("[terminal] resize error", error);
        },
      );
    };

    const observer = new ResizeObserver(() => {
      resize();
    });

    if (containerRef.current) {
      observer.observe(containerRef.current);
    }
    resize();

    return () => {
      observer.disconnect();
    };
  }, [sessionId, terminalId, hasSession, isVisible]);

  return {
    status,
    message,
    containerRef,
    hasSession,
    cleanupTerminalSession,
  };
}
