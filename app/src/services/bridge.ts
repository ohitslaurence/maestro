type TauriWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
};

const isTauriRuntime =
  typeof window !== "undefined" &&
  typeof (window as TauriWindow).__TAURI_INTERNALS__ !== "undefined";

export const isWebRuntime = !isTauriRuntime;

export async function invoke<T>(
  command: string,
  payload?: Record<string, unknown>,
): Promise<T> {
  if (isTauriRuntime) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, payload);
  }

  const { webInvoke } = await import("./web/daemon");
  return webInvoke<T>(command, payload);
}

export async function listen<T>(
  eventName: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  if (isTauriRuntime) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(eventName, handler);
  }

  const { webListen } = await import("./web/daemon");
  return webListen<T>(eventName, handler);
}
