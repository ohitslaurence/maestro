import { listen } from "@tauri-apps/api/event";
import { useEffect, type DependencyList } from "react";
import type { SessionStatus } from "../types";

export type Unsubscribe = () => void;

type Listener<T> = (payload: T) => void;

type SubscriptionOptions = {
  onError?: (error: unknown) => void;
};

// Event payload types

export type TerminalOutputEvent = {
  sessionId: string;
  terminalId: string;
  data: string;
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

// Event hub factory

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

// Event hubs

const terminalOutputHub = createEventHub<TerminalOutputEvent>("terminal-output");
const agentEventHub = createEventHub<AgentEvent>("agent-event");
const sessionStatusHub = createEventHub<SessionStatusEvent>("session-status");

// Subscription helpers

export function subscribeTerminalOutput(
  onEvent: (event: TerminalOutputEvent) => void,
  options?: SubscriptionOptions,
): Unsubscribe {
  return terminalOutputHub.subscribe(onEvent, options);
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

// React hook helper

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
