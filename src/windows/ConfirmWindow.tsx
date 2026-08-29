// ConfirmWindow — a full-screen destructive-confirmation overlay. It dims
// the ENTIRE primary display (not just the library window) and shows a
// centered card, so irreversible operations like emptying the trash or
// permanently deleting a capture are unmistakable. Esc / Cancel closes;
// Confirm closes only after the action succeeds.

import { useEffect, useState } from "react";
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const close = () => {
    void getCurrentWindow().close();
  };

  // Esc cancels the dialog.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy) {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [busy]);

  const confirm = async () => {
    if (busy) return;
    let action: Promise<void> | null = null;
    if (props.kind === "emptyTrash") {
      action = api.emptyTrash();
    } else if (props.kind === "batchDelete") {
      action = api.batchPermanentlyDelete(props.ids ?? []);
    } else if (props.kind.startsWith("delete:")) {
      const id = props.kind.slice("delete:".length);
      action = api.permanentlyDelete(id);
    } else if (props.kind.startsWith("removeMissing:")) {
      const id = props.kind.slice("removeMissing:".length);
      action = api.removeMissingAsset(id);
    }
    if (!action) {
      close();
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await action;
      close();
    } catch {
      setError("Couldn't complete this action");
      setBusy(false);
    }
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
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="kiri-confirm-title"
        aria-describedby={props.message ? "kiri-confirm-message" : undefined}
        style={{
          background: "var(--kiri-elevated)",
          borderRadius: 14,
          padding: 20,
          width: 340,
          border: "1px solid var(--kiri-surface-border)",
          boxShadow: "none",
        }}
      >
        <div id="kiri-confirm-title" style={{ fontSize: 13.5, fontWeight: 600, marginBottom: 6 }}>
          {t(props.title)}
        </div>
        {props.message && (
          <div
            id="kiri-confirm-message"
            style={{
              fontSize: 12.5,
              color: "var(--kiri-secondary-label)",
              marginBottom: 16,
              lineHeight: 1.4,
            }}
          >
            {t(props.message)}
          </div>
        )}
        {error && (
          <div
            role="alert"
            style={{
              fontSize: 12.5,
              color: "var(--kiri-coral)",
              marginBottom: 16,
            }}
          >
            {t(error)}
          </div>
        )}
        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <button
            type="button"
            className="kiri-button kiri-button--secondary"
            disabled={busy}
            autoFocus
            onClick={close}
          >
            {t("Cancel")}
          </button>
          <button
            type="button"
            className="kiri-button kiri-button--destructive kiri-button--destructive-fill"
            disabled={busy}
            onClick={() => void confirm()}
          >
            {t(props.confirmLabel)}
          </button>
        </div>
      </div>
    </div>
  );
}
