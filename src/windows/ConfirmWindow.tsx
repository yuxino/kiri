// ConfirmWindow — a full-screen destructive-confirmation overlay. It dims
// the ENTIRE primary display (not just the library window) and shows a
// centered card, so irreversible operations like emptying the trash or
// permanently deleting a capture are unmistakable. Esc / Cancel closes;
// Confirm runs the action and closes.

import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "../lib/ipc";
import { t } from "../i18n";

interface ConfirmProps {
  kind: string;
  title: string;
  message: string;
  confirmLabel: string;
  ids?: string[];
}

export function ConfirmWindow(props: ConfirmProps) {
  const close = () => {
    void getCurrentWindow().close();
  };

  // Esc cancels the dialog.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const confirm = () => {
    if (props.kind === "emptyTrash") {
      void api.emptyTrash().catch(() => {});
    } else if (props.kind === "batchDelete") {
      void api.batchPermanentlyDelete(props.ids ?? []).catch(() => {});
    } else if (props.kind.startsWith("delete:")) {
      const id = props.kind.slice("delete:".length);
      void api.permanentlyDelete(id).catch(() => {});
    }
    close();
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.55)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: "var(--kiri-font-ui)",
      }}
    >
      <div
        style={{
          background: "var(--kiri-elevated)",
          borderRadius: 14,
          padding: 20,
          width: 340,
          border: "1px solid var(--kiri-surface-border)",
          boxShadow: "none",
        }}
      >
        <div style={{ fontSize: 13.5, fontWeight: 600, marginBottom: 6 }}>
          {t(props.title)}
        </div>
        <div
          style={{
            fontSize: 12.5,
            color: "var(--kiri-secondary-label)",
            marginBottom: 16,
            lineHeight: 1.4,
          }}
        >
          {t(props.message)}
        </div>
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <button className="kiri-secondary-button" onClick={close}>
            {t("Cancel")}
          </button>
          <button
            className="kiri-primary-button"
            style={{ background: "var(--kiri-coral)" }}
            onClick={confirm}
          >
            {t(props.confirmLabel)}
          </button>
        </div>
      </div>
    </div>
  );
}
