import { usePrintThickness } from "./usePrintThickness";
import { ArrowNumberControl, useArrowNumber } from "./ArrowNumberControl";
import { printToolForKey, usePrintBindings } from "./toolShortcuts";
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent,
  type ComponentType,
} from "react";
import {
  Pencil,
  ArrowUpRight,
  GripVertical,
  Square,
  Circle,
  Type,
  Highlighter,
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
function BlurIcon({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <rect
        x="3"
        y="4"
        width="18"
        height="16"
        rx="2"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      {[7, 11, 15].map((y) => (
        <g key={y} stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <path d={`M7 ${y}h3`} />
          <path d={`M13 ${y}h1`} opacity=".45" />
          <path d={`M17 ${y}h.1`} opacity=".2" />
        </g>
      ))}
    </svg>
  );
}
function NumberedArrowIcon({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <path
        d="M4 20 15 9m-6 0h6v6"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="18" cy="6" r="5" fill="currentColor" />
      <path
        d="m17 4 1-1v6m-1 0h2"
        stroke="#1b2130"
        strokeWidth="1.4"
        strokeLinecap="round"
      />
    </svg>
  );
}
const tools: [Tool, string, ComponentType<{ size?: number }>][] = [
  ["pen", "Desenhar", Pencil],
  ["arrow", "Seta", ArrowUpRight],
  ["numbered-arrow", "Seta numerada", NumberedArrowIcon],
  ["rectangle", "Retângulo", Square],
  ["ellipse", "Elipse", Circle],
  ["text", "Texto", Type],
  ["highlight", "Destacar", Highlighter],
  ["fill", "Ocultar", Square],
  ["blur", "Desfocar", BlurIcon],
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
  const [rect, setRect] = useState<Rect | null>(
      preview.initialSelection ?? null,
    ),
    [ops, setOps] = useState<Operation[]>([]),
    [draft, setDraft] = useState<Operation | null>(null);
  const [tool, setTool] = useState<Tool | null>(null),
    [color, setColor] = useState("#ef4444"),
    [size, setSize] = usePrintThickness(2, 12);
  const [textEntry, setTextEntry] = useState<Operation | null>(null);
  const [arrowNumber, setArrowNumber] = useArrowNumber(ops);
  const bindings = usePrintBindings();
  const textEntryRef = useRef<Operation | null>(null);
  const textInput = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (textEntry) textInput.current?.focus();
  }, [textEntry?.id]);
  function commitText(id?: string) {
    const entry = textEntryRef.current;
    if (!entry || (id && entry.id !== id)) return;
    if (entry.text?.trim()) setOps((current) => [...current, entry]);
    textEntryRef.current = null;
    setTextEntry(null);
  }
  const [pending, setPending] = useState(false),
    [error, setError] = useState("");
  const toolbar = useRef<HTMLDivElement>(null);
  const toolbarMoved = useRef(false);
  const toolbarDrag = useRef<{ x: number; y: number } | null>(null);
  function boundedToolbar(left: number, top: number) {
    const menu = toolbar.current?.getBoundingClientRect();
    return {
      left: Math.max(
        8,
        Math.min(left, window.innerWidth - (menu?.width ?? 0) - 8),
      ),
      top: Math.max(
        8,
        Math.min(top, window.innerHeight - (menu?.height ?? 0) - 8),
      ),
    };
  }
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
      if (toolbarMoved.current) {
        setToolbarPosition((position) =>
          boundedToolbar(position.left, position.top),
        );
        return;
      }
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
    busy = useRef(false),
    readySession = useRef<string | null>(null);
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
    let painted = 0;
    const frame = requestAnimationFrame(() => {
      void renderDocument(doc(draft ? [...ops, draft] : ops))
        .then((image) => {
          if (!active || !canvas.current) return;
          canvas.current.width = image.width;
          canvas.current.height = image.height;
          canvas.current.getContext("2d")?.drawImage(image, 0, 0);
          if (
            preview.image.startsWith("http://tinrec-print.localhost/") &&
            readySession.current !== preview.sessionId
          ) {
            painted = requestAnimationFrame(() => {
              readySession.current = preview.sessionId;
              void call("print_editor_ready", {
                sessionId: preview.sessionId,
              }).catch(() => {});
            });
          }
        })
        .catch((e) => {
          if (active) setError(message(e));
        });
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(painted);
      active = false;
    };
  }, [rect, ops, draft, preview]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const shortcutTool = printToolForKey(e, bindings);
      if (shortcutTool === "arrow-number-up" || shortcutTool === "arrow-number-down") {
        if (rect && !busy.current && tool === "numbered-arrow" && !drawing.current) {
          e.preventDefault();
          setArrowNumber(arrowNumber + (shortcutTool === "arrow-number-up" ? 1 : -1));
        }
        return;
      }
      if (rect && !busy.current && shortcutTool && tools.some(([value]) => value === shortcutTool)) {
        e.preventDefault();
        origin.current = null;
        drawing.current = null;
        setDraft(null);
        setTool(shortcutTool);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        if (busy.current || e.repeat) return;
        if (textEntryRef.current) {
          textEntryRef.current = null;
          setTextEntry(null);
          return;
        }
        origin.current = null;
        drawing.current = null;
        setDraft(null);
        if (tool) setTool(null);
        else onCancel();
        return;
      }
      if (
        e.ctrlKey &&
        !e.altKey &&
        !e.shiftKey &&
        ["c", "s"].includes(e.key.toLowerCase())
      ) {
        e.preventDefault();
        if (!e.repeat && !busy.current && rect)
          void finish(e.key.toLowerCase() === "c" ? "copy" : "upload");
        return;
      }
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key === "z" &&
        !(e.target instanceof HTMLInputElement) &&
        !(e.target instanceof HTMLTextAreaElement)
      ) {
        e.preventDefault();
        setOps((v) => v.slice(0, -1));
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [onCancel, onDone, tool, rect, ops, color, size, bindings, arrowNumber]);
  useEffect(() => {
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      if (!busy.current && event.deltaY)
        setSize((value) =>
          Math.max(2, Math.min(12, value + (event.deltaY < 0 ? 1 : -1))),
        );
    };
    window.addEventListener("wheel", wheel, { passive: false });
    return () => window.removeEventListener("wheel", wheel);
  }, []);
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
    if (pending || event.button !== 0 || (rect && !tool)) return;
    const p = point(event);
    if (
      rect &&
      (p.x < rect.x ||
        p.y < rect.y ||
        p.x > rect.x + rect.width ||
        p.y > rect.y + rect.height)
    )
      return;
    if (rect && tool === "text") {
      // Keep the pointer's default focus change from blurring the new input.
      event.preventDefault();
      commitText();
      const entry: Operation = {
        id: crypto.randomUUID(),
        tool: "text",
        x: p.x - rect.x,
        y: p.y - rect.y,
        w: 0,
        h: 0,
        text: "",
        size: Math.max(18, size * 5),
        color,
      };
      textEntryRef.current = entry;
      setTextEntry(entry);
      return;
    }
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
        tool: tool!,
        number: tool === "numbered-arrow" ? arrowNumber : undefined,
        x: a.x,
        y: a.y,
        w: 0,
        h: 0,
        points: tool === "pen" || tool === "highlight" ? [a] : undefined,
        color,
        size: tool === "text" ? Math.max(18, size * 5) : size,
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
      ...(tool === "arrow" || tool === "numbered-arrow"
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
    } else if (drawing.current) {
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
      const title = `Print - ${new Date().toLocaleString("pt-BR").replace(/[/:]/g, "-")}`;
      const entry = textEntryRef.current;
      const finalOps = entry?.text?.trim() ? [...ops, entry] : ops;
      if (!finalOps.length) {
        await call("export_print_selection", {
          request: { sessionId: preview.sessionId, rect, title, action },
        });
      } else {
        const image = await exportPng(doc(finalOps));
        await call("export_print", { image, title, action });
      }
      await call("select_print_area", {
        sessionId: preview.sessionId,
        returnImage: false,
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
      {rect && textEntry && (
        <textarea
          key={textEntry.id}
          ref={textInput}
          className="print-inline-text"
          aria-label="Texto da anotação"
          placeholder="Digite aqui"
          value={textEntry.text ?? ""}
          rows={Math.min(8, (textEntry.text ?? "").split("\n").length)}
          style={{
            left: `${((rect.x + textEntry.x) / preview.sourceWidth) * 100}%`,
            top: `${((rect.y + textEntry.y) / preview.sourceHeight) * 100}%`,
            width: `${(Math.min(360, rect.width - textEntry.x) / preview.sourceWidth) * 100}%`,
            maxHeight: `${((rect.height - textEntry.y) / preview.sourceHeight) * 100}%`,
            fontSize: `${(textEntry.size / preview.sourceWidth) * 100}vw`,
            color: textEntry.color,
          }}
          onChange={(event) => {
            const entry = { ...textEntry, text: event.target.value };
            textEntryRef.current = entry;
            setTextEntry(entry);
          }}
          onBlur={() => commitText(textEntry.id)}
          onKeyDown={(event) => {
            if (
              event.key === "Enter" &&
              !event.shiftKey &&
              !event.nativeEvent.isComposing
            ) {
              event.preventDefault();
              commitText(textEntry.id);
            }
          }}
        />
      )}
      {rect && (
        <div
          ref={toolbar}
          style={toolbarPosition}
          className="print-overlay-toolbar"
          role="toolbar"
          aria-label="Ferramentas do print"
          aria-busy={pending}
        >
          <button
            className="print-toolbar-grip"
            title="Arraste para mover a barra (ou use as setas do teclado)"
            aria-label="Mover barra de ferramentas"
            disabled={pending}
            onPointerDown={(event) => {
              if (event.button !== 0) return;
              event.preventDefault();
              toolbarMoved.current = true;
              const bounds = toolbar.current!.getBoundingClientRect();
              toolbarDrag.current = {
                x: event.clientX - bounds.left,
                y: event.clientY - bounds.top,
              };
              try {
                event.currentTarget.setPointerCapture(event.pointerId);
              } catch {
                /* Synthetic input */
              }
            }}
            onPointerMove={(event) => {
              if (!toolbarDrag.current) return;
              setToolbarPosition(
                boundedToolbar(
                  event.clientX - toolbarDrag.current.x,
                  event.clientY - toolbarDrag.current.y,
                ),
              );
            }}
            onPointerUp={() => {
              toolbarDrag.current = null;
            }}
            onPointerCancel={() => {
              toolbarDrag.current = null;
            }}
            onLostPointerCapture={() => {
              toolbarDrag.current = null;
            }}
            onKeyDown={(event) => {
              const directions: Record<string, [number, number]> = {
                ArrowLeft: [-1, 0],
                ArrowRight: [1, 0],
                ArrowUp: [0, -1],
                ArrowDown: [0, 1],
              };
              const direction = directions[event.key];
              if (!direction) return;
              event.preventDefault();
              toolbarMoved.current = true;
              const step = event.shiftKey ? 24 : 8;
              setToolbarPosition((position) =>
                boundedToolbar(
                  position.left + direction[0] * step,
                  position.top + direction[1] * step,
                ),
              );
            }}
          >
            <GripVertical size={16} />
          </button>
          {tools.map(([value, label, Icon]) => (
            <button
              key={value}
              title={`${
                value === "blur"
                  ? "Desfocar: arraste sobre textos ou detalhes da imagem"
                  : value === "fill"
                    ? "Ocultar: cubra uma área com uma cor sólida"
                    : label
              }${bindings[value] ? ` (${bindings[value]})` : ""}`}
              aria-label={label}
              className={value === "fill" ? "print-fill-tool" : undefined}
              aria-pressed={tool === value}
              disabled={pending}
              onClick={() =>
                setTool((current) => (current === value ? null : value))
              }
            >
              <Icon size={17} />
              {value === "blur" && <span>Desfocar</span>}
            </button>
          ))}
          <span className="print-toolbar-divider" aria-hidden="true" />
          {tool === "numbered-arrow" && <ArrowNumberControl value={arrowNumber} onChange={setArrowNumber} disabled={pending} />}
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
            title="Copiar (Ctrl+C)"
            aria-label="Copiar"
            disabled={pending}
            onClick={() => void finish("copy")}
          >
            <Copy size={18} />
          </button>
          <button
            className="print-upload"
            title="Enviar ao site (Ctrl+S)"
            aria-label="Enviar ao site"
            disabled={pending}
            onClick={() => void finish("upload")}
          >
            <Upload size={18} />
            <span>Enviar</span>
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
