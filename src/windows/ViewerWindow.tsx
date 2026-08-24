// ViewerWindow — in-app image preview / video player. Opened from the
// library (double-click, "Open", or the quick view button). Esc closes.

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, mediaUrl, type AssetDto } from "../lib/ipc";
import { t } from "../i18n";
import { KiriIcon } from "../components/KiriIcons";

export function ViewerWindow(props: { id: string }) {
  const [asset, setAsset] = useState<AssetDto | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    api
      .getAsset(props.id)
      .then(setAsset)
      .catch(() => setFailed(true));
  }, [props.id]);

  // Esc (or ⌘W) closes the viewer.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "w")) {
        e.preventDefault();
        void getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const close = () => {
    void getCurrentWindow().close();
  };

  const isVideo = asset?.kind === "video";

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "#101014",
        overflow: "hidden",
      }}
    >
      {failed ? (
        <div style={{ color: "rgba(255,255,255,0.6)", font: "400 13px var(--kiri-font-ui)" }}>
          {t("The capture could not be found.")}
        </div>
      ) : isVideo ? (
        <video
          key={props.id}
          src={mediaUrl(props.id)}
          controls
          autoPlay
          preload="metadata"
          onError={() => setFailed(true)}
          style={{ maxWidth: "100%", maxHeight: "100%" }}
        />
      ) : (
        <img
          key={props.id}
          src={mediaUrl(props.id)}
          alt=""
          draggable={false}
          onError={() => setFailed(true)}
          style={{ maxWidth: "100%", maxHeight: "100%", objectFit: "contain" }}
        />
      )}

      {/* Close button (top-right) + hint */}
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
