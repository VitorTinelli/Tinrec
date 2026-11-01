import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Video } from "lucide-react";
import { call, subscribe, type Preview } from "./bridge";

export function WebcamOverlay() {
  const [preview, setPreview] = useState<Preview | null>(null);
  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const register = (stop: () => void) =>
      disposed ? stop() : stops.push(stop);
    let sessionId: string | null | undefined;
    let enabled = true;
    const accept = (frame: Preview | null) => {
      if (
        !disposed &&
        frame &&
        enabled &&
        (!sessionId || frame.sessionId === sessionId)
      ) {
        setPreview((previous) =>
          !previous ||
          previous.sessionId !== frame.sessionId ||
          frame.sequence >= previous.sequence
            ? frame
            : previous,
        );
      }
    };
    listen<Preview>("webcam-preview", (event) => accept(event.payload))
      .then(register)
      .catch(() => {});
    subscribe((snapshot) => {
      enabled =
        !!snapshot.webcamEnabled &&
        ["recording", "paused"].includes(snapshot.phase);
      sessionId = snapshot.sessionId;
      if (!enabled || !["recording", "paused"].includes(snapshot.phase))
        setPreview(null);
    })
      .then(register)
      .catch(() => {});
    void call<Preview | null>("get_webcam_preview")
      .then(accept)
      .catch(() => {});
    return () => {
      disposed = true;
      stops.forEach((stop) => stop());
    };
  }, []);
  return (
    <div
      className="webcam-overlay"
      data-tauri-drag-region
      role="img"
      aria-label="Webcam — arraste para mover"
      title="Arrastar webcam"
    >
      {preview ? (
        <img src={preview.image} alt="" draggable={false} />
      ) : (
        <Video size={32} aria-hidden="true" />
      )}
    </div>
  );
}
