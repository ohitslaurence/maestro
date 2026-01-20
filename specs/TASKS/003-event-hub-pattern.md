# Task: Implement Event Hub Pattern

## Objective

Port CodexMonitor's event hub pattern to Maestro. This provides efficient event subscription with single native listeners and fan-out to React subscribers.

## Reference

- `reference/codex-monitor/src/services/events.ts` - Event hub implementation
- `reference/codex-monitor/src/features/terminal/hooks/useTerminalSession.ts` - Usage example

## Output

Create `app/src/services/events.ts` with:
- `createEventHub<T>(eventName)` factory function
- Event hubs for: terminal output, agent events, git changes
- Subscription helpers for React hooks

## Implementation Details

### Pattern to Copy

```typescript
function createEventHub<T>(eventName: string) {
  const listeners = new Set<(payload: T) => void>();
  let unlisten: (() => void) | null = null;

  const start = async () => {
    if (unlisten) return;
    unlisten = await listen<T>(eventName, (event) => {
      for (const listener of listeners) {
        try {
          listener(event.payload);
        } catch (error) {
          console.error(`[events] ${eventName} listener failed`, error);
        }
      }
    });
  };

  const stop = () => {
    unlisten?.();
    unlisten = null;
  };

  const subscribe = (onEvent: (payload: T) => void) => {
    listeners.add(onEvent);
    start();
    return () => {
      listeners.delete(onEvent);
      if (listeners.size === 0) stop();
    };
  };

  return { subscribe };
}
```

### Key Points

1. Single native `listen()` call per event type
2. Multiple React subscribers fan out from one listener
3. Try/catch around each subscriber prevents blocking
4. Auto-cleanup when last subscriber unsubscribes

### Events to Implement

- `terminal-output` - PTY data streaming
- `agent-event` - Agent lifecycle events (started, stopped, output)
- `session-status` - Session state changes

### React Hook Helper

```typescript
export function useTauriEvent<T>(
  subscribe: (handler: (payload: T) => void) => () => void,
  handler: (payload: T) => void,
  deps: DependencyList = []
) {
  useEffect(() => {
    return subscribe(handler);
  }, deps);
}
```

## Constraints

- No duplicate native listeners
- Error isolation between subscribers
- Memory-leak safe (cleanup on unmount)
