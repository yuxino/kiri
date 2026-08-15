// ToastWindow — a transient global completion toast shown near the top-center
// of the primary display. Unlike the library-window notice, this stays
// visible even when focus returns to the source application after a
// screenshot or recording, so "Recording Saved" / "Copied to Clipboard"
// feedback is never missed. Borderless, always-on-top, ignores mouse input,
// and hides itself after 2 seconds (AppNotice behavior).

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { t } from "../i18n";
import { KiriIcon } from "../components/KiriIcons";

interface NoticePayload {
  id: string;
  title: string;
  symbol: string;
}

export function ToastWindow(props: { title?: string; symbol?: string }) {
  // Initial content arrives via URL params on first mount; later toasts
  // arrive as "toast" events while the window stays resident.
  const [notice, setNotice] = useState<NoticePayload | null>(
    props.title ? { id: "initial", title: props.title, symbol: props.symbol ?? "" } : null,
  );

  useEffect(() => {
    void getCurrentWindow().setIgnoreCursorEvents(true);
    const unlisten = listen<NoticePayload>("toast", (event) => {
      setNotice(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Hide (not close) 2s after the latest toast so the resident window can
  // be reused for the next completion notice.
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => {
      void getCurrentWindow().hide();
    }, 2000);
    return () => clearTimeout(timer);
  }, [notice]);

  if (!notice) return null;

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "transparent",
        pointerEvents: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          background: "var(--kiri-elevated)",
          border: "1px solid var(--kiri-surface-border)",
          borderRadius: 13,
          padding: "10px 16px",
          boxShadow: "0 8px 18px rgba(0,0,0,0.18)",
          color: "var(--kiri-label)",
          fontSize: 13,
          fontWeight: 500,
          maxWidth: 320,
          boxSizing: "border-box",
          pointerEvents: "none",
        }}
      >
        {notice.symbol && <KiriIcon name={notice.symbol as never} size={14} />}
        <span>{t(notice.title)}</span>
      </div>
    </div>
  );
}
