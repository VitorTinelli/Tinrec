import { useEffect, useRef, useState } from "react";
import { AppWindow, Monitor } from "lucide-react";
import { call, message, type CaptureMode, type Sources } from "../bridge";

export function PrintSourceWindow() {
  const mode = (new URLSearchParams(location.search).get("mode") ??
    "window") as CaptureMode;
  const [sources, setSources] = useState<Sources | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  const busy = useRef(false);
  async function capture(id = "auto") {
    if (busy.current) return;
    busy.current = true;
    setPending(true);
    setError("");
    try {
      await call("print_source_visibility", { visible: false });
      await call("capture_print", {
        mode,
        display: mode === "window" ? "auto" : id,
        window: mode === "window" ? id : "",
        ...(mode === "area" ? {} : { action: "edit" }),
      });
      await call("close_print");
      await call("close_print_source");
    } catch (e) {
      await call("close_print").catch(() => {});
      if (/cancel/i.test(message(e))) {
        await call("close_print_source").catch(() => {});
      } else {
        setError(message(e));
        await call("print_source_visibility", { visible: true }).catch(
          () => {},
        );
      }
    } finally {
      busy.current = false;
      setPending(false);
    }
  }
  useEffect(() => {
    let active = true;
    void call<Sources>("get_sources")
      .then(async (value) => {
        if (!active) return;
        setSources(value);
        if (
          mode === "area" ||
          value.wayland ||
          (mode === "screen" && value.displays.length === 1)
        ) {
          await capture(mode === "screen" ? value.displays[0]?.id : undefined);
        } else {
          await call("print_source_visibility", { visible: true });
        }
      })
      .catch(async (e) => {
        if (active) {
          setError(message(e));
          await call("print_source_visibility", { visible: true }).catch(
            () => {},
          );
        }
      });
    return () => {
      active = false;
    };
  }, []);
  const items = mode === "screen" ? sources?.displays : sources?.windows;
  return (
    <main className="print-source-window">
      <div className="panel-heading">
        <h2>
          {mode === "screen"
            ? "Escolha a tela"
            : mode === "area"
              ? "Print de área"
              : "Escolha a janela"}
        </h2>
        <button
          className="button secondary"
          disabled={pending}
          onClick={() => void call("close_print_source")}
        >
          Cancelar
        </button>
      </div>
      {error && (
        <p role="alert" className="name-error">
          {error}
        </p>
      )}
      {!sources || pending ? (
        <p role="status">Preparando captura…</p>
      ) : (
        <div className="print-source-list">
          {items?.map((item) => (
            <button key={item.id} onClick={() => void capture(item.id)}>
              {mode === "screen" ? (
                <Monitor size={24} />
              ) : (
                <AppWindow size={24} />
              )}
              <span>
                <strong>{item.name}</strong>
                {"width" in item && "height" in item && (
                  <small>
                    {String(item.width)} × {String(item.height)}
                  </small>
                )}
              </span>
            </button>
          ))}
          {!items?.length && (
            <p>Nenhuma {mode === "screen" ? "tela" : "janela"} disponível.</p>
          )}
        </div>
      )}
    </main>
  );
}
