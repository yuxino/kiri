import {useEffect, useRef, useState} from "react";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {t} from "../i18n";

/** The window close button, Escape and Cmd/Ctrl+W share the same guard. */
export function VideoCloseGuard({dirty, busy, hasPendingEdits}: {dirty: boolean; busy: boolean; hasPendingEdits?(): boolean}) {
  const state = useRef({dirty, busy, hasPendingEdits});
  state.current = {dirty, busy, hasPendingEdits};
  const allowClose = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    let disposed = false;
    const subscription = getCurrentWindow().onCloseRequested(event => {
      if (disposed || allowClose.current || (!state.current.dirty && !state.current.busy && !state.current.hasPendingEdits?.())) return;
      event.preventDefault();
      setOpen(true);
    });
    return () => {disposed = true; void subscription.then(stop => stop()).catch(() => {});};
  }, []);
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else dialog.current?.close();
  }, [open]);
  return <dialog ref={dialog} role="dialog" className="kiri-video-close-dialog"
    aria-labelledby="video-close-title" aria-describedby="video-close-detail"
    onCancel={event => {event.preventDefault(); setOpen(false);}}
    onKeyDown={event => event.stopPropagation()}>
    <strong id="video-close-title">{t(busy ? "Export in progress" : "Close without exporting?")}</strong>
    <p id="video-close-detail">{t(busy
      ? "Wait for the export to finish before closing this window."
      : "Your cuts, text and effects will be lost when this window closes. Export a copy to keep the finished video.")}</p>
    <div>
      <button type="button" autoFocus className="kiri-button kiri-button--primary" onClick={() => setOpen(false)}>{t("Keep editing")}</button>
      {!busy && <button type="button" className="kiri-button kiri-button--secondary" onClick={() => {
        allowClose.current = true;
        void getCurrentWindow().close().catch(() => {allowClose.current = false;});
      }}>{t("Close without exporting")}</button>}
    </div>
  </dialog>;
}
