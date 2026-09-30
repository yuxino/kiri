import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";
import "./image-close-guard.css";

export interface ImageCloseGuardHandle {
  requestClose(): Promise<void>;
  closeSaved(): Promise<void>;
}

/** Covers Cancel/Escape and the native window close with the same decision. */
export const ImageCloseGuard = forwardRef<ImageCloseGuardHandle, {
  dirty: boolean; busy: boolean; lock: { readonly locked: boolean }; error: string | null; onSave(): Promise<void>;
}>(function ImageCloseGuard({ dirty, busy, lock, error, onSave }, ref) {
  const state = useRef({ dirty, busy, onSave });
  state.current = { dirty, busy, onSave };
  const allowClose = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const [open, setOpen] = useState(false);
  const closeSaved = async () => {
    allowClose.current = true;
    try { await getCurrentWindow().close(); }
    catch { allowClose.current = false; setOpen(true); }
  };
  const requestClose = useRef(async () => {});
  requestClose.current = async () => {
    if (state.current.busy || lock.locked) return;
    if (state.current.dirty) { setOpen(true); return; }
    await closeSaved();
  };
  useImperativeHandle(ref, () => ({ requestClose: () => requestClose.current(), closeSaved }));
  useEffect(() => {
    let disposed = false;
    const subscription = getCurrentWindow().onCloseRequested(event => {
      if (disposed || allowClose.current) return;
      event.preventDefault(); void requestClose.current();
    });
    const onShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "w" || event.isComposing) return;
      event.preventDefault(); event.stopImmediatePropagation(); void requestClose.current();
    };
    window.addEventListener("keydown", onShortcut, true);
    return () => {
      disposed = true; window.removeEventListener("keydown", onShortcut, true);
      void subscription.then(stop => stop()).catch(() => {});
    };
  }, [lock]);
  useEffect(() => { if (open) dialog.current?.showModal(); else dialog.current?.close(); }, [open]);
  return <dialog ref={dialog} className="kiri-image-close-dialog" aria-hidden={!open}
    aria-labelledby="image-close-title" aria-describedby="image-close-detail"
    onCancel={event => { event.preventDefault(); if (!busy) setOpen(false); }}
    onKeyDown={event => event.stopPropagation()}>
    <strong id="image-close-title">{t("Save changes before closing?")}</strong>
    <p id="image-close-detail">{t("Your image has unsaved changes. Save them, discard them, or keep editing.")}</p>
    {error && <p role="alert">{t(error)}</p>}
    <div>
      <button type="button" autoFocus disabled={busy} className="kiri-button kiri-button--secondary" onClick={() => setOpen(false)}>{t("Keep editing")}</button>
      <button type="button" disabled={busy} className="kiri-button kiri-button--secondary" onClick={() => void closeSaved()}>{t("Discard unsaved changes & close")}</button>
      <button type="button" disabled={busy} className="kiri-primary-button" onClick={() => void state.current.onSave()}>{t(busy ? "Saving edit…" : "Save & close")}</button>
    </div>
  </dialog>;
});
