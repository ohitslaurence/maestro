import { useCallback, useEffect, useRef, useState } from "react";
import type { DaemonConnectionStatus, DaemonStatus } from "../../../types";
import {
  daemonConfigure,
  daemonConnect,
  daemonDisconnect,
  daemonStatus,
} from "../../../services/tauri";
import {
  subscribeDaemonConnected,
  subscribeDaemonDebug,
  subscribeDaemonDisconnected,
} from "../../../services/events";

export type DaemonConnectionState = {
  status: DaemonConnectionStatus;
  host?: string;
  port?: number;
  error?: string;
  connect: () => Promise<void>;
  disconnect: () => Promise<void>;
  configure: (host: string, port: number, token: string) => Promise<void>;
  refresh: () => Promise<void>;
};

export function useDaemonConnection(): DaemonConnectionState {
  const [status, setStatus] = useState<DaemonConnectionStatus>("disconnected");
  const [host, setHost] = useState<string | undefined>();
  const [port, setPort] = useState<number | undefined>();
  const [error, setError] = useState<string | undefined>();
  const autoConnectPendingRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const result: DaemonStatus = await daemonStatus();
      setHost(result.host);
      setPort(result.port);
      setStatus(result.connected ? "connected" : "disconnected");
      setError(undefined);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message === "daemon_not_configured") {
        setStatus("disconnected");
      } else {
        setStatus("error");
        setError(message);
      }
    }
  }, []);

  const connect = useCallback(async () => {
    setStatus("connecting");
    setError(undefined);
    try {
      await daemonConnect();
      setStatus("connected");
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setStatus("error");
      setError(formatError(message));
    }
  }, []);

  const disconnect = useCallback(async () => {
    try {
      await daemonDisconnect();
      setStatus("disconnected");
      setError(undefined);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    }
  }, []);

  const configure = useCallback(
    async (newHost: string, newPort: number, token: string) => {
      try {
        autoConnectPendingRef.current = false;
        await daemonConfigure(newHost, newPort, token);
        setHost(newHost);
        setPort(newPort);
        setError(undefined);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        throw err;
      }
    },
    [],
  );

  // Initial status check
  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!autoConnectPendingRef.current) {
      return;
    }

    if (status === "connected" || status === "connecting" || status === "error") {
      autoConnectPendingRef.current = false;
      return;
    }

    if (status === "disconnected" && host && port) {
      autoConnectPendingRef.current = false;
      void connect();
    }
  }, [status, host, port, connect]);

  // Subscribe to daemon connection events
  useEffect(() => {
    const unsubConnect = subscribeDaemonConnected(() => {
      setStatus("connected");
      setError(undefined);
    });

    const unsubDisconnect = subscribeDaemonDisconnected((event) => {
      setStatus("disconnected");
      if (event.reason) {
        setError(`Disconnected: ${event.reason}`);
      }
    });

    return () => {
      unsubConnect();
      unsubDisconnect();
    };
  }, []);

  useEffect(() => {
    return subscribeDaemonDebug((event) => {
      if (event.data !== undefined) {
        console.info("[daemon]", event.message, event.data);
      } else {
        console.info("[daemon]", event.message);
      }
    });
  }, []);

  return {
    status,
    host,
    port,
    error,
    connect,
    disconnect,
    configure,
    refresh,
  };
}

/** Format daemon error messages for display */
function formatError(error: string): string {
  if (error.startsWith("daemon_not_configured")) {
    return "Daemon not configured";
  }
  if (error.startsWith("daemon_connection_failed")) {
    const reason = error.split(": ")[1] ?? "";
    return `Cannot reach daemon${reason ? `: ${reason}` : ""}`;
  }
  if (error.startsWith("daemon_auth_failed")) {
    return "Invalid token";
  }
  if (error === "daemon_disconnected") {
    return "Disconnected from daemon";
  }
  return error;
}
