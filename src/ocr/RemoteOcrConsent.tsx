import { KiriIcon } from "../components/KiriIcons";
import { fmt, t } from "../i18n";
import type { PreparedOcrRequestDto } from "../lib/ipc";
import type { Rect } from "../annotation/geom";
import { useDialogFocusTrap } from "../settings/useDialogFocusTrap";
import { ocrProviderLabel } from "./providerLabel";
import "./remoteOcrConsent.css";

interface RemoteOcrConsentProps {
  prepared: PreparedOcrRequestDto;
  anchor: Rect;
  bounds: Rect;
  failed: boolean;
  onCancel(): void;
  onUseLocal(): void;
  onSend(): void;
}

export function RemoteOcrConsent({
  prepared,
  anchor,
  bounds,
  failed,
  onCancel,
  onUseLocal,
  onSend,
}: RemoteOcrConsentProps) {
  const dialogRef = useDialogFocusTrap<HTMLElement>();
  const profile = prepared.profile;
  if (!profile) return null;

  const margin = 8;
  const width = Math.min(440, Math.max(320, bounds.width - margin * 2));
  const height = failed ? 350 : 320;
  const maxTop = Math.max(margin, bounds.height - height - margin);
  const below = anchor.y + anchor.height + 10;
  const above = anchor.y - height - 10;
  const belowFits = below + height + margin <= bounds.height;
  const top = Math.min(Math.max(margin, belowFits ? below : above), maxTop);
  const centerX = anchor.x + anchor.width / 2 - width / 2;
  const left = Math.min(
    Math.max(margin, centerX),
    Math.max(margin, bounds.width - width - margin),
  );

  return (
    <section
      ref={dialogRef}
      className="kiri-hud kiri-remote-consent"
      role="dialog"
      aria-modal="true"
      aria-labelledby="remote-ocr-consent-title"
      onPointerDown={(event) => event.stopPropagation()}
      style={{ left, top, width, maxHeight: Math.max(240, bounds.height - margin * 2) }}
    >
      <div
        className="kiri-remote-consent__tail"
        data-above={!belowFits || undefined}
        aria-hidden="true"
      />

      <div className="kiri-remote-consent__header">
        <div>
          <span className="kiri-remote-consent__warning">
            {t("Image leaves this device")}
          </span>
          <h2 id="remote-ocr-consent-title">{t("Send image for remote OCR?")}</h2>
        </div>
        <button
          type="button"
          className="kiri-remote-consent__close"
          onClick={onCancel}
          aria-label={t("Cancel")}
          title={t("Cancel")}
        >
          <KiriIcon name="xmark" size={12} />
        </button>
      </div>

      <p className="kiri-remote-consent__summary">
        {t("Only this selected image will be sent after you click Send or Retry.")}
      </p>

      <dl className="kiri-remote-consent__details">
        <div>
          <dt>{t("Profile")}</dt>
          <dd>{profile.name}</dd>
        </div>
        <div>
          <dt>{t("Provider")}</dt>
          <dd>{ocrProviderLabel(profile.provider)}</dd>
        </div>
        <div>
          <dt>{t("Destination")}</dt>
          <dd title={profile.origin}>{profile.origin}</dd>
        </div>
        <div>
          <dt>{t("Model")}</dt>
          <dd>{profile.model}</dd>
        </div>
        <div>
          <dt>{t("Image")}</dt>
          <dd>
            {fmt("%d × %d px", prepared.imageWidth, prepared.imageHeight)} ·{" "}
            {fmt("%d KB", Math.max(1, Math.ceil(prepared.byteLength / 1024)))}
          </dd>
        </div>
      </dl>

      {failed && (
        <div className="kiri-remote-consent__error" role="alert">
          {t("Remote OCR failed. The image was not sent again.")}
        </div>
      )}

      <div className="kiri-remote-consent__hint">
        {t("Press Return to use local OCR for this image only.")}
      </div>

      <div className="kiri-remote-consent__actions">
        <button
          type="button"
          className="kiri-button kiri-button--secondary"
          onClick={onCancel}
        >
          {t("Cancel")}
        </button>
        <div className="kiri-remote-consent__action-spacer" />
        <button
          type="button"
          className="kiri-button kiri-button--primary"
          onClick={onUseLocal}
          autoFocus
        >
          {t("Use Local This Time")}
        </button>
        <button
          type="button"
          className="kiri-button kiri-button--secondary kiri-remote-consent__send"
          onClick={onSend}
        >
          {failed ? t("Retry Remote OCR") : fmt("Send to %@", profile.name)}
        </button>
      </div>
    </section>
  );
}
