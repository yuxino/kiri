import { useCallback, useEffect, useId, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { KiriIcon } from "../components/KiriIcons";
import { fmt, getLanguage, setLanguage, t, type KiriLanguage } from "../i18n";
import {
  api,
  type OcrEngineRef,
  type OcrProviderProfileDto,
  type OcrProviderSettingsDto,
  type SaveOcrProviderProfileRequest,
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
          <p>{t("Keep capture preferences simple and choose where text recognition runs.")}</p>
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

type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate"; currentVersion: string }
  | { kind: "available"; latestVersion: string }
  | { kind: "error"; action: "check" | "open"; latestVersion?: string };

function AboutSettingsSection() {
  const [currentVersion, setCurrentVersion] = useState("");
  const [updateState, setUpdateState] = useState<UpdateState>({ kind: "idle" });

  useEffect(() => {
    let active = true;
    void getVersion()
      .then((version) => {
        if (active) setCurrentVersion(version);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, []);

  const checkForUpdates = async () => {
    if (updateState.kind === "checking") return;
    setUpdateState({ kind: "checking" });
    try {
      const result = await api.checkForUpdates();
      setCurrentVersion(result.currentVersion);
      setUpdateState(
        result.updateAvailable
          ? { kind: "available", latestVersion: result.latestVersion }
          : { kind: "upToDate", currentVersion: result.currentVersion },
      );
    } catch {
      setUpdateState({ kind: "error", action: "check" });
    }
  };

  const openUpdate = async (latestVersion: string) => {
    try {
      await api.openReleasePage();
    } catch {
      setUpdateState({ kind: "error", action: "open", latestVersion });
    }
  };

  const opensUpdate =
    updateState.kind === "available" ||
    (updateState.kind === "error" && updateState.action === "open");
  const latestVersion =
    updateState.kind === "available" || updateState.kind === "error"
      ? updateState.latestVersion
      : undefined;

  let status = t("Check GitHub Releases for a newer version.");
  if (updateState.kind === "checking") {
    status = t("Checking…");
  } else if (updateState.kind === "upToDate") {
    status = fmt("Kiri %@ is up to date.", `v${updateState.currentVersion}`);
  } else if (updateState.kind === "available") {
    status = fmt("Kiri %@ is available.", `v${updateState.latestVersion}`);
  } else if (updateState.kind === "error") {
    status = t(
      updateState.action === "open"
        ? "Couldn't open the update page. Try again."
        : "Couldn't check for updates. Try again.",
    );
  }

  const buttonLabel = updateState.kind === "checking"
    ? t("Checking…")
    : opensUpdate
      ? t("View Update")
      : t("Check for Updates");

  return (
    <section className="kiri-settings-section" aria-labelledby="about-settings-title">
      <div className="kiri-settings-section__heading">
        <div>
          <h2 id="about-settings-title">{t("About")}</h2>
          <p>
            {t(
              "Check for updates manually. Kiri never downloads or installs them automatically.",
            )}
          </p>
        </div>
      </div>
      <div className="kiri-settings-card kiri-version-row">
        <div>
          <div className="kiri-version-title">
            <strong>Kiri</strong>
            <span className="kiri-settings-badge">
              {fmt("Version %@", currentVersion ? `v${currentVersion}` : "—")}
            </span>
          </div>
          <span id="kiri-update-status" role="status" aria-live="polite">
            {status}
          </span>
        </div>
        <button
          type="button"
          className="kiri-button kiri-button--secondary kiri-update-button"
          disabled={updateState.kind === "checking"}
          aria-describedby="kiri-update-status"
          onClick={() => {
            if (opensUpdate && latestVersion) {
              void openUpdate(latestVersion);
            } else {
              void checkForUpdates();
            }
          }}
        >
          {buttonLabel}
        </button>
      </div>
    </section>
  );
}

function GeneralSettingsSection() {
  const [language, setCurrentLanguage] = useState<KiriLanguage>(getLanguage());

  const switchTo = (next: KiriLanguage) => {
    setCurrentLanguage(next);
    setLanguage(next);
    void api.setLanguage(next).catch(() => {});
  };

  return (
    <section className="kiri-settings-section" aria-labelledby="general-settings-title">
      <div className="kiri-settings-section__heading">
        <div>
          <h2 id="general-settings-title">{t("General")}</h2>
          <p>{t("Choose the language used throughout Kiri.")}</p>
        </div>
      </div>
      <div className="kiri-settings-card kiri-language-row">
        <div>
          <strong>{t("Language")}</strong>
          <span>{t("Changes apply to every Kiri window.")}</span>
        </div>
        <div className="kiri-language-picker" role="radiogroup" aria-label={t("Language")}>
          {(["en", "zh-Hans", "ja"] as const).map((item) => (
            <button
              type="button"
              role="radio"
              aria-checked={language === item}
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
            className="kiri-button kiri-button--destructive kiri-settings-destructive-fill"
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
