import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { AreaSelector } from "./AreaSelector";
import { loadImage, releaseImage } from "./screenshots/render";
import { PrintOverlay } from "./screenshots/PrintOverlay";
import { call, idle, message, type Preview } from "./bridge";
export function MonitorAreaWindow() {
  const currentEditor = useRef<Preview | null>(null);
  const generation = useRef(0);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [editor, setEditor] = useState<Preview | null>(null);
  const [selected, setSelected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const options = {
      target: {
        kind: "WebviewWindow" as const,
        label: getCurrentWebviewWindow().label,
      },
    };
    let disposed = false;
    const stops: (() => void)[] = [];
    const reset = (value: Preview | null) => {
      generation.current++;
      if (currentEditor.current) releaseImage(currentEditor.current.image);
      currentEditor.current = null;
      setEditor(null);
      setSelected(false);
      setError(null);
      setPreview(value);
    };
    const prepareEditor = async (value: Preview | null) => {
      if (!value?.image || disposed) return;
      const epoch = generation.current;
      setSelected(true);
      // Keep the live selector painted until the frozen image is decoded.
      const image = await loadImage(value.image);
      await image.decode();
      if (!disposed && generation.current === epoch) {
        currentEditor.current = value;
        setEditor(value);
      }
    };
    void listen<Preview>(
      "print-area-selection",
      (event) => {
        void prepareEditor(event.payload).catch((e) => {
          if (!disposed) setError(message(e));
        });
      },
      options,
    )
      .then(async (unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        stops.push(unlisten);
        for (const [name, callback] of [
          ["area-pick", (event: { payload: Preview }) => reset(event.payload)],
          ["area-reset", () => reset(null)],
        ] as const) {
          const stop = await listen<Preview>(name, callback, options);
          if (disposed) stop();
          else stops.push(stop);
        }
        const value = await call<Preview | null>("get_area_pick");
        if (!disposed && value) {
          setPreview(value);
          return;
        }
        // Recover a handoff that completed before this webview finished loading.
        await prepareEditor(await call<Preview | null>("get_print_area"));
      })
      .catch((e) => {
        if (!disposed) setError(message(e));
      });
    return () => {
      disposed = true;
      generation.current++;
      stops.forEach((stop) => stop());
      if (currentEditor.current) releaseImage(currentEditor.current.image);
    };
  }, []);
  const cancel = () => {
    void call(selected || editor ? "close_print" : "cancel_area_pick").catch(
      (e) => setError(message(e)),
    );
  };
  if (error)
    return (
      <div role="alert">
        {error}
        <button onClick={cancel}>Cancelar</button>
      </div>
    );
  if (editor)
    return (
      <PrintOverlay preview={editor} onCancel={cancel} onDone={() => {}} />
    );
  return preview ? (
    <>
      <AreaSelector
        snapshot={{ ...idle, phase: "starting", sessionId: preview.sessionId }}
        enabled
        onCancel={cancel}
        selectionCommand="finish_area_pick"
        initialPreview={preview}
        keepSelection
        onSelected={() => setSelected(true)}
      />
      {!selected && (
        <div className="monitor-area-hint">
          Arraste para selecionar uma área neste monitor.
        </div>
      )}
    </>
  ) : null;
}
