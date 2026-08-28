// ViewerWindow — in-app image preview / video player. Opened from the
// library (double-click, "Open", or the quick view button). Esc closes.

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, mediaUrl, onAssetContentChanged, type AssetAvailability } from "../lib/ipc";
import { t } from "../i18n";
import { KiriIcon } from "../components/KiriIcons";
import {
  createViewerLoadingState,
  createViewerReadyState,
  viewerMediaKind,
  viewerStateAfterFailure,
  type ViewerState,
} from "./viewer-state.js";

export function ViewerWindow(props: { id: string }) {
  const [state, setState] = useState<ViewerState>(createViewerLoadingState());
  const [mediaRevision, setMediaRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const loadGeneration = useRef(0);

  const loadAsset = useCallback(async () => {
    const generation = ++loadGeneration.current;
    setState(createViewerLoadingState());
    setConfirmRemove(false);
    setOperationError(null);
    try {
      const asset = await api.getAsset(props.id);
      if (generation === loadGeneration.current) {
        setState(createViewerReadyState(asset));
      }
    } catch {
      let availability: AssetAvailability | null = null;
      try {
        availability = (await api.getAssetAvailability(props.id)).status;
      } catch {
        // Both metadata paths failed. Do not mislabel the file as missing.
      }
      if (generation === loadGeneration.current) {
        setState(viewerStateAfterFailure(null, availability));
      }
    }
  }, [props.id]);

  useEffect(() => {
    void loadAsset();
    return () => {
      loadGeneration.current += 1;
    };
  }, [loadAsset]);

  useEffect(() => {
    const subscription = onAssetContentChanged((assetId) => {
      if (assetId !== props.id) return;
      setMediaRevision((revision) => revision + 1);
      void loadAsset();
    });
    return () => {
      void subscription.then((dispose) => dispose()).catch(() => {});
    };
  }, [loadAsset, props.id]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" ||
        ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "w")
      ) {
        event.preventDefault();
        void getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const close = () => {
    void getCurrentWindow().close();
  };

  const retry = async () => {
    if (busy) return;
    setBusy(true);
    try {
      setMediaRevision((revision) => revision + 1);
      await loadAsset();
    } finally {
      setBusy(false);
    }
  };

  const handleMediaError = async () => {
    if (state.kind !== "ready") return;
    const failedAsset = state.asset;
    const generation = loadGeneration.current;
    let availability: AssetAvailability | null = null;
    try {
      availability = (await api.getAssetAvailability(failedAsset.id)).status;
    } catch {
      // An availability check failure is unreadable, never proof of deletion.
    }
    if (generation === loadGeneration.current) {
      setState((current) =>
        current.kind === "ready" && current.asset.id === failedAsset.id
          ? viewerStateAfterFailure(failedAsset, availability)
          : current,
      );
    }
  };

  const restoreMissing = async () => {
    if (busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      const restored = await api.restoreMissingAsset(props.id);
      if (restored) {
        setMediaRevision((revision) => revision + 1);
        await loadAsset();
      }
    } catch {
      setOperationError("Couldn't restore this file");
    } finally {
      setBusy(false);
    }
  };

  const removeMissing = async () => {
    if (busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      await api.removeMissingAsset(props.id);
      close();
    } catch {
      setOperationError("Couldn't update this item");
    } finally {
      setBusy(false);
    }
  };

  const mediaKind = viewerMediaKind(state);

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "#080808",
        overflow: "hidden",
      }}
    >
      {state.kind === "loading" ? (
        <ViewerMessage title={t("Loading…")} />
      ) : state.kind === "missing" && confirmRemove ? (
        <ViewerMessage
          title={t("Remove this record?")}
          detail={operationError ? t(operationError) : undefined}
          actions={
            <>
              <ViewerButton
                disabled={busy}
                onClick={() => {
                  setOperationError(null);
                  setConfirmRemove(false);
                }}
              >
                {t("Cancel")}
              </ViewerButton>
              <ViewerButton destructive disabled={busy} onClick={() => void removeMissing()}>
                {t("Remove Record")}
              </ViewerButton>
            </>
          }
        />
      ) : state.kind === "missing" ? (
        <ViewerMessage
          title={t("File missing")}
          detail={operationError ? t(operationError) : undefined}
          actions={
            <>
              <ViewerButton disabled={busy} onClick={() => void restoreMissing()}>
                {t("Restore File…")}
              </ViewerButton>
              <ViewerButton
                disabled={busy}
                onClick={() => {
                  setOperationError(null);
                  setConfirmRemove(true);
                }}
              >
                {t("Remove Record")}
              </ViewerButton>
            </>
          }
        />
      ) : state.kind === "unreadable" ? (
        <ViewerMessage
          title={t(
            state.availabilityUnknown
              ? "Couldn't load this item"
              : state.libraryUnavailable
                ? "Library unavailable"
                : "Can't read this file",
          )}
          actions={
            <>
              <ViewerButton disabled={busy} onClick={() => void retry()}>
                {t("Retry")}
              </ViewerButton>
              {!state.libraryUnavailable && state.asset && (
                <ViewerButton onClick={() => void api.revealAsset(state.asset!.id).catch(() => {})}>
                  {t("Show in Folder")}
                </ViewerButton>
              )}
            </>
          }
        />
      ) : state.kind === "playbackFailed" ? (
        <ViewerMessage
          title={t(
            state.asset.kind === "video"
              ? "Couldn't play this video."
              : "Couldn't open this image.",
          )}
          actions={
            <>
              <ViewerButton disabled={busy} onClick={() => void retry()}>
                {t("Retry")}
              </ViewerButton>
              <ViewerButton onClick={() => void api.revealAsset(state.asset.id).catch(() => {})}>
                {t("Show in Folder")}
              </ViewerButton>
            </>
          }
        />
      ) : mediaKind === "video" ? (
        <video
          key={`${props.id}:${mediaRevision}`}
          src={mediaUrl(props.id)}
          controls
          autoPlay
          preload="metadata"
          onError={() => void handleMediaError()}
          style={{ maxWidth: "100%", maxHeight: "100%" }}
        />
      ) : mediaKind === "image" ? (
        <img
          key={`${props.id}:${mediaRevision}`}
          src={mediaUrl(props.id)}
          alt=""
          draggable={false}
          onError={() => void handleMediaError()}
          style={{ maxWidth: "100%", maxHeight: "100%", objectFit: "contain" }}
        />
      ) : null}

      <button
        onClick={close}
        title={t("Close · Esc")}
        style={{
          position: "absolute",
          top: 12,
          right: 12,
          width: 30,
          height: 30,
          borderRadius: 10,
          border: "1px solid rgba(255,255,255,0.16)",
          background: "rgba(0,0,0,0.55)",
          color: "#fff",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <KiriIcon name="xmark" size={14} />
      </button>
      <div
        style={{
          position: "absolute",
          bottom: 10,
          left: "50%",
          transform: "translateX(-50%)",
          color: "rgba(255,255,255,0.45)",
          font: "400 11px var(--kiri-font-ui)",
          pointerEvents: "none",
        }}
      >
        {t("Esc to close")}
      </div>
    </div>
  );
}

