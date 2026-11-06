import { usePrintThickness } from "./usePrintThickness";
import { useEffect, useRef, useState, type PointerEvent } from "react";
import {
  MousePointer2,
  ArrowUpRight,
  ListOrdered,
  Minus,
  RectangleHorizontal,
  Circle,
  Pencil,
  Type,
  Highlighter,
  Square,
  Blend,
  Crop,
  Palette,
  Contrast,
  Sparkles,
  AlignJustify,
  MessageSquare,
} from "lucide-react";
import { call, message } from "../bridge";
import {
  annotationBounds,
  dimensions,
  moveOperation,
  region,
  nextArrowNumber,
  validateDocument,
  type Operation,
  type Point,
  type PrintDocument,
  type Tool,
} from "./document";
import { exportPng, loadImage, renderDocument } from "./render";
export interface PrintCapture {
  id: string;
  image: string;
  width: number;
  height: number;
  needsCrop: boolean;
  project?: PrintDocument;
}
const tools: [Tool, string][] = [
  ["select", "Selecionar"],
  ["arrow", "Seta"],
  ["numbered-arrow", "Seta numerada"],
  ["line", "Linha"],
  ["rectangle", "Retângulo"],
  ["ellipse", "Elipse"],
  ["pen", "Desenhar"],
  ["text", "Texto"],
  ["callout", "Chamada"],
  ["highlight", "Destacar"],
  ["fill", "Ocultar / preencher"],
  ["blur", "Desfocar"],
  ["crop", "Recortar"],
  ["grayscale", "Cinza"],
  ["invert", "Inverter cores"],
  ["glow", "Bordas luminosas"],
  ["interlace", "Entrelaçar"],
];
const toolIcons: Partial<Record<Tool, typeof Pencil>> = {
  select: MousePointer2,
  arrow: ArrowUpRight,
  "numbered-arrow": ListOrdered,
  line: Minus,
  rectangle: RectangleHorizontal,
  ellipse: Circle,
  pen: Pencil,
  text: Type,
  callout: MessageSquare,
  highlight: Highlighter,
  fill: Square,
  blur: Blend,
  crop: Crop,
  grayscale: Palette,
  invert: Contrast,
  glow: Sparkles,
  interlace: AlignJustify,
};
const annotations = new Set<Tool>([
  "callout",
  "numbered-arrow",
  "arrow",
  "line",
  "rectangle",
  "ellipse",
  "pen",
  "text",
  "highlight",
  "fill",
  "image",
]);
const id = () => crypto.randomUUID();
export function PrintEditor({
  capture,
  onClose,
  onSaved,
}: {
  capture: PrintCapture;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [picking, setPicking] = useState(false);
  const [doc, setDoc] = useState<PrintDocument>(
    capture.project ?? {
      version: 1,
      source: capture.image,
      width: capture.width,
      height: capture.height,
      operations: [],
    },
  );
  const [tool, setTool] = useState<Tool>(capture.needsCrop ? "crop" : "select"),
    [color, setColor] = useState("#eb4960"),
    [size, setSize] = usePrintThickness(1, 500),
    [font, setFont] = useState("Arial"),
    [filled, setFilled] = useState(false),
    [text, setText] = useState("Texto"),
    [zoom, setZoom] = useState(1),
    [selected, setSelected] = useState<string | null>(null),
    [draft, setDraft] = useState<Operation | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [title, setTitle] = useState(
      `Print - ${new Date().toLocaleString("pt-BR").replaceAll("/", "-").replaceAll(":", "h")}`,
    ),
    [dirty, setDirty] = useState(false),
    [closeConfirm, setCloseConfirm] = useState(false),
    [resize, setResize] = useState(false),
    [newWidth, setNewWidth] = useState(capture.width),
    [newHeight, setNewHeight] = useState(capture.height),
    [upload, setUpload] = useState<string | null>(null),
    [link, setLink] = useState<string | null>(null),
    [siteId, setSiteId] = useState<string | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null),
    viewport = useRef<HTMLDivElement>(null),
    drag = useRef<{
      start: Point;
      op?: Operation;
      drawing?: Operation;
      scale?: number;
      scaleY?: number;
    } | null>(null),
    exportBusy = useRef(false),
    undo = useRef<Operation[][]>([]),
    redo = useRef<Operation[][]>([]),
    latest = useRef(doc),
    inputImage = useRef<HTMLInputElement>(null),
    inputProject = useRef<HTMLInputElement>(null),
    [historyVersion, setHistoryVersion] = useState(0);
  latest.current = doc;
  function commit(operations: Operation[]) {
    undo.current.push(doc.operations);
    if (undo.current.length > 100) undo.current.shift();
    redo.current = [];
    setDoc({ ...doc, operations });
    setDirty(true);
    setNotice("");
    setHistoryVersion((v) => v + 1);
  }
  function history(direction: "undo" | "redo") {
    const from = direction === "undo" ? undo.current : redo.current,
      to = direction === "undo" ? redo.current : undo.current;
    const ops = from.pop();
    if (!ops) return;
    to.push(doc.operations);
    setDoc({ ...doc, operations: ops });
    setSelected(null);
    setDirty(true);
    setHistoryVersion((v) => v + 1);
  }
  useEffect(() => {
    let cancelled = false;
    const current = draft
      ? { ...doc, operations: [...doc.operations, draft] }
      : doc;
    void renderDocument(current)
      .then((output) => {
        if (cancelled || !canvas.current) return;
        const target = canvas.current;
        target.width = output.width;
        target.height = output.height;
        const ctx = target.getContext("2d")!;
        ctx.drawImage(output, 0, 0);
        if (selected) {
          const index = doc.operations.findIndex((op) => op.id === selected);
          if (index >= 0) {
            const b = annotationBounds(doc, index);
            ctx.save();
            ctx.strokeStyle = "#456ce8";
            ctx.lineWidth = 2;
            ctx.setLineDash([6, 4]);
            ctx.strokeRect(b.x - 4, b.y - 4, b.w + 8, b.h + 8);
            ctx.restore();
          }
        }
      })
      .catch((e) => !cancelled && setError(message(e)));
    return () => {
      cancelled = true;
    };
  }, [doc, draft, selected]);
  useEffect(() => {
    const fit = () => {
      const width = viewport.current?.clientWidth ?? 800;
      setZoom(Math.min(1, (width - 48) / capture.width));
    };
    fit();
  }, [capture.width]);
  useEffect(() => {
    if (!upload) return;
    let cancelled = false,
      checking = false;
    const check = async () => {
      if (checking) return;
      checking = true;
      try {
        const view = await call<{
          jobs: {
            id: string;
            url: string | null;
            status: string;
            error: string | null;
          }[];
        }>("cloud_state");
        const job = view.jobs.find((j) => j.id === upload);
        if (!cancelled && job) {
          if (job.status === "uploaded") {
            setNotice("Print enviado.");
            setLink(job.url);
            setUpload(null);
          } else if (job.error) setError(job.error);
          else setNotice("Enviando print… Pode continuar usando o editor.");
        }
      } catch (e) {
        if (!cancelled) setError(message(e));
      } finally {
        checking = false;
      }
    };
    void check();
    const timer = setInterval(() => void check(), 2000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [upload]);
  const currentSize = dimensions(doc);
  const selection = doc.operations.find((op) => op.id === selected);
  function point(event: PointerEvent<HTMLCanvasElement>): Point {
    const b = event.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(
        0,
        Math.min(
          currentSize.width,
          ((event.clientX - b.left) * currentSize.width) / b.width,
        ),
      ),
      y: Math.max(
        0,
        Math.min(
          currentSize.height,
          ((event.clientY - b.top) * currentSize.height) / b.height,
        ),
      ),
    };
  }
  function down(event: PointerEvent<HTMLCanvasElement>) {
    if (pending || event.button !== 0) return;
    const p = point(event);
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      /* Window-sized pointer fallback. */
    }
    setError("");
    if (tool === "select") {
      const reversed = doc.operations
        .map((op, i) => ({ op, i }))
        .reverse()
        .find(({ op, i }) => {
          if (!annotations.has(op.tool)) return false;
          const b = annotationBounds(doc, i);
          return (
            p.x >= b.x - 8 &&
            p.x <= b.x + b.w + 8 &&
            p.y >= b.y - 8 &&
            p.y <= b.y + b.h + 8
          );
        });
      if (!reversed) {
        setSelected(null);
        return;
      }
      const index = reversed.i;
      const op = doc.operations[index];
      setSelected(op.id);
      setColor(op.color);
      setSize(op.size);
      if (op.text) setText(op.text);
      drag.current = {
        start: p,
        op,
        scale: annotationBounds(doc, index).scale,
        scaleY: annotationBounds(doc, index).scaleY,
      };
      return;
    }
    setSelected(null);
    const op: Operation = {
      id: id(),
      tool,
      number:
        tool === "numbered-arrow" ? nextArrowNumber(doc.operations) : undefined,
      x: p.x,
      y: p.y,
      w: 0,
      h: 0,
      color,
      size: tool === "text" ? Math.max(20, size) : size,
      font,
      filled,
      text,
      points: [p],
    };
    if (tool === "text") {
      op.w = Math.max(
        30,
        ...text.split("\n").map((line) => line.length * op.size * 0.65),
      );
      op.h = text.split("\n").length * op.size * 1.25;
      commit([...doc.operations, op]);
      return;
    }
    drag.current = { start: p, drawing: op };
    setDraft(op);
  }
  function moving(event: PointerEvent<HTMLCanvasElement>) {
    const active = drag.current;
    if (!active || pending) return;
    const p = point(event);
    if (active.op) {
      const moved = moveOperation(
        active.op,
        (p.x - active.start.x) / (active.scale ?? 1),
        (p.y - active.start.y) / (active.scaleY ?? 1),
      );
      setDoc({
        ...doc,
        operations: doc.operations.map((op) =>
          op.id === moved.id ? moved : op,
        ),
      });
      return;
    }
    if (active.drawing) {
      active.drawing = {
        ...active.drawing,
        w: p.x - active.start.x,
        h: p.y - active.start.y,
        points: active.drawing.points
          ? [...active.drawing.points, p]
          : undefined,
      };
      setDraft(active.drawing);
    }
  }
  function up(event: PointerEvent<HTMLCanvasElement>) {
    const active = drag.current;
    if (!active) return;
    drag.current = null;
    const p = point(event);
    if (active.op) {
      undo.current.push(
        doc.operations.map((op) => (op.id === active.op!.id ? active.op! : op)),
      );
      redo.current = [];
      setDirty(true);
      setHistoryVersion((v) => v + 1);
      return;
    }
    if (!active.drawing) return;
    const drawing = active.drawing;
    let op = {
      ...drawing,
      w: p.x - active.start.x,
      h: p.y - active.start.y,
      points: drawing.points ? [...drawing.points, p] : undefined,
    };
    if (!["line", "arrow", "numbered-arrow", "pen", "highlight"].includes(tool))
      op = {
        ...op,
        ...region(active.start, p, currentSize.width, currentSize.height),
      };
    setDraft(null);
    if (Math.abs(op.w) < 1 && Math.abs(op.h) < 1) return;
    if ((op.tool === "crop" || op.tool === "blur") && (op.w < 1 || op.h < 1))
      return;
    commit([...doc.operations, op]);
    if (tool === "crop") setTool("select");
  }
  function updateSelected(values: Partial<Operation>) {
    if (!selection) return;
    commit(
      doc.operations.map((op) =>
        op.id === selected ? { ...op, ...values } : op,
      ),
    );
  }
  async function output(action: "copy" | "save" | "upload") {
    if (pending || exportBusy.current) return;
    exportBusy.current = true;
    setPending(true);
    setError("");
    try {
      const image = await exportPng(doc);
      const result = await call<{
        fileName: string | null;
        uploadId: string | null;
      }>("export_print", { image, title, action });
      setDirty(false);
      setNotice(
        action === "copy"
          ? "Imagem copiada."
          : action === "save"
            ? "Print salvo."
            : "Print na fila de envio.",
      );
      if (result.uploadId) {
        setUpload(result.uploadId);
        setSiteId(result.uploadId);
      }
      onSaved();
    } catch (e) {
      setError(message(e));
    } finally {
      exportBusy.current = false;
      setPending(false);
    }
  }
  async function readFile(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }
  async function saveProject() {
    try {
      await call("save_print_project", {
        document: JSON.stringify(doc),
        title,
      });
      setDirty(false);
      setNotice("Rascunho editável salvo no computador.");
      return true;
    } catch (e) {
      setError(message(e));
      return false;
    }
  }
  async function openProject() {
    try {
      const data = await call<string | null>("open_print_project");
      if (data) await importProject(new File([data], "rascunho.json"));
    } catch (e) {
      setError(message(e));
    }
  }
  async function importProject(file: File) {
    try {
      if (file.size > 110 * 1024 * 1024)
        throw new Error("Rascunho muito grande.");
      const next = validateDocument(JSON.parse(await file.text()));
      await loadImage(next.source);
      setDoc(next);
      undo.current = [];
      redo.current = [];
      setDirty(false);
      setSelected(null);
    } catch (e) {
      setError(message(e));
    }
  }
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        ["c", "s"].includes(event.key.toLowerCase())
      ) {
        event.preventDefault();
        if (!pending && !event.repeat)
          void output(event.key.toLowerCase() === "c" ? "copy" : "upload");
        return;
      }
      if (pending) return;
      if (
        event.target instanceof Element &&
        event.target.closest("input,textarea,select") &&
        event.key !== "Escape"
      )
        return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
        event.preventDefault();
        history(event.shiftKey ? "redo" : "undo");
      } else if (
        (event.key === "Delete" || event.key === "Backspace") &&
        selected
      ) {
        event.preventDefault();
        commit(doc.operations.filter((op) => op.id !== selected));
        setSelected(null);
      } else if (event.key === "Escape") {
        event.preventDefault();
        if (event.repeat) return;
        if (tool === "select" && !selected && !draft && !picking) {
          onClose();
          return;
        }
        setTool("select");
        setPicking(false);
        setSelected(null);
        setDraft(null);
        drag.current = null;
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [doc, selected, tool, draft, picking, pending, title, onClose, onSaved]);
  useEffect(() => {
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      if (!pending && event.deltaY) {
        const next = Math.max(
          1,
          Math.min(500, size + (event.deltaY < 0 ? 1 : -1)),
        );
        setSize(next);
        updateSelected({ size: next });
      }
    };
    window.addEventListener("wheel", wheel, { passive: false });
    return () => window.removeEventListener("wheel", wheel);
  }, [pending, size, doc, selected]);
  return (
    <section
      className="print-editor"
      role="dialog"
      aria-label="Editar print"
      aria-modal="true"
    >
      <header>
        <input
          aria-label="Nome do print"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          maxLength={150}
        />
        <div>
          <button disabled={pending} onClick={() => void output("copy")}>
            Copiar imagem
          </button>
          <button disabled={pending} onClick={() => void output("save")}>
            Salvar
          </button>
          <button
            className="primary"
            disabled={pending}
            onClick={() => void output("upload")}
          >
            Enviar ao site
          </button>
          <button
            disabled={pending}
            aria-label="Fechar editor"
            onClick={() => (dirty ? setCloseConfirm(true) : onClose())}
          >
            ×
          </button>
        </div>
      </header>
      <div className="editor-toolbar" aria-label="Ferramentas de edição">
        {tools.map(([value, label]) => {
          const Icon = toolIcons[value] ?? Pencil;
          return (
            <button
              className="editor-tool-icon"
              key={value}
              aria-label={label}
              title={label}
              aria-pressed={tool === value}
              disabled={pending}
              onClick={() => {
                setTool((current) => (current === value ? "select" : value));
                setSelected(null);
                setPicking(false);
              }}
            >
              <Icon size={17} />
            </button>
          );
        })}
        <button disabled={pending} onClick={() => inputImage.current?.click()}>
          Inserir imagem
        </button>
        <button
          disabled={pending}
          onClick={() => {
            const op: Operation = {
              id: id(),
              tool: "text",
              x: 20,
              y: 20,
              w: currentSize.width - 40,
              h: 40,
              color,
              size: Math.max(20, size),
              font,
              text,
            };
            commit([...doc.operations, op]);
          }}
        >
          Cabeçalho
        </button>
        <button
          disabled={pending}
          onClick={() => {
            const op: Operation = {
              id: id(),
              tool: "text",
              x: 20,
              y: Math.max(0, currentSize.height - 50),
              w: currentSize.width - 40,
              h: 40,
              color,
              size: Math.max(20, size),
              font,
              text,
            };
            commit([...doc.operations, op]);
          }}
        >
          Rodapé
        </button>
        <button
          disabled={pending}
          onClick={() => {
            const op: Operation = {
              id: id(),
              tool: "text",
              x: currentSize.width / 3,
              y: currentSize.height / 2,
              w: currentSize.width / 2,
              h: 60,
              color,
              size: Math.max(36, size),
              font,
              text,
              opacity: 0.35,
            };
            commit([...doc.operations, op]);
          }}
        >
          Marca-d’água
        </button>
        <button
          disabled={pending}
          onClick={() => {
            setNewWidth(currentSize.width);
            setNewHeight(currentSize.height);
            setResize(true);
          }}
        >
          Redimensionar
        </button>
        <button
          disabled={!undo.current.length || pending}
          onClick={() => history("undo")}
        >
          Desfazer
        </button>
        <button
          disabled={!redo.current.length || pending}
          onClick={() => history("redo")}
        >
          Refazer
        </button>
      </div>
      <div className="editor-properties">
        <label>
          Cor{" "}
          <input
            type="color"
            value={color}
            onChange={(e) => {
              setColor(e.target.value);
              updateSelected({ color: e.target.value });
            }}
          />
        </label>
        <button
          onClick={() => {
            setPicking(!picking);
            setSelected(null);
          }}
          aria-pressed={picking}
          title="Clique para escolher uma cor na imagem. Atalho: Alt + clique."
        >
          Conta-gotas
        </button>
        <label>
          Tamanho{" "}
          <input
            type="number"
            min="1"
            max="500"
            value={size}
            onChange={(e) => {
              const v = Math.max(1, Math.min(500, Number(e.target.value) || 1));
              setSize(v);
              updateSelected({ size: v });
            }}
          />
        </label>
        <label>
          <input
            type="checkbox"
            checked={filled}
            onChange={(e) => {
              setFilled(e.target.checked);
              updateSelected({ filled: e.target.checked });
            }}
          />
          Preencher forma
        </label>
        <label>
          Fonte{" "}
          <select
            value={font}
            onChange={(e) => {
              setFont(e.target.value);
              updateSelected({ font: e.target.value });
            }}
          >
            {["Arial", "Verdana", "Georgia", "monospace"].map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
        </label>
        <label>
          Texto{" "}
          <textarea
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              updateSelected({ text: e.target.value });
            }}
          />
        </label>
        {selection && (
          <>
            <label>
              Largura{" "}
              <input
                type="number"
                value={Math.round(selection.w)}
                onChange={(e) =>
                  updateSelected({ w: Number(e.target.value) || 1 })
                }
              />
            </label>
            <label>
              Altura{" "}
              <input
                type="number"
                value={Math.round(selection.h)}
                onChange={(e) =>
                  updateSelected({ h: Number(e.target.value) || 1 })
                }
              />
            </label>
            <label>
              Opacidade{" "}
              <input
                type="range"
                min="0.1"
                max="1"
                step="0.05"
                value={selection.opacity ?? 1}
                onChange={(e) =>
                  updateSelected({ opacity: Number(e.target.value) })
                }
              />
            </label>
            <button
              onClick={() => {
                commit(doc.operations.filter((op) => op.id !== selected));
                setSelected(null);
              }}
            >
              Excluir objeto
            </button>
          </>
        )}
        <label>
          Zoom{" "}
          <input
            type="range"
            min="0.1"
            max="4"
            step="0.1"
            value={zoom}
            onChange={(e) => setZoom(Number(e.target.value))}
          />
        </label>
        <span>
          {currentSize.width} × {currentSize.height}
        </span>
      </div>
      <div className="editor-viewport" ref={viewport}>
        <canvas
          ref={canvas}
          style={{
            width: currentSize.width * zoom,
            height: currentSize.height * zoom,
          }}
          onPointerDown={(e) => {
            if ((picking || e.altKey) && canvas.current) {
              setPicking(false);
              const p = point(e);
              const data = canvas.current
                .getContext("2d")!
                .getImageData(
                  Math.min(currentSize.width - 1, Math.floor(p.x)),
                  Math.min(currentSize.height - 1, Math.floor(p.y)),
                  1,
                  1,
                ).data;
              setColor(
                `#${[...data]
                  .slice(0, 3)
                  .map((c) => c.toString(16).padStart(2, "0"))
                  .join("")}`,
              );
              return;
            }
            down(e);
          }}
          onPointerMove={moving}
          onPointerUp={up}
          onPointerCancel={() => {
            drag.current = null;
            setDraft(null);
          }}
        />
      </div>
      <footer>
        <span role={error ? "alert" : "status"}>
          {error ||
            notice ||
            (pending
              ? "Preparando imagem…"
              : "Edite antes de copiar ou enviar.")}
        </span>
        {link && (
          <button onClick={() => void call("cloud_open", { id: siteId })}>
            Ver no site
          </button>
        )}
        <button onClick={() => void saveProject()}>Salvar rascunho</button>
        <button onClick={() => void openProject()}>Abrir rascunho</button>
      </footer>
      <input
        hidden
        type="file"
        accept="image/png,image/jpeg,image/webp"
        ref={inputImage}
        onChange={async (e) => {
          const file = e.target.files?.[0];
          if (!file) return;
          try {
            if (file.size > 20 * 1024 * 1024)
              throw new Error("Imagem inserida muito grande.");
            const src = await readFile(file),
              image = await loadImage(src);
            const w = Math.min(currentSize.width / 2, image.width),
              h = (image.height * w) / image.width;
            commit([
              ...doc.operations,
              {
                id: id(),
                tool: "image",
                x: 20,
                y: 20,
                w,
                h,
                color,
                size,
                image: src,
              },
            ]);
          } catch (error) {
            setError(message(error));
          }
          e.target.value = "";
        }}
      />
      <input
        hidden
        type="file"
        accept=".tinrecprint.json,application/json"
        ref={inputProject}
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) void importProject(file);
          e.target.value = "";
        }}
      />
      {resize && (
        <div
          className="editor-prompt"
          role="dialog"
          aria-label="Redimensionar print"
        >
          <label>
            Largura{" "}
            <input
              type="number"
              value={newWidth}
              onChange={(e) => setNewWidth(Number(e.target.value))}
            />
          </label>
          <label>
            Altura{" "}
            <input
              type="number"
              value={newHeight}
              onChange={(e) => setNewHeight(Number(e.target.value))}
            />
          </label>
          <button onClick={() => setResize(false)}>Cancelar</button>
          <button
            disabled={
              !Number.isInteger(newWidth) ||
              !Number.isInteger(newHeight) ||
              newWidth < 1 ||
              newHeight < 1 ||
              newWidth * newHeight > 40_000_000
            }
            onClick={() => {
              commit([
                ...doc.operations,
                {
                  id: id(),
                  tool: "resize",
                  x: 0,
                  y: 0,
                  w: newWidth,
                  h: newHeight,
                  color,
                  size,
                },
              ]);
              setResize(false);
            }}
          >
            Aplicar
          </button>
        </div>
      )}
      {closeConfirm && (
        <div
          className="editor-prompt"
          role="alertdialog"
          aria-label="Alterações não exportadas"
        >
          <p>Há alterações não exportadas.</p>
          <button onClick={() => setCloseConfirm(false)}>
            Continuar editando
          </button>
          <button
            onClick={() =>
              void saveProject().then((saved) => {
                if (saved) onClose();
              })
            }
          >
            Salvar rascunho e fechar
          </button>
          <button onClick={onClose}>Descartar</button>
        </div>
      )}
    </section>
  );
}
