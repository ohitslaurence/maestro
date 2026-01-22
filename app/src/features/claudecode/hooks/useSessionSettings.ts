import { useCallback, useEffect, useState } from "react";
import { claudeSdkSessionSettingsUpdate } from "../../../services/tauri";
import type {
  SessionSettings,
  SessionSettingsUpdate,
} from "../../../types";
import { DEFAULT_SESSION_SETTINGS } from "../../../types";

type UseSessionSettingsParams = {
  workspaceId: string | null;
  sessionId: string | null;
  /** Initial settings from session data (if available) */
  initialSettings?: SessionSettings;
  /** Called when settings are updated successfully */
  onUpdate?: (settings: SessionSettings) => void;
  /** Called when an error occurs */
  onError?: (error: string) => void;
};

type UseSessionSettingsReturn = {
  /** Current settings state */
  settings: SessionSettings;
  /** Whether a settings update is in progress */
  isUpdating: boolean;
  /** Last update error (cleared on next update attempt) */
  error: string | null;
  /**
   * Update settings with partial changes.
   * Uses optimistic update (§5.1) with rollback on error (§6.2).
   *
   * Merge semantics (§4.1):
   * - undefined field = leave unchanged
   * - null field = reset to default
   * - provided value = set to value
   */
  updateSettings: (patch: SessionSettingsUpdate) => Promise<boolean>;
  /** Reset settings to defaults */
  resetSettings: () => Promise<boolean>;
  /** Sync settings from external source (e.g., SSE event) */
  syncSettings: (settings: SessionSettings) => void;
};

/**
 * Hook for managing Claude SDK session settings.
 *
 * Provides optimistic updates with automatic rollback on error.
 * Settings are persisted via PATCH /session/:id/settings endpoint.
 *
 * Reference: session-settings spec §5.1, §6.2
 */
export function useSessionSettings({
  workspaceId,
  sessionId,
  initialSettings,
  onUpdate,
  onError,
}: UseSessionSettingsParams): UseSessionSettingsReturn {
  // Current settings state
  const [settings, setSettings] = useState<SessionSettings>(
    initialSettings ?? DEFAULT_SESSION_SETTINGS
  );
  const [isUpdating, setIsUpdating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Sync with initial settings when they change (e.g., session load)
  useEffect(() => {
    if (initialSettings) {
      setSettings(initialSettings);
    }
  }, [initialSettings]);

  // Reset when session changes
  useEffect(() => {
    if (!sessionId) {
      setSettings(DEFAULT_SESSION_SETTINGS);
      setError(null);
    }
  }, [sessionId]);

  /**
   * Apply merge semantics locally (§4.1):
   * - undefined = unchanged
   * - null = reset to default
   * - value = set
   */
  const mergeSettings = useCallback(
    (current: SessionSettings, patch: SessionSettingsUpdate): SessionSettings => {
      const merged = { ...current };

      // maxTurns
      if (patch.maxTurns === null) {
        merged.maxTurns = DEFAULT_SESSION_SETTINGS.maxTurns;
      } else if (patch.maxTurns !== undefined) {
        merged.maxTurns = patch.maxTurns;
      }

      // systemPrompt
      if (patch.systemPrompt === null) {
        merged.systemPrompt = { ...DEFAULT_SESSION_SETTINGS.systemPrompt };
      } else if (patch.systemPrompt !== undefined) {
        merged.systemPrompt = { ...merged.systemPrompt };
        if (patch.systemPrompt.mode !== undefined) {
          merged.systemPrompt.mode = patch.systemPrompt.mode;
        }
        if (patch.systemPrompt.content === null) {
          merged.systemPrompt.content = undefined;
        } else if (patch.systemPrompt.content !== undefined) {
          merged.systemPrompt.content = patch.systemPrompt.content;
        }
      }

      // disallowedTools
      if (patch.disallowedTools === null) {
        merged.disallowedTools = undefined;
      } else if (patch.disallowedTools !== undefined) {
        merged.disallowedTools =
          patch.disallowedTools.length > 0 ? [...patch.disallowedTools] : undefined;
      }

      return merged;
    },
    []
  );

  const updateSettings = useCallback(
    async (patch: SessionSettingsUpdate): Promise<boolean> => {
      if (!workspaceId || !sessionId) {
        const msg = "Cannot update settings: no workspace or session";
        console.warn("[useSessionSettings]", msg);
        setError(msg);
        onError?.(msg);
        return false;
      }

      // Clear previous error
      setError(null);
      setIsUpdating(true);

      // Store previous state for rollback (§6.2)
      const previousSettings = settings;

      // Apply optimistic update (§5.1)
      const optimisticSettings = mergeSettings(settings, patch);
      setSettings(optimisticSettings);

      try {
        // PATCH /session/:id/settings via Tauri command
        await claudeSdkSessionSettingsUpdate(workspaceId, sessionId, patch);

        // Success - notify callback
        onUpdate?.(optimisticSettings);
        return true;
      } catch (err) {
        // Rollback on error (§6.2)
        const message = err instanceof Error ? err.message : String(err);
        console.error("[useSessionSettings] Update failed:", message);
        setSettings(previousSettings);
        setError(message);
        onError?.(message);
        return false;
      } finally {
        setIsUpdating(false);
      }
    },
    [workspaceId, sessionId, settings, mergeSettings, onUpdate, onError]
  );

  const resetSettings = useCallback(async (): Promise<boolean> => {
    // Reset all fields to defaults via null values (§4.1)
    return updateSettings({
      maxTurns: null,
      systemPrompt: null,
      disallowedTools: null,
    });
  }, [updateSettings]);

  const syncSettings = useCallback((newSettings: SessionSettings) => {
    setSettings(newSettings);
  }, []);

  return {
    settings,
    isUpdating,
    error,
    updateSettings,
    resetSettings,
    syncSettings,
  };
}
