import { Monitor, AppWindow, Scan, ArrowUpRight, Upload } from "lucide-react";
import { useCallback, useEffect, useState, useRef } from "react";
import { call, message, size, type Sources } from "../bridge";
interface PrintCapture {
  image: string;
}
interface PrintFile {
  fileName: string;
  title: string;
  bytes: number;
  modifiedMs: number;
  thumbnail: string;
}
export function ScreenshotPanel({
  sources,
  display,
  windowId,
  disabled,
  onWindow,
  onDisplay,
  shortcut,
  shortcutMode = "area",
  libraryOnly = false,
  captureOnly = false,
  onBusyChange,
}: {
  sources: Sources | null;
  display: string;
  windowId: string;
  disabled: boolean;
  onWindow: (id: string) => void;
  onDisplay: (id: string) => void;
  shortcut: number;
  shortcutMode?: "screen" | "window" | "area";
  libraryOnly?: boolean;
  captureOnly?: boolean;
  onBusyChange?: (busy: boolean) => void;
}) {
  const [selectedMode, setSelectedMode] = useState("screen");
  const [pending, setPending] = useState(false),
    [notice, setNotice] = useState(""),
    [error, setError] = useState(""),
    [files, setFiles] = useState<PrintFile[]>([]),
    [query, setQuery] = useState("");
  useEffect(() => {
    onBusyChange?.(pending);
  }, [pending, onBusyChange]);
  const mounted = useRef(true),
    opened = useRef(false),
    seenShortcut = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (opened.current) {
        opened.current = false;
        void call("close_print").catch(() => {});
      }
    };
  }, []);
  const refreshRevision = useRef(0);
  const refresh = useCallback(async () => {
    const revision = ++refreshRevision.current;
    try {
      const value = await call<PrintFile[]>("list_prints");
      if (mounted.current && revision === refreshRevision.current)
        setFiles(value ?? []);
    } catch (e) {
      if (mounted.current && revision === refreshRevision.current)
        setError(message(e));
    }
  }, []);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  async function take(mode: "screen" | "window" | "area") {
    if (pending || disabled) return;
    setSelectedMode(mode);
    setPending(true);
    setError("");
    setNotice("");
    try {
      const result = await call<PrintCapture>("capture_print", {
        mode,
        display,
        window: windowId,
        ...(mode === "area" ? {} : { action: "copy" }),
      });
      if (mode === "area") return;
      opened.current = true;
      const title = `Print - ${new Date().toLocaleString("pt-BR").replace(/[/:]/g, "-")}`;
      if (result.image)
        await call("export_print", {
          image: result.image,
          title,
          action: "copy",
        });
      if (mounted.current) setNotice("Print copiado.");
    } catch (e) {
      const text = message(e);
      if (!/cancel/i.test(text)) setError(text);
    } finally {
      opened.current = false;
      await call("close_print").catch(() => {});
      if (mounted.current) setPending(false);
      void refresh();
    }
  }
  useEffect(() => {
    if (shortcut > seenShortcut.current && !disabled && !pending) {
      seenShortcut.current = shortcut;
      void take(shortcutMode);
    }
  }, [shortcut, shortcutMode, disabled]);
  async function upload(fileName: string) {
    setPending(true);
    setError("");
    try {
      await call("upload_print", { fileName });
      setNotice("Print na fila de envio.");
    } catch (e) {
      setError(message(e));
    } finally {
      setPending(false);
    }
  }
  return (
    <section
      className={`screenshot-panel ${captureOnly ? "panel configuration print-capture-panel" : ""}`}
    >
      {!libraryOnly && (
        <>
          <div className="panel-heading">
            <h2>Sua captura</h2>
          </div>
          <div className="capture-modes" aria-label="O que capturar no print">
            {(
              [
                ["screen", "Tela", Monitor],
                ["window", "Janela", AppWindow],
                ["area", "Área", Scan],
              ] as const
            ).map(([mode, label, Icon]) => (
              <button
                key={mode}
                type="button"
                className={selectedMode === mode ? "selected" : ""}
                aria-pressed={selectedMode === mode}
                aria-label={
                  mode === "screen"
                    ? "Print da tela"
                    : mode === "window"
                      ? "Print da janela"
                      : "Print de uma área"
                }
                disabled={
                  disabled ||
                  pending ||
                  (mode === "window" && !sources?.wayland && !windowId)
                }
                onClick={() => void take(mode as "screen" | "window" | "area")}
              >
                <Icon size={20} />
                <span>{label}</span>
              </button>
            ))}
          </div>
          {!sources?.wayland && (
            <div className="field-grid">
              <label>
                Tela
                <select
                  value={display}
                  onChange={(e) => onDisplay(e.target.value)}
                  disabled={pending || disabled}
                >
                  <option value="auto">Tela principal</option>
                  {sources?.displays.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Janela
                <select
                  value={windowId}
                  onChange={(e) => onWindow(e.target.value)}
                  disabled={pending || disabled}
                >
                  <option value="">Escolha uma janela</option>
                  {sources?.windows?.map((w) => (
                    <option value={w.id} key={w.id}>
                      {w.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}
          {pending && (
            <p role="status">Preparando print… Conclua a seleção no sistema.</p>
          )}
        </>
      )}
      {notice && <p role="status">{notice}</p>}
      {error && (
        <p className="name-error" role="alert">
          {error}
        </p>
      )}
      {!captureOnly && (
        <>
          <div className="print-library-header">
            <h2>Prints</h2>
            <input
              type="search"
              aria-label="Buscar prints"
              placeholder="Buscar prints…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <button className="button secondary" onClick={() => void refresh()}>
              Atualizar
            </button>
          </div>
          {!files.length ? (
            <p className="empty-state">Seus prints salvos aparecem aqui.</p>
          ) : (
            <div className="print-grid">
              {files
                .filter((f) =>
                  f.title
                    .toLocaleLowerCase("pt-BR")
                    .includes(query.toLocaleLowerCase("pt-BR")),
                )
                .map((file) => (
                  <article key={file.fileName}>
                    <button
                      className="print-preview"
                      onClick={() =>
                        void call("open_print", {
                          fileName: file.fileName,
                        }).catch((e) => setError(message(e)))
                      }
                    >
                      <img src={file.thumbnail} alt={file.title} />
                    </button>
                    <strong title={file.title}>{file.title}</strong>
                    <small>{size(file.bytes)}</small>
                    <button
                      className="text-button"
                      disabled={pending || disabled}
                      onClick={() => void upload(file.fileName)}
                    >
                      <Upload size={14} /> Enviar ao site
                    </button>
                    <button
                      className="text-button"
                      onClick={() =>
                        void call("print_site", {
                          fileName: file.fileName,
                        }).catch((e) => setError(message(e)))
                      }
                    >
                      <ArrowUpRight size={14} /> Abrir no site
                    </button>
                  </article>
                ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}
