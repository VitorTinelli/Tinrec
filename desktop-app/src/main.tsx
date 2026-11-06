import { CaptureBorder } from "./CaptureBorder";
import { MonitorAreaWindow } from "./MonitorAreaWindow";
import { AreaSelectionWindow } from "./AreaSelector";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { WebcamOverlay } from "./WebcamOverlay";
import { call } from "./bridge";
import { runInstalledTest } from "./selfTest";
import { FloatingRecorder } from "./RecordingControls";
import "./style.css";
import "./workspace.css";

function editableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    !!target.closest(
      'input:not([type="checkbox"]):not([type="range"]):not([type="button"]):not([type="submit"]),textarea,[contenteditable="true"]',
    )
  );
}
document.addEventListener("selectstart", (event) => {
  if (!editableTarget(event.target)) event.preventDefault();
});
document.addEventListener("keydown", (event) => {
  if (event.defaultPrevented) return;
  if (
    (event.ctrlKey || event.metaKey) &&
    !event.altKey &&
    !event.shiftKey &&
    ["w", ","].includes(event.key.toLowerCase())
  ) {
    event.preventDefault();
    if (!event.repeat)
      void call("local_shortcut", {
        action: event.key === "," ? "settings" : "close",
      }).catch(() => {});
    return;
  }
  if (
    (event.ctrlKey || event.metaKey) &&
    event.key.toLowerCase() === "a" &&
    !editableTarget(event.target)
  ) {
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
  }
});

const surface = new URLSearchParams(location.search).get("window");
function DesktopReady() {
  useEffect(() => {
    if (!surface) void runInstalledTest();
  }, []);
  return null;
}
const captureBorder = surface === "capture-border";
if (captureBorder)
  document.documentElement.classList.add("capture-border-window");
const printArea = surface === "print-area";
const monitorArea = surface === "monitor-area";
const area = surface === "area" || printArea || monitorArea;
if (area)
  document.documentElement.classList.add(
    printArea ? "print-window" : "area-window",
  );
const floating = surface === "controls";
const webcam = surface === "webcam";
if (webcam) document.documentElement.classList.add("webcam-window");
if (floating) document.documentElement.classList.add("controls-window");
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <DesktopReady />
    {surface === "capture-fixture" ? (
      <div
        style={{
          padding: 32,
          minHeight: "100vh",
          boxSizing: "border-box",
          background: "#2468ac",
        }}
      >
        Tinrec — captura de janela em teste
      </div>
    ) : captureBorder ? (
      <CaptureBorder />
    ) : monitorArea ? (
      <MonitorAreaWindow />
    ) : area ? (
      <AreaSelectionWindow />
    ) : webcam ? (
      <WebcamOverlay />
    ) : floating ? (
      <FloatingRecorder />
    ) : (
      <App />
    )}
  </StrictMode>,
);
