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

// Surface runtime errors inside the window instead of a blank page.
function installErrorDiagnostics() {
  const show = (message: string) => {
    document.title = `kiri [error] ${message.slice(0, 80)}`;
    const box = document.createElement("pre");
    box.style.cssText =
      "position:fixed;inset:0;margin:0;padding:24px;background:#1e1b28;color:#fa476e;" +
      "font:12px ui-monospace,Menlo,monospace;white-space:pre-wrap;z-index:99999";
    box.textContent = message;
    document.body.appendChild(box);
  };
  window.addEventListener("error", (event) => {
    show(`window.onerror: ${event.message}\n${event.filename ?? ""}:${event.lineno ?? 0}`);
  });
  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const message =
      reason instanceof Error ? `${reason.message}\n${reason.stack ?? ""}` : String(reason);
    show(`unhandledrejection: ${message}`);
  });
}
installErrorDiagnostics();

function resolveWindow(): { kind: string; params: URLSearchParams } {
  const params = new URLSearchParams(window.location.search);
  return { kind: params.get("window") ?? "library", params };
}

function App() {
  const { kind, params } = resolveWindow();
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
