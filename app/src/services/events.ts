import { listen } from "./bridge";
import { useEffect, type DependencyList } from "react";
import type { OpenCodeEvent, SessionStatus } from "../types";
import {
  AGENT_STATE_EVENT_CHANNEL,
  type AgentStateEventEnvelope,
} from "../types/agent";
import {
  STREAM_EVENT_CHANNEL,
  type StreamEvent,
} from "../types/streaming";

export type Unsubscribe = () => void;

type Listener<T> = (payload: T) => void;

type SubscriptionOptions = {
  onError?: (error: unknown) => void;
};

// --- Event payload types ---

/** Terminal output event (daemon sends snake_case, we normalize to camelCase) */
export type TerminalOutputEvent = {
  sessionId: string;
  terminalId: string;
  data: string;
};

/** Raw terminal output from daemon (snake_case) */
type RawTerminalOutputEvent = {
  session_id: string;
  terminal_id: string;
  data: string;
};

/** Terminal exited event */
export type TerminalExitedEvent = {
  sessionId: string;
  terminalId: string;
  exitCode?: number;
};

/** Raw terminal exited from daemon (snake_case) */
type RawTerminalExitedEvent = {
  session_id: string;
  terminal_id: string;
  exit_code?: number;
};

/** Daemon connection event */
export type DaemonConnectionEvent = {
  connected: boolean;
  reason?: string;
};

export type DaemonDebugEvent = {
  message: string;
  data?: unknown;
};

export type AgentEventType = "started" | "stopped" | "output" | "error";

export type AgentEvent = {
  sessionId: string;
  type: AgentEventType;
  data?: string;
};

export type SessionStatusEvent = {
  sessionId: string;
  status: SessionStatus;
};

// --- Event hub factory ---

function createEventHub<T>(eventName: string) {
  const listeners = new Set<Listener<T>>();
  let unlisten: Unsubscribe | null = null;
  let listenPromise: Promise<Unsubscribe> | null = null;

  const start = (options?: SubscriptionOptions) => {
    if (unlisten || listenPromise) {
      return;
    }
    listenPromise = listen<T>(eventName, (event) => {
      for (const listener of listeners) {
        try {
          listener(event.payload);
        } catch (error) {
          console.error(`[events] ${eventName} listener failed`, error);
        }
      }
    });
    listenPromise
      .then((handler) => {
        listenPromise = null;
        if (listeners.size === 0) {
          handler();
          return;
        }
        unlisten = handler;
      })
      .catch((error) => {
        listenPromise = null;
        options?.onError?.(error);
      });
  };

  const stop = () => {
    if (unlisten) {
      try {
        unlisten();
      } catch {
        // Ignore double-unlisten when tearing down.
      }
      unlisten = null;
    }
  };

  const subscribe = (
    onEvent: Listener<T>,
    options?: SubscriptionOptions,
  ): Unsubscribe => {
    listeners.add(onEvent);
    start(options);
    return () => {
      listeners.delete(onEvent);
      if (listeners.size === 0) {
        stop();
      }
    };
  };

  return { subscribe };
}

/** Create an event hub that transforms the payload before dispatching */
function createTransformingEventHub<TRaw, T>(
  eventName: string,
  transform: (raw: TRaw) => T,
) {
  const listeners = new Set<Listener<T>>();
  let unlisten: Unsubscribe | null = null;
  let listenPromise: Promise<Unsubscribe> | null = null;

  const start = (options?: SubscriptionOptions) => {
    if (unlisten || listenPromise) {
      return;
    }
    listenPromise = listen<TRaw>(eventName, (event) => {
      const transformed = transform(event.payload);
      for (const listener of listeners) {
        try {
          listener(transformed);
        } catch (error) {
          console.error(`[events] ${eventName} listener failed`, error);
        }
      }
    });
    listenPromise
      .then((handler) => {
        listenPromise = null;
        if (listeners.size === 0) {
          handler();
          return;
        }
        unlisten = handler;
      })
      .catch((error) => {
        listenPromise = null;
        options?.onError?.(error);
      });
  };

  const stop = () => {
    if (unlisten) {
      try {
        unlisten();
      } catch {
        // Ignore double-unlisten when tearing down.
      }
      unlisten = null;
    }
  };

  const subscribe = (
    onEvent: Listener<T>,
    options?: SubscriptionOptions,
  ): Unsubscribe => {
    listeners.add(onEvent);
    start(options);
    return () => {
      listeners.delete(onEvent);
      if (listeners.size === 0) {
        stop();
      }
    };
  };

  return { subscribe };
}

