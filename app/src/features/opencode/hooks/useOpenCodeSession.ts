import { useCallback, useState } from "react";
import {
  opencodeSessionCreate,
  opencodeSessionPrompt,
  opencodeSessionAbort,
} from "../../../services/tauri";

type UseOpenCodeSessionOptions = {
  workspaceId: string | null;
};

type SessionCreateResult = {
  id: string;
  [key: string]: unknown;
};

export type OpenCodeSessionState = {
  sessionId: string | null;
  create: (title?: string) => Promise<string | null>;
  prompt: (message: string, overrideSessionId?: string) => Promise<void>;
  abort: () => Promise<void>;
  isPrompting: boolean;
  setSessionId: (id: string | null) => void;
};

export function useOpenCodeSession({
  workspaceId,
}: UseOpenCodeSessionOptions): OpenCodeSessionState {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [isPrompting, setIsPrompting] = useState(false);

  const create = useCallback(
    async (title?: string): Promise<string | null> => {
      if (!workspaceId) {
        console.warn("[opencode] Cannot create session: no workspace");
        return null;
      }
      try {
        const result = (await opencodeSessionCreate(workspaceId, title)) as SessionCreateResult;
        const newId = result.id;
        setSessionId(newId);
        return newId;
      } catch (error) {
        console.error("[opencode] Failed to create session", error);
        return null;
      }
    },
    [workspaceId]
  );

  const prompt = useCallback(
    async (message: string, overrideSessionId?: string): Promise<void> => {
      const targetSessionId = overrideSessionId ?? sessionId;
      if (!workspaceId || !targetSessionId) {
        console.warn("[opencode] Cannot prompt: no workspace or session");
        return;
      }
      setIsPrompting(true);
      try {
        await opencodeSessionPrompt(workspaceId, targetSessionId, message);
      } catch (error) {
        console.error("[opencode] Failed to send prompt", error);
        throw error;
      } finally {
        setIsPrompting(false);
      }
    },
    [workspaceId, sessionId]
  );

  const abort = useCallback(async (): Promise<void> => {
    if (!workspaceId || !sessionId) {
      console.warn("[opencode] Cannot abort: no workspace or session");
      return;
    }
    try {
      await opencodeSessionAbort(workspaceId, sessionId);
    } catch (error) {
      console.error("[opencode] Failed to abort session", error);
      throw error;
    }
  }, [workspaceId, sessionId]);

  return {
    sessionId,
    create,
    prompt,
    abort,
    isPrompting,
    setSessionId,
  };
}
