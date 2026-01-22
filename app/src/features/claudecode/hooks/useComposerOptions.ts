import { useCallback, useEffect, useState } from "react";
import { claudeSdkModels } from "../../../services/tauri";
import type { ModelInfo, ThinkingMode } from "../../../types";
import { THINKING_BUDGETS } from "../../../types";

type UseComposerOptionsParams = {
  workspaceId: string | null;
  isConnected: boolean;
};

type UseComposerOptionsReturn = {
  // Model state
  models: ModelInfo[];
  selectedModel: string | undefined;
  setSelectedModel: (modelId: string) => void;
  isLoadingModels: boolean;
  modelsError: string | null;

  // Thinking state
  thinkingMode: ThinkingMode;
  setThinkingMode: (mode: ThinkingMode) => void;

  // Derived value for prompt call
  maxThinkingTokens: number | undefined;

  // Control
  disabled: boolean;
};

/**
 * Hook for managing composer options (model selection, thinking mode).
 * Fetches available models on session connect via `GET /models` (spec §2, §5).
 */
export function useComposerOptions({
  workspaceId,
  isConnected,
}: UseComposerOptionsParams): UseComposerOptionsReturn {
  // Model state
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [selectedModel, setSelectedModel] = useState<string | undefined>(undefined);
  const [isLoadingModels, setIsLoadingModels] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);

  // Thinking state (default: off, per spec §3)
  const [thinkingMode, setThinkingMode] = useState<ThinkingMode>("off");

  // Fetch models when connected (spec §5: Main Flow - Model Selection)
  useEffect(() => {
    if (!workspaceId || !isConnected) {
      return;
    }

    let cancelled = false;

    const fetchModels = async () => {
      setIsLoadingModels(true);
      setModelsError(null);

      try {
        const result = await claudeSdkModels(workspaceId);
        if (cancelled) return;

        setModels(result);
        // Auto-select first model if none selected (spec §10, Q4: use first in list)
        if (result.length > 0 && !selectedModel) {
          setSelectedModel(result[0].value);
        }
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        setModelsError(message);
        console.error("[useComposerOptions] Failed to fetch models:", message);
      } finally {
        if (!cancelled) {
          setIsLoadingModels(false);
        }
      }
    };

    void fetchModels();

    return () => {
      cancelled = true;
    };
  }, [workspaceId, isConnected]); // eslint-disable-line react-hooks/exhaustive-deps

  // Clear state when disconnected
  useEffect(() => {
    if (!isConnected) {
      setModels([]);
      setSelectedModel(undefined);
      setModelsError(null);
      setThinkingMode("off");
    }
  }, [isConnected]);

  // Derive maxThinkingTokens from thinking mode (spec §3)
  const maxThinkingTokens = THINKING_BUDGETS[thinkingMode];

  const handleSetSelectedModel = useCallback((modelId: string) => {
    setSelectedModel(modelId);
  }, []);

  const handleSetThinkingMode = useCallback((mode: ThinkingMode) => {
    setThinkingMode(mode);
  }, []);

  return {
    models,
    selectedModel,
    setSelectedModel: handleSetSelectedModel,
    isLoadingModels,
    modelsError,

    thinkingMode,
    setThinkingMode: handleSetThinkingMode,

    maxThinkingTokens,

    // Dropdowns disabled when loading models or not connected (spec §5)
    disabled: isLoadingModels || !isConnected,
  };
}
