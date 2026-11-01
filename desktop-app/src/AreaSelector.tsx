import { useEffect, useRef, useState, type PointerEvent } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, message, type Preview, type Snapshot } from "./bridge";
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
}: {
  snapshot: Snapshot;
  enabled: boolean;
  onCancel: () => void;
}) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [start, setStart] = useState<Point | null>(null),
    [end, setEnd] = useState<Point | null>(null);
  const [pending, setPending] = useState(false),
    [submitted, setSubmitted] = useState(false),
    [error, setError] = useState<string | null>(null);
  const dragging = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    setPreview(null);
    setStart(null);
    setEnd(null);
    setSubmitted(false);
    setError(null);
    if (!enabled || snapshot.phase !== "starting") return;
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
        void call<Preview | null>("get_preview")
          .then(accept)
          .catch(() => {});
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled, snapshot.sessionId, snapshot.phase]);
  const visible =
    !!preview && snapshot.phase === "starting" && !submitted && enabled;
  useEffect(() => {
    if (visible && !dialog.current?.open) dialog.current?.showModal();
    else if (!visible && dialog.current?.open) dialog.current.close();
  }, [visible]);
  const point = (event: PointerEvent<HTMLDivElement>): Point => {
    const bounds = event.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)),
      y: Math.max(0, Math.min(1, (event.clientY - bounds.top) / bounds.height)),
    };
  };
  const rect =
    preview && start && end
      ? selectedPixels(start, end, preview.sourceWidth, preview.sourceHeight)
      : null;
  async function confirm() {
    if (!rect || !preview) return;
    setPending(true);
    setError(null);
    try {
      // Close the selection dialog before the capture engine starts its clock.
      setSubmitted(true);
      await new Promise((resolve) => setTimeout(resolve, 80));
      await call("select_area", { sessionId: preview.sessionId, ...rect });
    } catch (error) {
      setSubmitted(false);
      setError(message(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <dialog
      ref={dialog}
      className="area-dialog"
      aria-labelledby="area-heading"
      onCancel={(event) => {
        event.preventDefault();
        onCancel();
      }}
    >
      <h2 id="area-heading">Selecione a área para gravar</h2>
      <p>Arraste sobre a imagem. A gravação começa depois de confirmar.</p>
      {preview ? (
        <div
          className="area-canvas"
          style={{
            width: `min(100%, calc((100vh - 230px) * ${preview.width / preview.height}))`,
            margin: "0 auto",
          }}
          onPointerDown={(event) => {
            if (pending) return;
            dragging.current = true;
            event.currentTarget.setPointerCapture(event.pointerId);
            const value = point(event);
            setStart(value);
            setEnd(value);
          }}
          onPointerMove={(event) => {
            if (dragging.current) setEnd(point(event));
          }}
          onPointerUp={(event) => {
            if (dragging.current) setEnd(point(event));
            dragging.current = false;
          }}
          onPointerCancel={() => {
            dragging.current = false;
          }}
        >
          <img
            src={preview.image}
            width={preview.width}
            height={preview.height}
            alt="Fonte selecionada para escolher a área de captura"
            draggable={false}
          />
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
      <div className="area-actions">
        <span>
          {rect
            ? `${rect.width} × ${rect.height} px`
            : "Nenhuma área selecionada"}
        </span>
        <button
          className="button secondary"
          disabled={pending}
          onClick={onCancel}
        >
          Cancelar
        </button>
        <button
          className="button primary"
          disabled={pending || !rect || rect.width < 2 || rect.height < 2}
          onClick={() => void confirm()}
        >
          {pending ? "Preparando…" : "Gravar área"}
        </button>
      </div>
    </dialog>
  );
}
