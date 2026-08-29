import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { DEFAULT_APPEARANCE, type AppearanceSettings } from "./model";

const SAVE_DELAY_MS = 180;

/**
 * Shares last-used annotation styling across independent Tauri windows.
 * Updates are debounced while sliders move and flushed when a window closes.
 */
export function useAnnotationAppearance(): [
  AppearanceSettings,
  (next: AppearanceSettings) => void,
] {
  const [appearance, setAppearanceState] = useState(DEFAULT_APPEARANCE);
  const [loaded, setLoaded] = useState(false);
  const loadedRef = useRef(false);
  const dirtyRef = useRef(false);
  const latestRef = useRef(DEFAULT_APPEARANCE);

  const setAppearance = useCallback((next: AppearanceSettings) => {
    latestRef.current = next;
    dirtyRef.current = true;
    setAppearanceState(next);
  }, []);

  useEffect(() => {
    let disposed = false;
    void api.getAnnotationAppearance()
      .then((saved) => {
        if (disposed) return;
        if (!dirtyRef.current) {
          latestRef.current = saved;
          setAppearanceState(saved);
        }
      })
      .catch(() => {})
      .finally(() => {
        if (disposed) return;
        loadedRef.current = true;
        setLoaded(true);
      });
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    if (!loaded || !dirtyRef.current) return;
    const pending = appearance;
    const timer = window.setTimeout(() => {
      void api.setAnnotationAppearance(pending)
        .then(() => {
          if (latestRef.current === pending) dirtyRef.current = false;
        })
        .catch(() => {});
    }, SAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [appearance, loaded]);

  useEffect(() => () => {
    if (loadedRef.current && dirtyRef.current) {
      void api.setAnnotationAppearance(latestRef.current).catch(() => {});
    }
  }, []);

  return [appearance, setAppearance];
}
