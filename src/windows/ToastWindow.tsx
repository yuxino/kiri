// ToastWindow — one resident, operation-local feedback window. Short status
// notices remain click-through; imported captures use an interactive preview
// card without taking focus when it appears.

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
} from "react";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { KiriIcon } from "../components/KiriIcons";
import { t } from "../i18n";
import { api } from "../lib/ipc";

interface NoticePayload {
  id: string;
  title: string;
  symbol: string;
}

interface CompletionPreviewPayload {
  id: string;
  phase: "processing" | "ready";
  assetId: string | null;
  kind: "image" | "video" | "gif";
  title: string;
  detail: string;
  gifEligible: boolean;
  copied: boolean;
}

interface UndoState {
  completion: CompletionPreviewPayload;
}

type ActionName = "open" | "copy" | "gif" | "trash" | "undo";

const COMPLETION_WIDTH = 360;
const COMPLETION_HEIGHT = 124;
const UNDO_HEIGHT = 60;
const ACTION_COOLDOWN_MS = 450;
const PASSIVE_DISMISS_MS = 1200;
const UNDO_DISMISS_MS = 3000;
const COMPLETION_DISMISS_MS = 8000;

function booleanParam(params: URLSearchParams, key: string): boolean {
  const value = params.get(key);
  return value === "1" || value === "true";
}

function initialCompletionFromUrl(): CompletionPreviewPayload | null {
  const params = new URLSearchParams(window.location.search);
  if (params.get("mode") !== "completion") return null;

  const phase = params.get("phase") === "processing" ? "processing" : "ready";
  const rawKind = params.get("kind");
  const kind = rawKind === "video" || rawKind === "gif" ? rawKind : "image";
  const rawAssetId = params.get("assetId");

  return {
    id: params.get("id") ?? "initial-completion",
    phase,
    assetId: rawAssetId && rawAssetId !== "null" ? rawAssetId : null,
    kind,
    title: params.get("title") ?? "",
    detail: params.get("detail") ?? "",
    gifEligible: booleanParam(params, "gifEligible"),
    copied: booleanParam(params, "copied"),
  };
}

function actionErrorMessage(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error) return error;
  return "The action could not be completed.";
}

function previewLabel(kind: CompletionPreviewPayload["kind"]): string {
  if (kind === "video") return t("Open recording preview");
  if (kind === "gif") return t("Open GIF preview");
  return t("Edit screenshot");
}

function ActionButton(props: {
  action: ActionName;
  label: string;
  title?: string;
  icon: Parameters<typeof KiriIcon>[0]["name"];
  pending: ActionName | null;
  accent?: boolean;
  destructive?: boolean;
  onClick: (event: ReactMouseEvent<HTMLButtonElement>) => void;
}) {
  const disabled = props.pending !== null;
  const isPending = props.pending === props.action;
  return (
    <button
      type="button"
      className={`kiri-completion-action${props.accent ? " is-accent" : ""}${
        props.destructive ? " is-destructive" : ""
      }`}
      disabled={disabled}
      title={props.title ?? props.label}
      aria-label={props.title ?? props.label}
      aria-busy={isPending}
      onClick={(event) => {
        if (event.detail > 1) return;
        props.onClick(event);
      }}
    >
      {isPending ? (
        <span className="kiri-completion-button-spinner" aria-hidden="true" />
      ) : (
        <KiriIcon name={props.icon} size={13} />
      )}
      <span>{props.label}</span>
    </button>
  );
}

