// Floating pinned image: always on top, drag to move, no opacity control.

import { getCurrentWindow } from "@tauri-apps/api/window";
import { pinImageUrl } from "../lib/ipc";
import { t } from "../i18n";
import { KiriIcon } from "../components/KiriIcons";

export function PinWindow(props: { id: string }) {
  const close = () => {
    void getCurrentWindow().close();
  };
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        // Spec: content container bg rgb(0.06,0.055,0.09) α0.98, radius 16,
        // white α0.16 border. The window itself is transparent.
        background: "rgba(15, 14, 23, 0.98)",
        borderRadius: 16,
        border: "1px solid rgba(255,255,255,0.16)",
        boxSizing: "border-box",
        overflow: "hidden",
      }}
      data-tauri-drag-region
    >
      <img
        src={pinImageUrl(props.id)}
        alt=""
        draggable={false}
        style={{ maxWidth: "100%", maxHeight: "100%", padding: 7, boxSizing: "border-box" }}
      />
      <button
        onClick={close}
        title={t("Close")}
        style={{
          position: "absolute",
          top: 10,
          right: -10,
          width: 24,
          height: 24,
          borderRadius: 10,
          border: "none",
          background: "rgba(0,0,0,0.58)",
          color: "#fff",
          fontSize: 12,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <KiriIcon name="xmark" size={12} />
      </button>
    </div>
  );
}
