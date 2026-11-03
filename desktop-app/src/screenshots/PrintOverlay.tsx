import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
} from "react";
import {
  Pencil,
  ArrowUpRight,
  Square,
  Circle,
  Type,
  Highlighter,
  Blend,
  Undo2,
  Copy,
  Upload,
  X,
  Scan,
} from "lucide-react";
import { call, message, type Preview } from "../bridge";
import {
  region,
  type Operation,
  type Point,
  type PrintDocument,
  type Tool,
} from "./document";
import { exportPng, renderDocument } from "./render";
type Rect = { x: number; y: number; width: number; height: number };
const tools: [Tool, string, typeof Pencil][] = [
  ["pen", "Desenhar", Pencil],
  ["arrow", "Seta", ArrowUpRight],
  ["rectangle", "Retângulo", Square],
  ["ellipse", "Elipse", Circle],
  ["text", "Texto", Type],
  ["highlight", "Destacar", Highlighter],
  ["fill", "Ocultar", Square],
  ["blur", "Desfocar", Blend],
];
export function PrintOverlay({
  preview,
  onCancel,
  onDone,
}: {
  preview: Preview;
  onCancel: () => void;
  onDone: () => void;
}) {
  const [rect, setRect] = useState<Rect | null>(null),
    [ops, setOps] = useState<Operation[]>([]),
    [draft, setDraft] = useState<Operation | null>(null);
  const [tool, setTool] = useState<Tool>("pen"),
    [color, setColor] = useState("#ef4444"),
    [size, setSize] = useState(4),
    [text, setText] = useState("");
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const toolbar = useRef<HTMLDivElement>(null);
  const [toolbarPosition, setToolbarPosition] = useState({ left: 16, top: 16 });
  useLayoutEffect(() => {
    if (!rect || !toolbar.current || !surface.current) return;
    const place = () => {
      const b = surface.current!.getBoundingClientRect();
      const menu = toolbar.current!.getBoundingClientRect();
      const x = b.left + (rect.x / preview.sourceWidth) * b.width;
      const y = b.top + (rect.y / preview.sourceHeight) * b.height;
      const bottom = y + (rect.height / preview.sourceHeight) * b.height;
      const safeBottom = Math.max(16, window.innerHeight - menu.height - 80);
      const below = bottom + 12;
      const above = y - menu.height - 12;
      setToolbarPosition({
        left: Math.max(16, Math.min(x, window.innerWidth - menu.width - 16)),
        top: Math.max(
          16,
          Math.min(
            below <= safeBottom ? below : above >= 16 ? above : safeBottom,
            safeBottom,
          ),
        ),
      });
    };
    place();
    const observer =
      typeof ResizeObserver !== "undefined" ? new ResizeObserver(place) : null;
    observer?.observe(toolbar.current);
    window.addEventListener("resize", place);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", place);
    };
  }, [rect, tool, preview.sourceWidth, preview.sourceHeight]);
  const canvas = useRef<HTMLCanvasElement>(null),
    surface = useRef<HTMLDivElement>(null),
    origin = useRef<Point | null>(null),
    drawing = useRef<Operation | null>(null),
    selection = useRef<Rect | null>(null),
    busy = useRef(false);
  const doc = (operations = ops): PrintDocument => ({
    version: 1,
    source: preview.image,
    width: preview.sourceWidth,
    height: preview.sourceHeight,
    operations: rect
      ? [
          {
            id: "crop",
            tool: "crop",
            x: rect.x,
            y: rect.y,
            w: rect.width,
            h: rect.height,
            size: 1,
            color: "#000000",
          },
          ...operations,
        ]
      : [],
  });
  useEffect(() => {
    if (!rect) return;
    let active = true;
    void renderDocument(doc(draft ? [...ops, draft] : ops))
      .then((image) => {
        if (!active || !canvas.current) return;
        canvas.current.width = image.width;
        canvas.current.height = image.height;
        canvas.current.getContext("2d")?.drawImage(image, 0, 0);
      })
      .catch((e) => {
        if (active) setError(message(e));
      });
    return () => {
      active = false;
    };
  }, [rect, ops, draft, preview]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy.current) onCancel();
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key === "z" &&
        !(e.target instanceof HTMLInputElement)
      ) {
        e.preventDefault();
        setOps((v) => v.slice(0, -1));
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onCancel]);
  function point(event: PointerEvent) {
    const b = surface.current!.getBoundingClientRect();
    return {
      x: Math.max(
        0,
        Math.min(
          preview.sourceWidth,
          ((event.clientX - b.left) / b.width) * preview.sourceWidth,
        ),
      ),
      y: Math.max(
        0,
        Math.min(
          preview.sourceHeight,
          ((event.clientY - b.top) / b.height) * preview.sourceHeight,
        ),
      ),
    };
  }
  function begin(event: PointerEvent<HTMLDivElement>) {
    if (pending || event.button !== 0) return;
    const p = point(event);
    if (
      rect &&
      (p.x < rect.x ||
        p.y < rect.y ||
        p.x > rect.x + rect.width ||
        p.y > rect.y + rect.height)
    )
      return;
    origin.current = rect ? { x: p.x - rect.x, y: p.y - rect.y } : p;
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      /* WebKit focus acquisition */
    }
    if (rect) {
      const a = origin.current;
      drawing.current = {
        id: crypto.randomUUID(),
        tool,
        x: a.x,
        y: a.y,
        w: 0,
        h: 0,
        points: tool === "pen" || tool === "highlight" ? [a] : undefined,
        color,
        size: tool === "text" ? Math.max(18, size * 5) : size,
        text,
        filled: tool === "fill",
      };
      setDraft(drawing.current);
    }
  }
  function move(event: PointerEvent) {
    if (!origin.current) return;
    const p = point(event),
      a = origin.current;
    if (!rect) {
      const r = region(a, p, preview.sourceWidth, preview.sourceHeight);
      selection.current = { x: r.x, y: r.y, width: r.w, height: r.h };
      setDraft({ id: "selection", tool: "rectangle", ...r, size: 1, color });
      return;
    }
    if (!drawing.current) return;
    const b = {
      x: Math.max(0, Math.min(rect.width, p.x - rect.x)),
      y: Math.max(0, Math.min(rect.height, p.y - rect.y)),
    };
    const op = drawing.current;
    drawing.current = {
      ...op,
      ...(tool === "arrow"
        ? { w: b.x - a.x, h: b.y - a.y }
        : tool === "pen" || tool === "highlight"
          ? { points: [...(op.points ?? []), b] }
          : tool === "text"
            ? {}
            : region(a, b, rect.width, rect.height)),
    };
    setDraft(drawing.current);
  }
  function end(event: PointerEvent) {
    if (!origin.current) return;
    move(event);
    if (!rect) {
      const r = selection.current;
      if (r && r.width >= 2 && r.height >= 2) setRect(r);
    } else if (drawing.current && (tool !== "text" || text.trim())) {
      const op = drawing.current;
      setOps((v) => [...v, op]);
    }
    origin.current = null;
    drawing.current = null;
    setDraft(null);
  }
  async function finish(action: "copy" | "upload") {
    if (!rect || busy.current) return;
    busy.current = true;
    setPending(true);
    setError("");
    try {
      const image = await exportPng(doc());
      await call("export_print", {
        image,
        title: `Print - ${new Date().toLocaleString("pt-BR").replace(/[/:]/g, "-")}`,
        action,
      });
      await call("select_print_area", {
        sessionId: preview.sessionId,
        ...rect,
      });
      onDone();
    } catch (e) {
      setError(message(e));
    } finally {
      busy.current = false;
      setPending(false);
    }
  }
  const active = rect ?? (draft?.id === "selection" ? selection.current : null);
  const style = active
    ? {
        left: `${(active.x / preview.sourceWidth) * 100}%`,
        top: `${(active.y / preview.sourceHeight) * 100}%`,
        width: `${(active.width / preview.sourceWidth) * 100}%`,
        height: `${(active.height / preview.sourceHeight) * 100}%`,
      }
    : {};
  return (
    <div
      className="print-overlay"
      role="dialog"
      aria-label="Capturar e editar print"
    >
      <div
        className="print-overlay-surface"
        ref={surface}
        style={{ backgroundImage: `url(${preview.image})` }}
        onPointerDown={begin}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={() => {
          origin.current = null;
          drawing.current = null;
          setDraft(null);
        }}
      >
        {active && (
          <>
            <div
              className="print-overlay-shade"
              style={{
                left: 0,
                top: 0,
                width: "100%",
                height: `${(active.y / preview.sourceHeight) * 100}%`,
              }}
            />
            <div
              className="print-overlay-shade"
              style={{
                left: 0,
                top: `${((active.y + active.height) / preview.sourceHeight) * 100}%`,
                right: 0,
                bottom: 0,
              }}
            />
            <div
              className="print-overlay-shade"
              style={{
                left: 0,
                top: style.top,
                width: style.left,
                height: style.height,
              }}
            />
            <div
              className="print-overlay-shade"
              style={{
                left: `${((active.x + active.width) / preview.sourceWidth) * 100}%`,
                right: 0,
                top: style.top,
                height: style.height,
              }}
            />
          </>
        )}
        {active && (
          <div className="print-overlay-selection" style={style}>
            {rect && <canvas ref={canvas} />}
          </div>
        )}
      </div>
      {rect && (
        <div
          ref={toolbar}
          style={toolbarPosition}
          className="print-overlay-toolbar"
          role="toolbar"
          aria-label="Ferramentas do print"
          aria-busy={pending}
        >
          {tools.map(([value, label, Icon]) => (
            <button
              key={value}
              title={label}
              aria-label={label}
              aria-pressed={tool === value}
              disabled={pending}
              onClick={() => setTool(value)}
            >
              <Icon size={18} />
            </button>
          ))}
          <span className="print-toolbar-divider" aria-hidden="true" />
          <input
            type="color"
            aria-label="Cor"
            value={color}
            disabled={pending}
            onChange={(e) => setColor(e.target.value)}
          />
          <input
            type="range"
            aria-label="Espessura"
            min="2"
            max="12"
            value={size}
            disabled={pending}
            onChange={(e) => setSize(Number(e.target.value))}
          />
          {tool === "text" && (
            <input
              aria-label="Texto da anotação"
              placeholder="Digite e clique na imagem"
              value={text}
              disabled={pending}
              onChange={(e) => setText(e.target.value)}
            />
          )}
          <button
            title="Desfazer"
            aria-label="Desfazer"
            disabled={pending || !ops.length}
            onClick={() => setOps((v) => v.slice(0, -1))}
          >
            <Undo2 size={18} />
          </button>
          <button
            title="Selecionar outra área"
            aria-label="Selecionar outra área"
            disabled={pending}
            onClick={() => {
              setRect(null);
              setOps([]);
              selection.current = null;
            }}
          >
            <Scan size={18} />
          </button>
          <span className="print-toolbar-divider" aria-hidden="true" />
          <button
            className="print-copy"
            disabled={pending}
            onClick={() => void finish("copy")}
          >
            <Copy size={18} />
            Copiar
          </button>
          <button
            className="print-upload"
            disabled={pending}
            onClick={() => void finish("upload")}
          >
            <Upload size={18} />
            Enviar ao site
          </button>
          <button
            className="print-cancel"
            title="Cancelar"
            aria-label="Cancelar print"
            disabled={pending}
            onClick={onCancel}
          >
            <X size={18} />
          </button>
        </div>
      )}
      {error && (
        <p className="print-overlay-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
