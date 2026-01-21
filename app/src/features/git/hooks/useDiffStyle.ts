import { useState, useCallback } from "react";

export type DiffStyle = "split" | "unified";

const STORAGE_KEY = "maestro:diffStyle";

export function useDiffStyle() {
  const [diffStyle, setDiffStyleState] = useState<DiffStyle>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "unified" ? "unified" : "split";
  });

  const setDiffStyle = useCallback((style: DiffStyle) => {
    setDiffStyleState(style);
    localStorage.setItem(STORAGE_KEY, style);
  }, []);

  return { diffStyle, setDiffStyle };
}
