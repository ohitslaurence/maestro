import type { DaemonStatus } from "../../types";
import type { StreamEvent } from "../../types/streaming";
import { OpenCodeAdapter } from "./opencodeAdapter";

type DaemonConfig = {
  host: string;
  port: number;
  token: string;
};

type RpcRequest = {
  host: string;
  port: number;
  token: string;
  method: string;
  params?: Record<string, unknown> | null;
};

type RpcError = {
  code?: string;
  message?: string;
};

type RpcResponse = {
  result?: unknown;
  error?: RpcError;
};

const STORAGE_KEY = "maestro.daemon.web.config";
const RPC_PATH = "/__daemon__/rpc";
const EVENTS_PATH = "/__daemon__/events";
const PARAM_KEY_MAP: Record<string, string> = {
  sessionId: "session_id",
  terminalId: "terminal_id",
  workspaceId: "workspace_id",
  workspacePath: "workspace_path",
};

const eventTarget = new EventTarget();
const opencodeAdapter = new OpenCodeAdapter();

let config: DaemonConfig | null = loadConfig();
let connected = false;
let eventSource: EventSource | null = null;

function loadConfig(): DaemonConfig | null {
  if (typeof localStorage === "undefined") {
    return null;
  }

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") {
      return null;
    }
    const host = typeof parsed.host === "string" ? parsed.host : "";
    const port = Number(parsed.port);
    const token = typeof parsed.token === "string" ? parsed.token : "";
    if (!host || !Number.isFinite(port)) {
      return null;
    }
    return { host, port, token };
  } catch {
    return null;
  }
}

function saveConfig(next: DaemonConfig | null) {
  if (typeof localStorage === "undefined") {
    return;
  }

  try {
    if (!next) {
      localStorage.removeItem(STORAGE_KEY);
      return;
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Ignore persistence errors
  }
}

function emitEvent(name: string, payload: unknown) {
  const event = new CustomEvent(name, { detail: payload });
  eventTarget.dispatchEvent(event);
}

function normalizeParams(
  params: Record<string, unknown> | null | undefined,
): Record<string, unknown> | null {
  if (!params) {
    return null;
  }

  const normalized: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    const mapped = PARAM_KEY_MAP[key] ?? key;
    normalized[mapped] = value;
  }
  return normalized;
}

function normalizeResult(command: string, result: unknown): unknown {
  const data = result as Record<string, unknown> | null;
  if (!data || typeof data !== "object") {
    return result;
  }

  switch (command) {
    case "session_info":
      return {
        ...data,
        hasGit: data.hasGit ?? data.has_git,
      };
    case "git_status":
      return {
        ...data,
        branchName: data.branchName ?? data.branch_name ?? "",
        stagedFiles: data.stagedFiles ?? data.staged_files ?? [],
        unstagedFiles: data.unstagedFiles ?? data.unstaged_files ?? [],
        totalAdditions: data.totalAdditions ?? data.total_additions ?? 0,
        totalDeletions: data.totalDeletions ?? data.total_deletions ?? 0,
      };
    case "git_diff":
      return {
        ...data,
        truncatedFiles: data.truncatedFiles ?? data.truncated_files ?? [],
      };
    case "terminal_open":
      return {
        id: data.id ?? data.terminal_id ?? "",
      };
    default:
      return result;
  }
}

function requireConfig(): DaemonConfig {
  if (!config) {
    throw "daemon_not_configured";
  }
  return config;
}

function assertConnected() {
  if (!connected) {
    throw "daemon_disconnected";
  }
}

async function rpcCall(method: string, params?: Record<string, unknown> | null) {
  const current = requireConfig();
  const body: RpcRequest = {
    host: current.host,
    port: current.port,
    token: current.token,
    method,
    params: params ?? null,
  };

  const response = await fetch(RPC_PATH, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });

  let data: RpcResponse | null = null;
  try {
    data = (await response.json()) as RpcResponse;
  } catch {
    data = null;
  }

  if (!response.ok || data?.error) {
    const code = data?.error?.code ?? "rpc_error";
    const message = data?.error?.message ?? response.statusText ?? "RPC error";
    throw new Error(`${code}: ${message}`);
  }

  return data?.result;
}

function startEventStream() {
  const current = requireConfig();
  if (eventSource) {
    eventSource.close();
  }

  const params = new URLSearchParams({
    host: current.host,
    port: String(current.port),
    token: current.token,
  });
  const url = `${EVENTS_PATH}?${params.toString()}`;
  eventSource = new EventSource(url);

  eventSource.onmessage = (event) => {
    try {
      const message = JSON.parse(event.data) as { method?: string; params?: unknown };
      handleDaemonEvent(message);
    } catch (error) {
      emitEvent("daemon:debug", {
        message: "web:invalid_event",
        data: { error: String(error) },
      });
    }
  };

  eventSource.onopen = () => {
    emitEvent("daemon:debug", { message: "web:events_open" });
  };

  eventSource.onerror = () => {
    if (eventSource?.readyState === EventSource.CLOSED) {
      connected = false;
      emitEvent("daemon:disconnected", { reason: "event_stream_closed" });
    }
  };
}

function stopEventStream() {
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
}

function handleDaemonEvent(message: { method?: string; params?: unknown }) {
  const method = message.method;
  if (!method) {
    return;
  }

  if (method === "terminal_output") {
    emitEvent("daemon:terminal_output", message.params ?? {});
    return;
  }

  if (method === "terminal_exited") {
    emitEvent("daemon:terminal_exited", message.params ?? {});
    return;
  }

  if (method === "opencode:event") {
    emitEvent("daemon:opencode_event", message.params ?? {});
    const streamEvents = opencodeAdapter.adapt(message.params);
    if (streamEvents) {
      for (const streamEvent of streamEvents) {
        emitEvent("agent:stream_event", streamEvent);
      }
    }
    return;
  }

  emitEvent("daemon:debug", {
    message: "web:unhandled_event",
    data: { method },
  });
}

export async function webInvoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  switch (command) {
    case "daemon_configure": {
      const host = String(payload?.host ?? "");
      const port = Number(payload?.port);
      const token = String(payload?.token ?? "");
      if (!host || !Number.isFinite(port)) {
        throw new Error("daemon_config_invalid");
      }
      if (connected) {
        await webInvoke("daemon_disconnect");
      }
      config = { host, port, token };
      saveConfig(config);
      return undefined as T;
    }
    case "daemon_connect": {
      const current = requireConfig();
      emitEvent("daemon:debug", {
        message: "web:connect:start",
        data: { host: current.host, port: current.port },
      });
      await rpcCall("list_sessions", null);
      connected = true;
      startEventStream();
      emitEvent("daemon:connected", { connected: true });
      return { connected: true } as T;
    }
    case "daemon_disconnect": {
      connected = false;
      stopEventStream();
      emitEvent("daemon:disconnected", {});
      return undefined as T;
    }
    case "daemon_status": {
      const status: DaemonStatus = {
        connected,
        host: config?.host,
        port: config?.port,
      };
      return status as T;
    }
    default: {
      assertConnected();
      const normalizedParams = normalizeParams(payload ?? null);
      const result = await rpcCall(command, normalizedParams);
      return normalizeResult(command, result) as T;
    }
  }
}

export async function webListen<T>(
  eventName: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const listener = (event: Event) => {
    if (event instanceof CustomEvent) {
      handler({ payload: event.detail as T });
    }
  };

  eventTarget.addEventListener(eventName, listener);

  return () => {
    eventTarget.removeEventListener(eventName, listener);
  };
}

export function emitStreamEvent(event: StreamEvent) {
  emitEvent("agent:stream_event", event);
}