// --- Event hubs ---

// Terminal output: daemon sends snake_case, transform to camelCase
const terminalOutputHub = createTransformingEventHub<
  RawTerminalOutputEvent,
  TerminalOutputEvent
>("daemon:terminal_output", (raw) => ({
  sessionId: raw.session_id,
  terminalId: raw.terminal_id,
  data: raw.data,
}));

// Terminal exited: daemon sends snake_case, transform to camelCase
const terminalExitedHub = createTransformingEventHub<
  RawTerminalExitedEvent,
  TerminalExitedEvent
>("daemon:terminal_exited", (raw) => ({
  sessionId: raw.session_id,
  terminalId: raw.terminal_id,
  exitCode: raw.exit_code,
}));

// Daemon connection events
const daemonConnectedHub = createEventHub<DaemonConnectionEvent>("daemon:connected");
const daemonDisconnectedHub = createEventHub<{ reason?: string }>("daemon:disconnected");
const daemonDebugHub = createEventHub<DaemonDebugEvent>("daemon:debug");

// Agent and session events (local)
const agentEventHub = createEventHub<AgentEvent>("agent-event");
const sessionStatusHub = createEventHub<SessionStatusEvent>("session-status");

// OpenCode events from daemon (daemon sends camelCase)
const opencodeEventHub = createTransformingEventHub<
  { workspaceId: string; eventType: string; event: unknown },
  OpenCodeEvent
>("daemon:opencode_event", (raw) => ({
  workspaceId: raw.workspaceId,
  eventType: raw.eventType,
  event: raw.event,
}));

// Agent state events (state machine lifecycle)
const agentStateEventHub = createEventHub<AgentStateEventEnvelope>(
  AGENT_STATE_EVENT_CHANNEL,
);

// Unified streaming events (§4: agent:stream_event)
const streamEventHub = createEventHub<StreamEvent>(STREAM_EVENT_CHANNEL);

// --- Subscription helpers ---

export function subscribeTerminalOutput(
  onEvent: (event: TerminalOutputEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return terminalOutputHub.subscribe(onEvent, options);
}

export function subscribeTerminalExited(
  onEvent: (event: TerminalExitedEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return terminalExitedHub.subscribe(onEvent, options);
}

export function subscribeDaemonConnected(
  onEvent: (event: DaemonConnectionEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return daemonConnectedHub.subscribe(onEvent, options);
}

export function subscribeDaemonDisconnected(
  onEvent: (event: { reason?: string }) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return daemonDisconnectedHub.subscribe(onEvent, options);
}

export function subscribeDaemonDebug(
  onEvent: (event: DaemonDebugEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return daemonDebugHub.subscribe(onEvent, options);
}

export function subscribeAgentEvents(
  onEvent: (event: AgentEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return agentEventHub.subscribe(onEvent, options);
}

export function subscribeSessionStatus(
  onEvent: (event: SessionStatusEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return sessionStatusHub.subscribe(onEvent, options);
}

export function subscribeOpenCodeEvents(
  onEvent: (event: OpenCodeEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return opencodeEventHub.subscribe(onEvent, options);
}

export function subscribeAgentStateEvents(
  onEvent: (event: AgentStateEventEnvelope) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return agentStateEventHub.subscribe(onEvent, options);
}

/**
 * Subscribe to unified streaming events (§4: agent:stream_event).
 *
 * All harness adapters emit StreamEvent envelopes on this channel.
 * Consumers should buffer by streamId and respect seq ordering per §5.
 */
export function subscribeStreamEvents(
  onEvent: (event: StreamEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return streamEventHub.subscribe(onEvent, options);
}

// --- React hook helper ---

export function useTauriEvent<T>(
  subscribe: (handler: (payload: T) => void, options?: SubscriptionOptions) => Unsubscribe,
  handler: (payload: T) => void,
  deps: DependencyList = [],
) {
  useEffect(() => {
    return subscribe(handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
