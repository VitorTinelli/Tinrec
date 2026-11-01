import { useEffect, useState } from "react";
import { Monitor, Pause } from "lucide-react";
import { call, subscribePreview, type Preview, type Snapshot } from "./bridge";
export function CapturePreview({ snapshot }: { snapshot: Snapshot }) {
  const [preview, setPreview] = useState<Preview | null>(null);
  useEffect(() => {
    setPreview(null);
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const accept = (value: Preview) => {
      if (!disposed && value.sessionId === snapshot.sessionId)
        setPreview((previous) =>
          !previous || previous.sequence <= value.sequence ? value : previous,
        );
    };
    subscribePreview(accept)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    void call<Preview | null>("get_preview")
      .then((value) => {
        if (value) accept(value);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [snapshot.sessionId]);
  useEffect(() => {
    if (snapshot.phase === "discarded") setPreview(null);
  }, [snapshot.phase]);
  const visible = preview && snapshot.phase !== "discarded";
  return (
    <section className="capture-preview" aria-label="Prévia da tela capturada">
      <div className="preview-topline">
        <span>
          <Monitor size={13} /> TELA CAPTURADA
        </span>
        <span>
          {visible
            ? `${preview.sourceWidth} × ${preview.sourceHeight}`
            : "Prévia ao iniciar"}
        </span>
      </div>
      <div
        className={`preview-viewport ${snapshot.phase === "paused" ? "preview-paused" : ""}`}
      >
        {visible ? (
          <img
            src={preview.image}
            width={preview.width}
            height={preview.height}
            alt="Prévia da tela realmente selecionada para gravação"
          />
        ) : (
          <div className="preview-placeholder">
            <Monitor size={30} />
            <strong>
              {snapshot.phase === "starting"
                ? "Escolha a tela no diálogo do sistema"
                : "Veja aqui o que está sendo capturado"}
            </strong>
            <span>A prévia começa junto com a gravação.</span>
          </div>
        )}
        {snapshot.phase === "paused" ? (
          <span className="preview-pause-label">
            <Pause size={13} /> Captura pausada
          </span>
        ) : null}
      </div>
      <p>A mesma fonte usada no vídeo, em prévia reduzida.</p>
    </section>
  );
}