export function ToastWindow(props: { title?: string; symbol?: string }) {
  const initialCompletion = useRef(initialCompletionFromUrl()).current;
  const [notice, setNotice] = useState<NoticePayload | null>(
    initialCompletion || !props.title
      ? null
      : { id: "initial", title: props.title, symbol: props.symbol ?? "" },
  );
  const [completion, setCompletion] = useState<CompletionPreviewPayload | null>(
    initialCompletion,
  );
  const [undo, setUndo] = useState<UndoState | null>(null);
  const [pendingAction, setPendingAction] = useState<ActionName | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [copiedNow, setCopiedNow] = useState(false);

  const completionRef = useRef(completion);
  const undoRef = useRef<UndoState | null>(null);
  const queuedCompletionRef = useRef<CompletionPreviewPayload | null>(null);
  const actionLockedRef = useRef(false);
  const actionCooldownUntilRef = useRef(0);
  const actionGenerationRef = useRef(0);

  completionRef.current = completion;
  undoRef.current = undo;

  const resetActionState = useCallback(() => {
    actionGenerationRef.current += 1;
    actionLockedRef.current = false;
    setPendingAction(null);
    setActionError(null);
    setCopiedNow(false);
  }, []);

  const hideWindow = useCallback(() => {
    setNotice(null);
    setCompletion(null);
    setUndo(null);
    completionRef.current = null;
    undoRef.current = null;
    queuedCompletionRef.current = null;
    actionCooldownUntilRef.current = 0;
    resetActionState();
    void getCurrentWindow().hide().catch(() => {});
  }, [resetActionState]);

  const presentCompletion = useCallback(
    (payload: CompletionPreviewPayload) => {
      if (undoRef.current) {
        // Preserve the recoverable Undo card. Only the newest subsequent
        // completion matters; it is presented when Undo finishes or expires.
        queuedCompletionRef.current = payload;
        const currentWindow = getCurrentWindow();
        void Promise.all([
          currentWindow.setSize(new LogicalSize(COMPLETION_WIDTH, UNDO_HEIGHT)),
          currentWindow.setIgnoreCursorEvents(false),
        ]).catch(() => {});
        return;
      }
      setNotice(null);
      setCompletion(payload);
      completionRef.current = payload;
      resetActionState();
    },
    [resetActionState],
  );

  const presentNotice = useCallback(
    (payload: NoticePayload) => {
      if (undoRef.current) {
        // Undo is the only time-sensitive, recoverable state. Ignore passive
        // notices until it finishes, and undo the backend's passive-toast
        // resize/click-through switch so the card remains fully interactive.
        const currentWindow = getCurrentWindow();
        void Promise.all([
          currentWindow.setSize(new LogicalSize(COMPLETION_WIDTH, UNDO_HEIGHT)),
          currentWindow.setIgnoreCursorEvents(false),
        ]).catch(() => {});
        return;
      }
      undoRef.current = null;
      queuedCompletionRef.current = null;
      setUndo(null);
      setCompletion(null);
      completionRef.current = null;
      setNotice(payload);
      resetActionState();
    },
    [resetActionState],
  );

  const showQueuedCompletion = useCallback((): boolean => {
    const queued = queuedCompletionRef.current;
    queuedCompletionRef.current = null;
    if (!queued) return false;
    setNotice(null);
    setCompletion(queued);
    completionRef.current = queued;
    resetActionState();
    void getCurrentWindow()
      .setSize(new LogicalSize(COMPLETION_WIDTH, COMPLETION_HEIGHT))
      .catch(() => {});
    return true;
  }, [resetActionState]);

  const finishUndo = useCallback(() => {
    undoRef.current = null;
    setUndo(null);
    if (showQueuedCompletion()) return;
    hideWindow();
  }, [hideWindow, showQueuedCompletion]);

  useEffect(() => {
    let disposed = false;
    let unlisteners: UnlistenFn[] = [];
    void Promise.all([
      listen<NoticePayload>("toast", (event) => presentNotice(event.payload)),
      listen<CompletionPreviewPayload>("completion-preview", (event) =>
        presentCompletion(event.payload),
      ),
      listen("toast-dismiss", hideWindow),
    ])
      .then((resolved) => {
        if (disposed) resolved.forEach((unlisten) => unlisten());
        else unlisteners = resolved;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [hideWindow, presentCompletion, presentNotice]);

  // Passive notices and non-actionable processing cards remain click-through.
  // Only ready cards and Undo need pointer input.
  useEffect(() => {
    const interactive = completion?.phase === "ready" || undo !== null;
    void getCurrentWindow().setIgnoreCursorEvents(!interactive).catch(() => {});
  }, [completion, undo]);

  // Ordinary notices stay just long enough to register without lingering. Ready cards
  // remain for eight seconds; the compact Undo state gets only three seconds.
  // Only an in-flight action pauses dismissal, so focus cannot leave a card
  // stuck forever.
  useEffect(() => {
    if (notice && !completion && !undo) {
      const timer = window.setTimeout(hideWindow, PASSIVE_DISMISS_MS);
      return () => window.clearTimeout(timer);
    }
    if (completion?.phase === "processing") return;
    if (!completion && !undo) return;
    if (pendingAction) return;

    const timer = window.setTimeout(() => {
      if (actionLockedRef.current) return;
      if (undoRef.current) finishUndo();
      else hideWindow();
    }, undo ? UNDO_DISMISS_MS : COMPLETION_DISMISS_MS);
    return () => window.clearTimeout(timer);
  }, [completion, finishUndo, hideWindow, notice, pendingAction, undo]);

  useEffect(() => {
    if (!copiedNow) return;
    const timer = window.setTimeout(() => setCopiedNow(false), 1400);
    return () => window.clearTimeout(timer);
  }, [copiedNow]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || (!completionRef.current && !undoRef.current)) return;
      event.preventDefault();
      if (undoRef.current) finishUndo();
      else hideWindow();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [finishUndo, hideWindow]);

  const runAction = useCallback(
    async (name: ActionName, operation: () => Promise<void>, onSuccess?: () => void) => {
      const now = Date.now();
      if (actionLockedRef.current || now < actionCooldownUntilRef.current) return;
      actionLockedRef.current = true;
      actionCooldownUntilRef.current = now + ACTION_COOLDOWN_MS;
      const generation = actionGenerationRef.current;
      setPendingAction(name);
      setActionError(null);
      try {
        await operation();
        if (generation !== actionGenerationRef.current) return;
        onSuccess?.();
      } catch (error) {
        if (generation === actionGenerationRef.current) {
          setActionError(actionErrorMessage(error));
        }
      } finally {
        if (generation === actionGenerationRef.current) {
          actionLockedRef.current = false;
          setPendingAction(null);
        }
      }
    },
    [],
  );

  if (notice && !completion && !undo) {
    return (
      <div className="kiri-toast-shell">
        <div className="kiri-toast-card" role="status" aria-live="polite">
          {notice.symbol && <KiriIcon name={notice.symbol as never} size={14} />}
          <span>{t(notice.title)}</span>
        </div>
        <ToastStyles />
      </div>
    );
  }

  const visibleCompletion = undo?.completion ?? completion;
  if (!visibleCompletion) return null;

  const assetId = visibleCompletion.assetId;
  const isProcessing = !undo && visibleCompletion.phase === "processing";
  const title = undo ? t("Moved to Trash") : t(visibleCompletion.title);
  const detail = actionError
    ? t(actionError)
    : undo
      ? t("You can restore it from Trash.")
      : t(visibleCompletion.detail);

  const openPreview = (event: ReactMouseEvent<HTMLButtonElement>) => {
    if (event.detail > 1) return;
    if (!assetId || isProcessing || undo) return;
    void runAction(
      "open",
      () =>
        visibleCompletion.kind === "image"
          ? api.openEditor(assetId)
          : api.openAsset(assetId),
      () => hideWindow(),
    );
  };

  const copyAsset = () => {
    if (!assetId) return;
    void runAction("copy", () => api.copyAsset(assetId), () => {
      setCopiedNow(true);
      const current = completionRef.current;
      if (!current || current.id !== visibleCompletion.id) return;
      const copiedCompletion = { ...current, copied: true };
      completionRef.current = copiedCompletion;
      setCompletion(copiedCompletion);
    });
  };

  const createGif = () => {
    if (!assetId) return;
    void runAction("gif", () => api.convertToGif(assetId));
  };

  const moveToTrash = () => {
    if (!assetId) return;
    const deletedCompletion = visibleCompletion;
    void runAction("trash", () => api.moveToTrash(assetId), () => {
      const nextUndo = { completion: deletedCompletion };
      undoRef.current = nextUndo;
      setUndo(nextUndo);
      setCompletion(null);
      completionRef.current = null;
      resetActionState();
      void getCurrentWindow()
        .setSize(new LogicalSize(COMPLETION_WIDTH, UNDO_HEIGHT))
        .catch(() => {});
    });
  };

  const restoreFromTrash = () => {
    if (!assetId) return;
    void runAction("undo", () => api.restoreAsset(assetId), finishUndo);
  };

  return (
    <div className={undo ? "kiri-undo-shell" : "kiri-completion-shell"}>
      {undo ? (
        <section className="kiri-undo-card" aria-label={title}>
          <div
            className={`kiri-undo-message${actionError ? " is-error" : ""}`}
            role="status"
            aria-live="polite"
          >
            <KiriIcon name="trash" size={13} />
            <span>{actionError ? t(actionError) : title}</span>
          </div>
          <ActionButton
            action="undo"
            icon="arrow.uturn.backward"
            label={t("Undo")}
            pending={pendingAction}
            accent
            onClick={restoreFromTrash}
          />
        </section>
      ) : (
        <section className="kiri-completion-card" aria-label={title}>
          <button
            type="button"
            className="kiri-completion-preview"
            disabled={!assetId || isProcessing || pendingAction !== null}
            aria-label={previewLabel(visibleCompletion.kind)}
            title={previewLabel(visibleCompletion.kind)}
            onClick={openPreview}
          >
            {assetId && !isProcessing ? (
              <>
                <img src={`kiri://thumbnail/${assetId}`} alt="" draggable={false} />
                {(visibleCompletion.kind === "video" || visibleCompletion.kind === "gif") && (
                  <span className="kiri-completion-play" aria-hidden="true">
                    <KiriIcon name="play.fill" size={12} />
                  </span>
                )}
              </>
            ) : (
              <span className="kiri-completion-processing" aria-hidden="true">
                <span className="kiri-completion-spinner" />
              </span>
            )}
          </button>

          <div className="kiri-completion-content">
            <div className="kiri-completion-copy" role="status" aria-live="polite">
              <strong>{title}</strong>
              <span className={actionError ? "is-error" : ""} title={detail}>
                {detail}
              </span>
            </div>

            {!isProcessing && (
              <div className="kiri-completion-actions">
                <ActionButton
                  action="copy"
                  icon={copiedNow ? "checkmark.circle.fill" : "doc.on.doc"}
                  label={
                    copiedNow
                      ? t("Copied")
                      : visibleCompletion.kind === "image"
                        ? t(visibleCompletion.copied ? "Copy Again" : "Copy")
                        : t("Copy File")
                  }
                  pending={pendingAction}
                  accent
                  onClick={copyAsset}
                />
                {visibleCompletion.kind === "video" && visibleCompletion.gifEligible && (
                  <ActionButton
                    action="gif"
                    icon="sparkles.rectangle.stack"
                    label={t("GIF")}
                    title={t("Convert to GIF")}
                    pending={pendingAction}
                    onClick={createGif}
                  />
                )}
                <ActionButton
                  action="trash"
                  icon="trash"
                  label={t("Trash")}
                  title={t("Move to Trash")}
                  pending={pendingAction}
                  destructive
                  onClick={moveToTrash}
                />
              </div>
            )}
          </div>
        </section>
      )}
      <ToastStyles />
    </div>
  );
}

function ToastStyles() {
  return (
    <style>{`
      .kiri-toast-shell,
      .kiri-completion-shell,
      .kiri-undo-shell {
        position: fixed;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        box-sizing: border-box;
        background: transparent;
      }

      .kiri-toast-shell { pointer-events: none; }

      .kiri-toast-card {
        display: flex;
        align-items: center;
        gap: 8px;
        max-width: 320px;
        padding: 10px 16px;
        box-sizing: border-box;
        border: 1px solid var(--kiri-surface-border);
        border-radius: 13px;
        background: var(--kiri-elevated);
        box-shadow: none;
        color: var(--kiri-label);
        font-size: 13px;
        font-weight: 500;
        pointer-events: none;
      }

      .kiri-completion-shell,
      .kiri-undo-shell { padding: 6px; }

      .kiri-completion-card {
        width: 100%;
        height: 100%;
        display: grid;
        grid-template-columns: 94px minmax(0, 1fr);
        gap: 10px;
        box-sizing: border-box;
        padding: 8px;
        border: 1px solid var(--kiri-surface-border);
        border-radius: 16px;
        background: var(--kiri-elevated);
        box-shadow: none;
        color: var(--kiri-label);
      }

      .kiri-undo-card {
        width: 100%;
        height: 100%;
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        box-sizing: border-box;
        padding: 7px 8px 7px 13px;
        border: 1px solid var(--kiri-surface-border);
        border-radius: 13px;
        background: var(--kiri-elevated);
        box-shadow: none;
        color: var(--kiri-label);
      }

      .kiri-undo-message {
        min-width: 0;
        display: flex;
        align-items: center;
        gap: 7px;
        color: var(--kiri-secondary-label);
        font: 500 12.5px/18px var(--kiri-font-ui);
      }

      .kiri-undo-message span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .kiri-undo-message.is-error { color: var(--kiri-coral); }

      .kiri-completion-preview {
        position: relative;
        width: 94px;
        height: 94px;
        overflow: hidden;
        padding: 0;
        border: 1px solid var(--kiri-surface-border);
        border-radius: 11px;
        background: var(--kiri-group-fill);
        color: var(--kiri-label);
        cursor: pointer;
        transition:
          border-color var(--kiri-motion-hover) ease-out,
          background var(--kiri-motion-hover) ease-out,
          transform var(--kiri-motion-hover) ease-out;
      }

      .kiri-completion-preview:hover:not(:disabled) {
        border-color: var(--kiri-accent-alpha-32);
        background: var(--kiri-accent-soft-alpha-10);
      }

      .kiri-completion-preview:active:not(:disabled) { transform: scale(0.97); }

      .kiri-completion-preview:disabled {
        cursor: default;
        opacity: 0.72;
      }

      .kiri-completion-preview img {
        width: 100%;
        height: 100%;
        display: block;
        object-fit: contain;
      }

      .kiri-completion-play {
        position: absolute;
        left: 50%;
        top: 50%;
        width: 27px;
        height: 27px;
        display: flex;
        align-items: center;
        justify-content: center;
        transform: translate(-50%, -50%);
        border: 1px solid rgba(255, 255, 255, 0.42);
        border-radius: 50%;
        background: rgba(0, 0, 0, 0.72);
        color: white;
      }

      .kiri-completion-processing {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
      }

      .kiri-completion-spinner,
      .kiri-completion-button-spinner {
        display: inline-block;
        border: 1.5px solid var(--kiri-surface-border);
        border-top-color: var(--kiri-accent);
        border-radius: 50%;
        animation: kiri-completion-spin 0.72s linear infinite;
      }

      .kiri-completion-spinner { width: 20px; height: 20px; }
      .kiri-completion-button-spinner { width: 11px; height: 11px; }

      .kiri-completion-content {
        min-width: 0;
        display: flex;
        flex-direction: column;
        justify-content: space-between;
        padding: 2px 0;
      }

      .kiri-completion-copy {
        min-width: 0;
        display: flex;
        flex-direction: column;
        gap: 3px;
      }

      .kiri-completion-copy strong,
      .kiri-completion-copy span {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }

      .kiri-completion-copy strong {
        color: var(--kiri-label);
        font-size: 13px;
        font-weight: 650;
        line-height: 18px;
      }

      .kiri-completion-copy span {
        color: var(--kiri-secondary-label);
        font-size: 11.5px;
        line-height: 16px;
      }

      .kiri-completion-copy span.is-error { color: var(--kiri-coral); }

      .kiri-completion-actions {
        min-width: 0;
        display: flex;
        align-items: center;
        gap: 5px;
      }

      .kiri-completion-action {
        height: 32px;
        min-width: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 5px;
        padding: 0 8px;
        border: 1px solid var(--kiri-surface-border);
        border-radius: 9px;
        background: var(--kiri-group-fill);
        color: var(--kiri-label);
        font: 600 11.5px var(--kiri-font-ui);
        white-space: nowrap;
        cursor: pointer;
        transition:
          background var(--kiri-motion-hover) ease-out,
          border-color var(--kiri-motion-hover) ease-out,
          color var(--kiri-motion-hover) ease-out,
          transform var(--kiri-motion-hover) ease-out;
      }

      .kiri-completion-action.is-accent {
        border-color: var(--kiri-accent-alpha-18);
        background: var(--kiri-accent-soft-alpha-10);
        color: var(--kiri-accent-strong);
      }

      .kiri-completion-action.is-destructive:hover:not(:disabled) {
        border-color: color-mix(in srgb, var(--kiri-coral) 40%, var(--kiri-surface-border));
        color: var(--kiri-coral);
      }

      .kiri-completion-action:hover:not(:disabled) { border-color: var(--kiri-accent-alpha-32); }

      .kiri-completion-action:active:not(:disabled) { transform: scale(0.96); }

      .kiri-completion-action:disabled {
        color: var(--kiri-disabled-label);
        cursor: default;
        opacity: 0.72;
      }

      .kiri-completion-action:focus-visible,
      .kiri-completion-preview:focus-visible {
        outline: 2px solid var(--kiri-accent);
        outline-offset: 2px;
      }

      @keyframes kiri-completion-spin { to { transform: rotate(360deg); } }

      @media (prefers-reduced-motion: reduce) {
        .kiri-completion-spinner,
        .kiri-completion-button-spinner { animation-duration: 1.4s; }
      }
    `}</style>
  );
}
