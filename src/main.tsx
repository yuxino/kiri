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
