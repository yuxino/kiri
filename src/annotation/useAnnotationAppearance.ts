import { useCallback, useEffect, useRef, useState } from "react";
import { api, onAnnotationAppearanceChanged } from "../lib/ipc";
import { DEFAULT_APPEARANCE, type AppearanceSettings } from "./model";
import { AppearanceUpdates } from "./appearance-updates.js";

const SAVE_DELAY_MS = 180;

/** Shares styling without overwriting unrelated cross-window edits. */
export function useAnnotationAppearance(): [AppearanceSettings, (next: AppearanceSettings) => void] {
  const updates = useRef(new AppearanceUpdates(DEFAULT_APPEARANCE));
  const [appearance, setAppearanceState] = useState(DEFAULT_APPEARANCE);
  const [loaded, setLoaded] = useState(false);
  const loadedRef = useRef(false);
  const activeRef = useRef(false);
  // Legacy preferences can still contain "single". New watermark tools only
  // offer tiling; reading preferences does not migrate any saved annotation.
  const publish = useCallback(() => setAppearanceState({...updates.current.current, watermarkMode: "tiled"}), []);
  const setAppearance = useCallback((next: AppearanceSettings) => {
    updates.current.update({...next, watermarkMode: "tiled"}, appearance);
    publish();
  }, [appearance, publish]);

  useEffect(() => {
    activeRef.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const stop = await onAnnotationAppearanceChanged(saved => {
          if (!disposed) { updates.current.receive(saved); publish(); }
        });
        if (disposed) { stop(); return; }
        unlisten = stop;
        const events = updates.current.events;
        const saved = await api.getAnnotationAppearance();
        if (!disposed && updates.current.events === events) {
          updates.current.receive(saved);
          publish();
        }
      } catch { /* Retain local styling when preferences are unavailable. */ }
      if (!disposed) { loadedRef.current = true; setLoaded(true); }
    })();
    return () => { disposed = true; activeRef.current = false; unlisten?.(); };
  }, [publish]);

  const savePending = useCallback(async function save() {
    const patch = updates.current.beginSave();
    if (!patch) return;
    try {
      const saved = await api.setAnnotationAppearance(patch);
      updates.current.finishSave(saved);
      if (activeRef.current) publish();
      // Closing preserves serial order even when a slider changed in flight.
      else if (updates.current.hasPending) void save();
    } catch { updates.current.failSave(); }
  }, [publish]);

  useEffect(() => {
    if (!loaded || !updates.current.hasPending || updates.current.inFlight) return;
    const timer = window.setTimeout(() => { void savePending(); }, SAVE_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [appearance, loaded, savePending]);

  useEffect(() => () => {
    if (loadedRef.current && updates.current.hasPending) void savePending();
  }, [savePending]);
  return [appearance, setAppearance];
}