function ViewerMessage(props: { title: string; detail?: string; actions?: ReactNode }) {
  return (
    <div
      role="status"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 14,
        color: "rgba(255,255,255,0.62)",
        font: "400 13px var(--kiri-font-ui)",
      }}
    >
      <span>{props.title}</span>
      {props.detail && (
        <span role="alert" style={{ color: "rgba(255,255,255,0.48)", fontSize: 11.5 }}>
          {props.detail}
        </span>
      )}
      {props.actions && <div style={{ display: "flex", gap: 8 }}>{props.actions}</div>}
    </div>
  );
}

function ViewerButton(props: {
  children: ReactNode;
  destructive?: boolean;
  disabled?: boolean;
  onClick(): void;
}) {
  return (
    <button
      type="button"
      disabled={props.disabled}
      onClick={props.onClick}
      style={{
        minHeight: 30,
        padding: "0 11px",
        border: "1px solid rgba(255,255,255,0.18)",
        borderRadius: 9,
        background: "rgba(255,255,255,0.07)",
        color: props.disabled
          ? "rgba(255,255,255,0.36)"
          : props.destructive
            ? "var(--kiri-coral)"
            : "rgba(255,255,255,0.84)",
        font: "600 11.5px var(--kiri-font-ui)",
        cursor: props.disabled ? "default" : "pointer",
      }}
    >
      {props.children}
    </button>
  );
}
