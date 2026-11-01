import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { WebcamOverlay } from "./WebcamOverlay";
import { call } from "./bridge";
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
const floating = surface === "controls";
const webcam = surface === "webcam";
if (webcam) document.documentElement.classList.add("webcam-window");
if (floating) document.documentElement.classList.add("controls-window");
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {webcam ? <WebcamOverlay /> : floating ? <FloatingRecorder /> : <App />}
  </StrictMode>,
);
