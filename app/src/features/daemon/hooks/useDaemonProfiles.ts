import { useCallback, useEffect, useMemo, useState } from "react";
import type { DaemonConnectionProfile } from "../../../types";

const STORAGE_KEY = "maestro.daemon.profiles";
const REMEMBER_KEY = "maestro.daemon.rememberLastUsed";
const LAST_USED_KEY = "maestro.daemon.lastUsedId";

type ProfileInput = {
  name?: string;
  host: string;
  port: number;
  token: string;
};

function makeProfileId(host: string, port: number): string {
  return `${host}:${port}`;
}

function loadProfiles(): DaemonConnectionProfile[] {
  if (typeof localStorage === "undefined") {
    return [];
  }

  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed.flatMap((entry) => {
      if (!entry || typeof entry !== "object") {
        return [];
      }

      const host = typeof entry.host === "string" ? entry.host.trim() : "";
      const port = Number(entry.port);
      const token = typeof entry.token === "string" ? entry.token : "";
      const name = typeof entry.name === "string" ? entry.name.trim() : undefined;
      if (!host || !Number.isFinite(port) || !token) {
        return [];
      }

      const id =
        typeof entry.id === "string" && entry.id.length > 0
          ? entry.id
          : makeProfileId(host, port);
      const lastUsedAt =
        typeof entry.lastUsedAt === "number" ? entry.lastUsedAt : undefined;

      return [
        {
          id,
          name,
          host,
          port,
          token,
          lastUsedAt,
        },
      ];
    });
  } catch {
    return [];
  }
}

function loadRememberLastUsed(): boolean {
  if (typeof localStorage === "undefined") {
    return true;
  }

  try {
    const raw = localStorage.getItem(REMEMBER_KEY);
    return raw ? raw === "true" : true;
  } catch {
    return true;
  }
}

function loadLastUsedId(): string | null {
  if (typeof localStorage === "undefined") {
    return null;
  }

  try {
    return localStorage.getItem(LAST_USED_KEY);
  } catch {
    return null;
  }
}

export type DaemonProfilesState = {
  profiles: DaemonConnectionProfile[];
  rememberLastUsed: boolean;
  lastUsedId: string | null;
  lastUsedProfile?: DaemonConnectionProfile;
  upsertProfile: (
    input: ProfileInput,
    previousId?: string | null,
  ) => DaemonConnectionProfile;
  removeProfile: (id: string) => void;
  markLastUsed: (id: string) => void;
  setRememberLastUsed: (value: boolean) => void;
};

export function useDaemonProfiles(): DaemonProfilesState {
  const [profiles, setProfiles] = useState<DaemonConnectionProfile[]>(() =>
    loadProfiles(),
  );
  const [rememberLastUsed, setRememberLastUsedState] = useState<boolean>(() =>
    loadRememberLastUsed(),
  );
  const [lastUsedId, setLastUsedId] = useState<string | null>(() =>
    loadLastUsedId(),
  );

  useEffect(() => {
    if (typeof localStorage === "undefined") {
      return;
    }

    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(profiles));
    } catch {
      // Ignore persistence failures
    }
  }, [profiles]);

  useEffect(() => {
    if (typeof localStorage === "undefined") {
      return;
    }

    try {
      localStorage.setItem(REMEMBER_KEY, rememberLastUsed ? "true" : "false");
    } catch {
      // Ignore persistence failures
    }
  }, [rememberLastUsed]);

  useEffect(() => {
    if (typeof localStorage === "undefined") {
      return;
    }

    try {
      if (lastUsedId) {
        localStorage.setItem(LAST_USED_KEY, lastUsedId);
      } else {
        localStorage.removeItem(LAST_USED_KEY);
      }
    } catch {
      // Ignore persistence failures
    }
  }, [lastUsedId]);

  const sortedProfiles = useMemo(() => {
    return [...profiles].sort((a, b) => {
      const aTime = a.lastUsedAt ?? 0;
      const bTime = b.lastUsedAt ?? 0;
      if (aTime === bTime) {
        return a.host.localeCompare(b.host);
      }
      return bTime - aTime;
    });
  }, [profiles]);

  const lastUsedProfile = useMemo(() => {
    return sortedProfiles.find((profile) => profile.id === lastUsedId);
  }, [sortedProfiles, lastUsedId]);

  const upsertProfile = useCallback(
    (input: ProfileInput, previousId?: string | null) => {
      const id = makeProfileId(input.host, input.port);

      let savedProfile: DaemonConnectionProfile = {
        id,
        name: input.name?.trim() || undefined,
        host: input.host,
        port: input.port,
        token: input.token,
      };

      setProfiles((prev) => {
        const filtered =
          previousId && previousId !== id
            ? prev.filter((profile) => profile.id !== previousId)
            : prev;

        const existingIndex = filtered.findIndex((item) => item.id === id);
        if (existingIndex === -1) {
          return [...filtered, savedProfile];
        }

        const existing = filtered[existingIndex];
        savedProfile = {
          ...existing,
          ...savedProfile,
          name:
            input.name !== undefined
              ? input.name.trim() || undefined
              : existing.name,
        };
        const next = [...filtered];
        next[existingIndex] = savedProfile;
        return next;
      });

      return savedProfile;
    },
    [],
  );

  const removeProfile = useCallback((id: string) => {
    setProfiles((prev) => prev.filter((profile) => profile.id !== id));
    setLastUsedId((prev) => (prev === id ? null : prev));
  }, []);

  const markLastUsed = useCallback((id: string) => {
    setProfiles((prev) =>
      prev.map((profile) =>
        profile.id === id
          ? { ...profile, lastUsedAt: Date.now() }
          : profile,
      ),
    );
    setLastUsedId(id);
  }, []);

  const setRememberLastUsed = useCallback((value: boolean) => {
    setRememberLastUsedState(value);
  }, []);

  return {
    profiles: sortedProfiles,
    rememberLastUsed,
    lastUsedId,
    lastUsedProfile,
    upsertProfile,
    removeProfile,
    markLastUsed,
    setRememberLastUsed,
  };
}
