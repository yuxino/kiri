import { useCallback, useEffect, useId, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { KiriIcon } from "../components/KiriIcons";
import { fmt, getLanguage, setLanguage, t, type KiriLanguage } from "../i18n";
import {
  api,
  onLibraryChanged,
  type LibraryStatusDto,
  type OcrEngineRef,
  type OcrProviderProfileDto,
  type OcrProviderSettingsDto,
  type SaveOcrProviderProfileRequest,
  type ShortcutStatusDto,
} from "../lib/ipc";
import { OcrProfileDialog } from "./OcrProfileDialog";
import { OcrSettingsSection } from "./OcrSettingsSection";
import { useDialogFocusTrap } from "./useDialogFocusTrap";
import "./settings.css";

export function SettingsView() {
  const [settings, setSettings] = useState<OcrProviderSettingsDto | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState(false);
  const [busy, setBusy] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [editingProfile, setEditingProfile] = useState<OcrProviderProfileDto | null | undefined>();
  const [deleteTarget, setDeleteTarget] = useState<OcrProviderProfileDto | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError(false);
    try {
      setSettings(await api.getOcrProviderSettings());
    } catch {
      setLoadError(true);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const activate = async (engine: OcrEngineRef) => {
    if (busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      setSettings(await api.setActiveOcrEngine(engine));
    } catch {
      setOperationError("Couldn't change the OCR engine.");
    } finally {
      setBusy(false);
    }
  };

  const saveProfile = async (request: SaveOcrProviderProfileRequest) => {
    if (busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      setSettings(await api.saveOcrProviderProfile(request));
      setEditingProfile(undefined);
    } catch {
      setOperationError("Couldn't save the OCR profile.");
    } finally {
      setBusy(false);
    }
  };

  const deleteProfile = async () => {
    if (!deleteTarget || busy) return;
    setBusy(true);
    setOperationError(null);
    try {
      setSettings(
        await api.deleteOcrProviderProfile(deleteTarget.id, deleteTarget.revision),
      );
      setDeleteTarget(null);
    } catch {
      setOperationError("Couldn't delete the OCR profile.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="kiri-settings" aria-labelledby="settings-page-title">
      <div className="kiri-settings__content">
        <header className="kiri-settings__intro">
          <div className="kiri-settings__eyebrow">kiri</div>
          <h1 id="settings-page-title">{t("Settings")}</h1>
        </header>

        <GeneralSettingsSection />

        {operationError && (
          <div className="kiri-settings-alert kiri-settings-alert--error" role="alert">
            {t(operationError)}
          </div>
        )}

        {loading ? (
          <div className="kiri-settings-card kiri-settings-loading" role="status">
            <span className="kiri-settings-spinner" aria-hidden="true" />
            {t("Loading OCR settings…")}
          </div>
        ) : loadError || !settings ? (
          <div className="kiri-settings-card kiri-settings-empty-state" role="alert">
            <KiriIcon name="text.viewfinder" size={24} />
            <strong>{t("Couldn't load OCR settings")}</strong>
            <span>{t("Local OCR remains available while you try again.")}</span>
            <button
              type="button"
              className="kiri-button kiri-button--secondary"
              onClick={() => void load()}
            >
              {t("Retry")}
            </button>
          </div>
        ) : (
          <OcrSettingsSection
            settings={settings}
            busy={busy}
            onActivate={(engine) => void activate(engine)}
            onAdd={() => {
              setOperationError(null);
              setEditingProfile(null);
            }}
            onEdit={(profile) => {
              setOperationError(null);
              setEditingProfile(profile);
            }}
            onDelete={(profile) => {
              setOperationError(null);
              setDeleteTarget(profile);
            }}
          />
        )}

        <AboutSettingsSection />
      </div>

      {editingProfile !== undefined && (
        <OcrProfileDialog
          profile={editingProfile}
          busy={busy}
          error={operationError ? t(operationError) : null}
          onCancel={() => {
            if (!busy) {
              setEditingProfile(undefined);
              setOperationError(null);
            }
          }}
          onSave={(request) => void saveProfile(request)}
        />
      )}

      {deleteTarget && (
        <DeleteProfileDialog
          profile={deleteTarget}
          active={
            settings?.activeEngine.kind === "profile" &&
            settings.activeEngine.profileId === deleteTarget.id
          }
          busy={busy}
          error={operationError ? t(operationError) : null}
          onCancel={() => {
            if (!busy) {
              setDeleteTarget(null);
              setOperationError(null);
            }
          }}
          onDelete={() => void deleteProfile()}
        />
      )}
    </main>
  );
}

type UpdateDetails = {
  version: string;
  notes: string | null;
};

type UpdateAction = "check" | "download" | "install" | "relaunch" | "open";

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | ({ kind: "available" } & UpdateDetails)
  | ({ kind: "downloading"; downloaded: number; total?: number } & UpdateDetails)
  | ({ kind: "downloaded" } & UpdateDetails)
  | ({ kind: "installing"; isWindows: boolean } & UpdateDetails)
  | ({ kind: "readyToRestart" } & UpdateDetails)
  | ({ kind: "relaunching" } & UpdateDetails)
  | ({ kind: "error"; action: UpdateAction; details?: UpdateDetails });

function updateDetails(update: Update): UpdateDetails {
  return {
    version: update.version,
    notes: update.body?.trim() || null,
  };
}

function AboutSettingsSection() {
  const [currentVersion, setCurrentVersion] = useState("");
  const [updateState, setUpdateState] = useState<UpdateState>({ kind: "idle" });
  const updateRef = useRef<Update | null>(null);
  const operationRef = useRef(false);
  const mountedRef = useRef(true);
  const isWindows = /Windows/i.test(navigator.userAgent);

  const finishOperation = () => {
    operationRef.current = false;
    if (!mountedRef.current) {
      const update = updateRef.current;
      updateRef.current = null;
      if (update) void update.close().catch(() => {});
    }
  };

  useEffect(() => {
    let active = true;
    mountedRef.current = true;
    void getVersion()
      .then((version) => {
        if (active) setCurrentVersion(version);
      })
      .catch(() => {});
    return () => {
      active = false;
      mountedRef.current = false;
      if (!operationRef.current) {
        const update = updateRef.current;
        updateRef.current = null;
        if (update) void update.close().catch(() => {});
      }
    };
  }, []);

  const checkForUpdates = async () => {
    if (operationRef.current) return;
    operationRef.current = true;
    setUpdateState({ kind: "checking" });
    try {
      const previous = updateRef.current;
      updateRef.current = null;
      if (previous) await previous.close();

      const update = await check({ timeout: 15_000 });
      if (!update) {
        setUpdateState({ kind: "upToDate" });
        return;
      }
      updateRef.current = update;
      setCurrentVersion(update.currentVersion);
      setUpdateState({ kind: "available", ...updateDetails(update) });
    } catch {
      setUpdateState({ kind: "error", action: "check" });
    } finally {
      finishOperation();
    }
  };

  const downloadUpdate = async () => {
    if (operationRef.current) return;
    const update = updateRef.current;
    if (!update) {
      setUpdateState({ kind: "error", action: "check" });
      return;
    }
    operationRef.current = true;
    const details = updateDetails(update);
    let downloaded = 0;
    let total: number | undefined;
    setUpdateState({ kind: "downloading", downloaded, ...details });
    try {
      await update.download((event) => {
        if (event.event === "Started") {
          downloaded = 0;
          total = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
        }
        setUpdateState({ kind: "downloading", downloaded, total, ...details });
      }, { timeout: 120_000 });
      setUpdateState({ kind: "downloaded", ...details });
    } catch {
      setUpdateState({ kind: "error", action: "download", details });
    } finally {
      finishOperation();
    }
  };

  const installUpdate = async () => {
    if (operationRef.current) return;
    const update = updateRef.current;
    if (!update) {
      setUpdateState({ kind: "error", action: "check" });
      return;
    }
    operationRef.current = true;
    const details = updateDetails(update);
    setUpdateState({ kind: "installing", isWindows, ...details });
    try {
      await update.install({ restartAfterInstall: false });
      if (!isWindows) setUpdateState({ kind: "readyToRestart", ...details });
    } catch {
      setUpdateState({ kind: "error", action: "install", details });
    } finally {
      finishOperation();
    }
  };

  const restartUpdatedApp = async () => {
    if (operationRef.current) return;
    const details = stateDetails(updateState);
    if (!details) return;
    operationRef.current = true;
    setUpdateState({ kind: "relaunching", ...details });
    try {
      await relaunch();
    } catch {
      setUpdateState({ kind: "error", action: "relaunch", details });
    } finally {
      finishOperation();
    }
  };

  const openRecoveryPage = async () => {
    if (operationRef.current) return;
    operationRef.current = true;
    const details = stateDetails(updateState);
    try {
      await api.openReleasePage();
    } catch {
      setUpdateState({ kind: "error", action: "open", details: details ?? undefined });
    } finally {
      finishOperation();
    }
  };

  let status = t("Updates are checked only when you choose to check.");
  if (updateState.kind === "checking") {
    status = t("Checking…");
  } else if (updateState.kind === "upToDate") {
    status = fmt("Kiri %@ is up to date.", `v${currentVersion}`);
  } else if (updateState.kind === "available") {
    status = fmt("Kiri %@ is available.", `v${updateState.version}`);
  } else if (updateState.kind === "downloading") {
    status = updateState.total
      ? fmt("Downloading… %@", `${Math.min(100, Math.round((updateState.downloaded / updateState.total) * 100))}%`)
      : t("Downloading…");
  } else if (updateState.kind === "downloaded") {
    status = t("Download and signature verification complete. Ready to install.");
  } else if (updateState.kind === "installing") {
    status = t(
      updateState.isWindows
        ? "Kiri will close while Windows completes the installation."
        : "Installing the signed update…",
    );
  } else if (updateState.kind === "readyToRestart") {
    status = t("Update installed. Restart Kiri when you're ready.");
  } else if (updateState.kind === "relaunching") {
    status = t("Restarting Kiri…");
  } else if (updateState.kind === "error") {
    const errors: Record<UpdateAction, string> = {
      check: "Couldn't check for updates. Try again.",
      download: "Couldn't download the update. Try again.",
      install: "Couldn't install the update. Try again.",
      relaunch: "Couldn't restart Kiri. Try again.",
      open: "Couldn't open the update page. Try again.",
    };
    status = t(errors[updateState.action]);
  }

  const details = stateDetails(updateState);
  const busy = ["checking", "downloading", "installing", "relaunching"].includes(updateState.kind);

  const runPrimaryAction = () => {
    if (busy) return;
    if (updateState.kind === "available") return void downloadUpdate();
    if (updateState.kind === "downloaded") return void installUpdate();
    if (updateState.kind === "readyToRestart") return void restartUpdatedApp();
    if (updateState.kind === "error") {
      if (updateState.action === "download") return void downloadUpdate();
      if (updateState.action === "install") return void installUpdate();
      if (updateState.action === "relaunch") return void restartUpdatedApp();
      if (updateState.action === "open") return void openRecoveryPage();
    }
    return void checkForUpdates();
  };

  const buttonLabel = updateState.kind === "checking"
    ? t("Checking…")
    : updateState.kind === "downloading"
      ? t("Downloading…")
      : updateState.kind === "downloaded"
        ? t("Install Update")
        : updateState.kind === "installing"
          ? t("Installing…")
          : updateState.kind === "readyToRestart" || updateState.kind === "relaunching"
            ? t(updateState.kind === "relaunching" ? "Restarting…" : "Restart and Finish Update")
            : updateState.kind === "available"
              ? t("Download Update")
              : updateState.kind === "error"
                ? t("Retry")
                : t("Check for Updates");

  return (
    <section className="kiri-settings-section" aria-labelledby="about-settings-title">
      <div className="kiri-settings-section__heading">
        <div>
          <h2 id="about-settings-title">{t("About")}</h2>
          <p>
            {t(
              "Check, download, and install updates only when you choose each step.",
            )}
          </p>
        </div>
      </div>
      <div className="kiri-settings-card kiri-version-row">
        <div className="kiri-update-copy">
          <div className="kiri-version-title">
            <strong>Kiri</strong>
            <span className="kiri-settings-badge">
              {fmt("Version %@", currentVersion ? `v${currentVersion}` : "—")}
            </span>
          </div>
          <span
            id="kiri-update-status"
            role={updateState.kind === "error" ? "alert" : "status"}
            aria-live={updateState.kind === "error" ? "assertive" : "polite"}
          >
            {status}
          </span>
          {updateState.kind === "downloading" && (
            <progress
              className="kiri-update-progress"
              max={updateState.total}
              value={updateState.total ? Math.min(updateState.downloaded, updateState.total) : undefined}
              aria-label={t("Update download progress")}
            />
          )}
          {details?.notes && (
            <div className="kiri-update-notes" aria-label={t("Release notes")}>
              <strong>{t("What's new")}</strong>
              <p>{details.notes}</p>
            </div>
          )}
        </div>
        <div className="kiri-update-actions">
          <button
            type="button"
            className="kiri-button kiri-button--secondary kiri-update-button"
            disabled={busy}
            aria-describedby="kiri-update-status"
            onClick={runPrimaryAction}
          >
            {buttonLabel}
          </button>
          {updateState.kind === "error" && (
            <button
              type="button"
              className="kiri-button kiri-button--ghost"
              onClick={() => void openRecoveryPage()}
            >
              {t("Open Releases Page")}
            </button>
          )}
        </div>
      </div>
    </section>
  );
}

function stateDetails(state: UpdateState): UpdateDetails | null {
  switch (state.kind) {
    case "available":
    case "downloading":
    case "downloaded":
    case "installing":
    case "readyToRestart":
    case "relaunching":
      return { version: state.version, notes: state.notes };
    case "error":
      return state.details ?? null;
    default:
      return null;
  }
}

function GeneralSettingsSection() {
  const [language, setCurrentLanguage] = useState<KiriLanguage>(getLanguage());
  const [libraryStatus, setLibraryStatus] = useState<LibraryStatusDto | null>(null);
  const [libraryLoadError, setLibraryLoadError] = useState(false);
  const [libraryBusy, setLibraryBusy] = useState(false);
  const [libraryOperationError, setLibraryOperationError] = useState<string | null>(null);
  const [shortcutStatus, setShortcutStatus] = useState<ShortcutStatusDto | null>(null);
  const [shortcutBusy, setShortcutBusy] = useState(false);
  const libraryStatusGeneration = useRef(0);

  const loadLibraryStatus = useCallback(async (clearOperationErrorOnSuccess = false) => {
    const generation = ++libraryStatusGeneration.current;
    try {
      const status = await api.getLibraryStatus();
      if (generation !== libraryStatusGeneration.current) return;
      setLibraryStatus(status);
      setLibraryLoadError(false);
      if (clearOperationErrorOnSuccess) setLibraryOperationError(null);
    } catch {
      if (generation !== libraryStatusGeneration.current) return;
      setLibraryLoadError(true);
    }
  }, []);

  useEffect(() => {
    void loadLibraryStatus();
    void api.getShortcutStatus().then(setShortcutStatus).catch(() => {});
    const subscription = onLibraryChanged(() => {
      void loadLibraryStatus();
    });
    return () => {
      void subscription.then((dispose) => dispose()).catch(() => {});
    };
  }, [loadLibraryStatus]);

  const switchTo = (next: KiriLanguage) => {
    setCurrentLanguage(next);
    setLanguage(next);
    void api.setLanguage(next).catch(() => {});
  };

  const retryShortcut = async () => {
    if (shortcutBusy) return;
    setShortcutBusy(true);
    try {
      setShortcutStatus(await api.retryShortcut());
    } catch {
      // Preserve the visible occupied state if the IPC layer itself fails.
    } finally {
      setShortcutBusy(false);
    }
  };

  const runLibraryAction = async (
    action: () => Promise<unknown>,
    errorMessage: "Couldn't open folder" | "Couldn't update location",
  ) => {
    if (libraryBusy) return;
    setLibraryBusy(true);
    setLibraryOperationError(null);
    try {
      await action();
      await loadLibraryStatus(true);
    } catch {
      setLibraryOperationError(errorMessage);
      await loadLibraryStatus();
    } finally {
      setLibraryBusy(false);
    }
  };

  const statusLabel = libraryLoadError
    ? t("Unavailable")
    : libraryStatus?.availability === "migrating"
      ? t("Moving…")
      : libraryStatus?.availability === "unavailable"
        ? t("Unavailable")
        : null;
  const locationLabel = libraryStatus
    ? libraryStatus.isDefault
      ? t("Default")
      : libraryStatus.locationLabel
    : libraryLoadError
      ? t("Couldn't load location")
      : "—";

  return (
    <section className="kiri-settings-section" aria-labelledby="general-settings-title">
      <div className="kiri-settings-section__heading">
        <div>
          <h2 id="general-settings-title">{t("General")}</h2>
        </div>
      </div>
      <div className="kiri-settings-card kiri-language-row">
        <div>
          <strong>{t("Language")}</strong>
          <span>{t("Changes apply to every Kiri window.")}</span>
        </div>
        <div className="kiri-language-picker" role="group" aria-label={t("Language")}>
          {(["en", "zh-Hans", "ja"] as const).map((item) => (
            <button
              type="button"
              className="kiri-language-option"
              aria-pressed={language === item}
              data-active={language === item || undefined}
              key={item}
              onClick={() => switchTo(item)}
              title={item === "en" ? "English" : item === "zh-Hans" ? "简体中文" : "日本語"}
            >
              {item === "en" ? "EN" : item === "zh-Hans" ? "中文" : "日本語"}
            </button>
          ))}
        </div>
      </div>
      <div className="kiri-settings-card kiri-shortcut-row">
        <div className="kiri-shortcut-copy">
          <strong>{t("Capture Shortcut")}</strong>
          <span>{shortcutStatus?.label ?? "—"}</span>
        </div>
        <div className="kiri-shortcut-actions">
          {shortcutStatus && (
            <span className="kiri-settings-badge" role="status" aria-live="polite">
              {t(shortcutStatus.status === "enabled" ? "Enabled" : "In Use")}
            </span>
          )}
          {shortcutStatus?.status === "occupied" && (
            <button
              type="button"
              className="kiri-button kiri-button--secondary"
              disabled={shortcutBusy}
              onClick={() => void retryShortcut()}
            >
              {t("Retry")}
            </button>
          )}
        </div>
      </div>
      <div className="kiri-settings-card kiri-storage-row">
        <div className="kiri-storage-copy">
          <div className="kiri-storage-title">
            <strong>{t("Library Location")}</strong>
            {statusLabel && <span className="kiri-settings-badge">{statusLabel}</span>}
          </div>
          <span title={locationLabel}>{locationLabel}</span>
          {libraryOperationError && (
            <span className="kiri-storage-error" role="alert">
              {t(libraryOperationError)}
            </span>
          )}
        </div>
        <div className="kiri-storage-actions">
          {libraryLoadError ? (
            <button
              type="button"
              className="kiri-button kiri-button--secondary"
              disabled={libraryBusy}
              onClick={() => {
                setLibraryOperationError(null);
                void loadLibraryStatus(true);
              }}
            >
              {t("Retry")}
            </button>
          ) : libraryStatus?.availability === "ready" ? (
            <>
              <button
                type="button"
                className="kiri-button kiri-button--secondary"
                disabled={libraryBusy}
                onClick={() =>
                  void runLibraryAction(api.revealLibrary, "Couldn't open folder")
                }
              >
                {t("Open Folder")}
              </button>
              <button
                type="button"
                className="kiri-button kiri-button--secondary"
                disabled={libraryBusy}
                onClick={() =>
                  void runLibraryAction(api.chooseLibraryLocation, "Couldn't update location")
                }
              >
                {t("Change…")}
              </button>
              {!libraryStatus.isDefault && (
                <button
                  type="button"
                  className="kiri-button kiri-button--secondary"
                  disabled={libraryBusy}
                  onClick={() =>
                    void runLibraryAction(api.restoreDefaultLibrary, "Couldn't update location")
                  }
                >
                  {t("Restore Default")}
                </button>
              )}
            </>
          ) : libraryStatus?.availability === "unavailable" ? (
            <>
              <button
                type="button"
                className="kiri-button kiri-button--secondary"
                disabled={libraryBusy}
                onClick={() =>
                  void runLibraryAction(api.retryLibrary, "Couldn't update location")
                }
              >
                {t("Retry")}
              </button>
              <button
                type="button"
                className="kiri-button kiri-button--secondary"
                disabled={libraryBusy}
                onClick={() =>
                  void runLibraryAction(api.locateLibrary, "Couldn't update location")
                }
              >
                {t("Locate…")}
              </button>
            </>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function DeleteProfileDialog({
  profile,
  active,
  busy,
  error,
  onCancel,
  onDelete,
}: {
  profile: OcrProviderProfileDto;
  active: boolean;
  busy: boolean;
  error: string | null;
  onCancel(): void;
  onDelete(): void;
}) {
  const titleId = useId();
  const dialogRef = useDialogFocusTrap<HTMLDivElement>();
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onCancel();
      } else if (event.key === "Enter" || event.key === "Return") {
        // Destructive actions are intentionally never bound to Return.
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [busy, onCancel]);

  return (
    <div className="kiri-settings-dialog-backdrop">
      <div
        ref={dialogRef}
        className="kiri-settings-dialog kiri-settings-dialog--compact"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
      >
        <h2 id={titleId}>{t("Delete OCR Profile?")}</h2>
        <p>{fmt("The profile “%@” and its saved API key will be removed.", profile.name)}</p>
        {active && <p>{t("Kiri will switch back to Local OCR after deletion.")}</p>}
        {error && (
          <div className="kiri-settings-form-error" role="alert">
            {error}
          </div>
        )}
        <div className="kiri-settings-dialog__actions">
          <button
            type="button"
            className="kiri-button kiri-button--secondary"
            onClick={onCancel}
            disabled={busy}
            autoFocus
          >
            {t("Cancel")}
          </button>
          <button
            type="button"
            className="kiri-button kiri-button--destructive kiri-button--destructive-fill"
            onClick={onDelete}
            disabled={busy}
          >
            {busy ? t("Deleting…") : t("Delete Profile")}
          </button>
        </div>
      </div>
    </div>
  );
}
