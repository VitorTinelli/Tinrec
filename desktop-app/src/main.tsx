import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { FloatingRecorder } from "./RecordingControls";
import "./style.css";

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
    event.key.toLowerCase() === "a" &&
    !editableTarget(event.target)
  ) {
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
  }
});

const floating =
  new URLSearchParams(location.search).get("window") === "controls";
if (floating) document.documentElement.classList.add("controls-window");
createRoot(document.getElementById("root")!).render(
  <StrictMode>{floating ? <FloatingRecorder /> : <App />}</StrictMode>,
);
