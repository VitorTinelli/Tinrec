import { useEffect, useRef, useState, type PointerEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  call,
  message,
  type Preview,
  idle,
  subscribe,
  type Snapshot,
} from "./bridge";
import { PrintOverlay } from "./screenshots/PrintOverlay";
type Point = { x: number; y: number };
export function selectedPixels(
  start: Point,
  end: Point,
  width: number,
  height: number,
) {
  const x = Math.max(
    0,
    Math.min(width - 1, Math.floor(Math.min(start.x, end.x) * width)),
  );
  const y = Math.max(
    0,
    Math.min(height - 1, Math.floor(Math.min(start.y, end.y) * height)),
  );
  return {
    x,
    y,
    width: Math.min(width - x, Math.ceil(Math.max(start.x, end.x) * width) - x),
    height: Math.min(
      height - y,
      Math.ceil(Math.max(start.y, end.y) * height) - y,
    ),
  };
}
export function AreaSelector({
  snapshot,
  enabled,
  onCancel,
  selectionCommand = "select_area",
  initialPreview,
  keepSelection = false,
  onSelected,
}: {
  snapshot: Snapshot;
  enabled: boolean;
  onCancel: () => void;
  selectionCommand?: string;
  initialPreview?: Preview;
  keepSelection?: boolean;
  onSelected?: () => void;
}) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [start, setStart] = useState<Point | null>(null),
    [end, setEnd] = useState<Point | null>(null);
  const [pending, setPending] = useState(false),
    [submitted, setSubmitted] = useState(false),
    [error, setError] = useState<string | null>(null);
  const dragging = useRef(false);
  const origin = useRef<Point | null>(null);

  useEffect(() => {
    dragging.current = false;
    origin.current = null;
    setPreview(null);
    setStart(null);
    setEnd(null);
    setSubmitted(false);
    setError(null);
    if (!enabled || snapshot.phase !== "starting") return;
    if (initialPreview && initialPreview.sessionId === snapshot.sessionId) {
      setPreview(initialPreview);
      return;
    }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const accept = (value: Preview | null) => {
      if (!disposed && value?.sessionId === snapshot.sessionId)
        setPreview(value);
    };
    void listen<Preview>("area-selection", (event) => accept(event.payload))
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
        void call<Preview | null>(
          selectionCommand === "select_print_area"
            ? "get_print_area"
            : "get_preview",
        )
          .then(accept)
          .catch(() => {});
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    enabled,
    snapshot.sessionId,
    snapshot.phase,
    selectionCommand,
    initialPreview,
  ]);
  const visible =
    !!preview &&
    snapshot.phase === "starting" &&
    (!submitted || keepSelection) &&
    enabled;
  useEffect(() => {
    const cancel = (event: KeyboardEvent) => {
      if (
        visible &&
        selectionCommand !== "select_print_area" &&
        event.key === "Escape" &&
        !pending
      )
        onCancel();
    };
    window.addEventListener("keydown", cancel);
    return () => window.removeEventListener("keydown", cancel);
  }, [onCancel, pending, visible, selectionCommand]);
  const point = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
      y: Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height)),
    };
  };
  const submitting = useRef(false);
  async function confirm(origin: Point, destination: Point) {
    if (!preview || submitting.current) return;
    const rect = selectedPixels(
      origin,
      destination,
      preview.sourceWidth,
      preview.sourceHeight,
    );
    if (rect.width < 2 || rect.height < 2) return;
    submitting.current = true;
    setPending(true);
    setError(null);
    try {
      // The native command hides the overlay before starting capture.
      setSubmitted(true);
      await call(selectionCommand, { sessionId: preview.sessionId, ...rect });
      onSelected?.();
    } catch (error) {
      setSubmitted(false);
      setError(message(error));
    } finally {
      submitting.current = false;
      setPending(false);
    }
  }
  if (!visible) return null;
  if (selectionCommand === "select_print_area" && preview) {
    return (
      <PrintOverlay
        preview={preview}
        onCancel={onCancel}
        onDone={() => {
          setSubmitted(true);
          onSelected?.();
        }}
      />
    );
  }
  return (
    <div
      className="area-overlay"
      role="dialog"
      aria-label="Selecionar área de gravação"
    >
      {preview ? (
        <div
          className="area-canvas"
          style={
            selectionCommand === "select_print_area" && preview?.image
              ? {
                  backgroundImage: `url(${preview.image})`,
                  backgroundSize: "100% 100%",
                }
              : undefined
          }
          onPointerDown={(event) => {
            if (pending || submitted || event.button !== 0) return;
            const value = point(event);
            origin.current = value;
            dragging.current = true;
            setStart(value);
            setEnd(value);
            // Focus acquisition can make WebKit reject the first capture call.
            // Keep the selection usable even without explicit pointer capture.
            try {
              event.currentTarget.setPointerCapture(event.pointerId);
            } catch {
              /* The monitor-sized canvas still receives the drag. */
            }
          }}
          onPointerMove={(event) => {
            if (dragging.current) setEnd(point(event));
          }}
          onPointerUp={(event) => {
            if (!dragging.current) return;
            dragging.current = false;
            const destination = point(event);
            setEnd(destination);
            const initial = origin.current;
            origin.current = null;
            if (initial) void confirm(initial, destination);
          }}
          onPointerCancel={() => {
            dragging.current = false;
            origin.current = null;
          }}
        >
          {start && end ? (
            <span
              className="area-rectangle"
              style={{
                left: `${Math.min(start.x, end.x) * 100}%`,
                top: `${Math.min(start.y, end.y) * 100}%`,
                width: `${Math.abs(start.x - end.x) * 100}%`,
                height: `${Math.abs(start.y - end.y) * 100}%`,
              }}
            />
          ) : null}
        </div>
      ) : null}
      {error ? (
        <p className="name-error" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

export function AreaSelectionWindow() {
  const [printArea, setPrintArea] = useState<Preview | null>(null);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<Preview>("print-area-selection", (event) => {
      if (!disposed) setPrintArea(event.payload);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else {
        stop = unlisten;
        void call<Preview | null>("get_print_area").then((value) => {
          if (!disposed && value) setPrintArea(value);
        });
      }
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);
  const [snapshot, setSnapshot] = useState<Snapshot>(idle);
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void subscribe((value) => {
      if (!disposed) setSnapshot(value);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else {
        stop = unlisten;
        void call<Snapshot>("get_snapshot").then((value) => {
          if (!disposed)
            setSnapshot((current) =>
              value.revision >= current.revision ? value : current,
            );
        });
      }
    });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);
  return (
    <AreaSelector
      key={printArea?.sessionId ?? snapshot.sessionId}
      snapshot={
        printArea
          ? { ...idle, phase: "starting", sessionId: printArea.sessionId }
          : snapshot
      }
      selectionCommand={printArea ? "select_print_area" : "select_area"}
      onSelected={() => {
        if (printArea) setPrintArea(null);
      }}
      enabled
      onCancel={() => {
        if (printArea) {
          setPrintArea(null);
          void call("close_print").catch(() => {});
        } else
          void call("control_recording", { action: "discard" }).catch(() => {});
      }}
    />
  );
}
