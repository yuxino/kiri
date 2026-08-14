import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/design-system.css";
import { OverlayWindow } from "./windows/OverlayWindow";
import { LibraryWindow } from "./windows/LibraryWindow";
import { CountdownWindow } from "./windows/CountdownWindow";
import { ControlPanelWindow } from "./windows/ControlPanelWindow";
import { RippleWindow } from "./windows/RippleWindow";
import { PinWindow } from "./windows/PinWindow";
import { EditorWindow } from "./windows/EditorWindow";

// Surface runtime errors as a compact bottom banner (never blocks the UI),
// and forward the message to the Rust log for diagnosis.
function installErrorDiagnostics() {
  const report = (message: string) => {
    try {
      void import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke("log_frontend_error", { message: message.slice(0, 2000) });
      });
    } catch {
      // not in a Tauri context
    }
    document.title = `kiri [error] ${message.slice(0, 80)}`;
    const box = document.createElement("pre");
    box.style.cssText =
      "position:fixed;left:12px;right:12px;bottom:12px;margin:0;padding:10px 14px;" +
      "background:rgba(30,27,40,0.95);color:#fa476e;border:1px solid rgba(250,71,110,0.5);" +
      "border-radius:10px;font:11px ui-monospace,Menlo,monospace;white-space:pre-wrap;" +
      "z-index:99999;max-height:160px;overflow:auto";
    box.textContent = message;
    document.body.appendChild(box);
  };
  window.addEventListener("error", (event) => {
    report(`window.onerror: ${event.message}\n${event.filename ?? ""}:${event.lineno ?? 0}`);
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const message =
      reason instanceof Error ? `${reason.message}\n${reason.stack ?? ""}` : String(reason);
    report(`unhandledrejection: ${message}`);
  });
}
installErrorDiagnostics();

function resolveWindow(): { kind: string; params: URLSearchParams } {
  const params = new URLSearchParams(window.location.search);
  return { kind: params.get("window") ?? "library", params };
}

function App() {
  const { kind, params } = resolveWindow();
  document.title = `${kind}-alive`;
  switch (kind) {
    case "overlay":
      return <OverlayWindow />;
    case "countdown":
      return <CountdownWindow />;
    case "control-panel":
      return <ControlPanelWindow />;
    case "ripple":
      return <RippleWindow />;
    case "pin":
      return <PinWindow id={params.get("id") ?? ""} />;
    case "editor":
      return <EditorWindow id={params.get("id") ?? ""} />;
    default:
      return <LibraryWindow />;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
