// PinWindow — a floating pinned image (PinnedImageController.swift):
// floating panel, always on top, drag to move, no opacity control.


import { getCurrentWindow } from "@tauri-apps/api/window";
import { pinImageUrl } from "../lib/ipc";

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
        background: "rgba(255,255,255,0.98)",
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
        title="Close"
        style={{
          position: "absolute",
          top: 6,
          right: 6,
          width: 22,
          height: 22,
          borderRadius: 7,
          border: "none",
          background: "rgba(0,0,0,0.08)",
          color: "#333",
          fontSize: 11,
          cursor: "default",
        }}
      >
        ✕
      </button>
    </div>
  );
}
