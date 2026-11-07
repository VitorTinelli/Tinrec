export type Tool =
  | "callout"
  | "select"
  | "pen"
  | "line"
  | "arrow"
  | "numbered-arrow"
  | "rectangle"
  | "ellipse"
  | "text"
  | "highlight"
  | "fill"
  | "blur"
  | "grayscale"
  | "invert"
  | "crop"
  | "resize"
  | "image";
export interface Point {
  x: number;
  y: number;
}
export interface Operation {
  id: string;
  tool: Tool;
  x: number;
  y: number;
  w: number;
  h: number;
  color: string;
  size: number;
  text?: string;
  number?: number;
  points?: Point[];
  image?: string;
  font?: string;
  filled?: boolean;
  opacity?: number;
}
export interface PrintDocument {
  version: 1;
  source: string;
  width: number;
  height: number;
  operations: Operation[];
}
export function dimensions(doc: PrintDocument) {
  let width = doc.width,
    height = doc.height;
  for (const op of doc.operations) {
    if (op.tool === "crop") {
      width = op.w;
      height = op.h;
    }
    if (op.tool === "resize") {
      width = op.w;
      height = op.h;
    }
  }
  return { width, height };
}
export function region(a: Point, b: Point, width: number, height: number) {
  const x = Math.max(0, Math.min(width, Math.floor(Math.min(a.x, b.x))));
  const y = Math.max(0, Math.min(height, Math.floor(Math.min(a.y, b.y))));
  return {
    x,
    y,
    w: Math.max(0, Math.min(width - x, Math.ceil(Math.max(a.x, b.x)) - x)),
    h: Math.max(0, Math.min(height - y, Math.ceil(Math.max(a.y, b.y)) - y)),
  };
}
export function moveOperation(
  op: Operation,
  dx: number,
  dy: number,
): Operation {
  return {
    ...op,
    x: op.x + dx,
    y: op.y + dy,
    points: op.points?.map((p) => ({ x: p.x + dx, y: p.y + dy })),
  };
}
export function validateDocument(value: unknown): PrintDocument {
  const d = value as PrintDocument;
  const allowed = new Set<Tool>([
    "callout",
    "pen",
    "line",
    "arrow",
    "numbered-arrow",
    "rectangle",
    "ellipse",
    "text",
    "highlight",
    "fill",
    "blur",
    "grayscale",
    "invert",
    "crop",
    "resize",
    "image",
  ]);
  if (
    !d ||
    d.version !== 1 ||
    !Number.isInteger(d.width) ||
    !Number.isInteger(d.height) ||
    d.width < 1 ||
    d.height < 1 ||
    d.width * d.height > 40_000_000 ||
    typeof d.source !== "string" ||
    !d.source.startsWith("data:image/png;base64,") ||
    !Array.isArray(d.operations) ||
    d.operations.length > 2000
  )
    throw new Error("Rascunho inválido ou muito grande.");
  for (const op of d.operations) {
    if (
      !allowed.has(op.tool) ||
      ![op.x, op.y, op.w, op.h, op.size].every(Number.isFinite) ||
      op.size < 1 ||
      op.size > 500 ||
      !/^#[0-9a-f]{6}$/i.test(op.color) ||
      (op.text?.length ?? 0) > 10000 ||
      (op.points?.length ?? 0) > 100000
    )
      throw new Error("Operação inválida no rascunho.");
    if (
      op.tool === "numbered-arrow" &&
      (!Number.isSafeInteger(op.number) || op.number! < 1)
    )
      throw new Error("Número inválido na seta.");
    if (
      op.opacity !== undefined &&
      (!Number.isFinite(op.opacity) || op.opacity < 0 || op.opacity > 1)
    )
      throw new Error("Opacidade inválida.");
    if (op.points?.some((p) => !Number.isFinite(p.x) || !Number.isFinite(p.y)))
      throw new Error("Traço inválido.");
    if (
      (op.tool === "crop" || op.tool === "resize") &&
      (!Number.isInteger(op.w) ||
        !Number.isInteger(op.h) ||
        op.w < 1 ||
        op.h < 1 ||
        op.w * op.h > 40_000_000)
    )
      throw new Error("Dimensões inválidas.");
    if (op.image && !/^data:image\/(png|jpeg|webp);base64,/.test(op.image))
      throw new Error("Imagem inserida inválida.");
    if (
      op.font &&
      !["Arial", "Verdana", "Georgia", "monospace"].includes(op.font)
    )
      throw new Error("Fonte inválida.");
  }
  return d;
}
/** Transform an annotation's bounding box into the final image coordinate space. */
export function annotationBounds(doc: PrintDocument, index: number) {
  const op = doc.operations[index];
  let x = Math.min(op.x, op.x + op.w),
    y = Math.min(op.y, op.y + op.h),
    w = Math.max(Math.abs(op.w), op.size * 2),
    h = Math.max(Math.abs(op.h), op.size * 2),
    scale = 1,
    scaleY = 1;
  if (op.tool === "numbered-arrow") {
    const radius = numberedArrowRadius(op);
    const right = Math.max(op.x + radius, op.x + op.w);
    const bottom = Math.max(op.y + radius, op.y + op.h);
    x = Math.min(op.x - radius, op.x + op.w);
    y = Math.min(op.y - radius, op.y + op.h);
    w = right - x;
    h = bottom - y;
  }
  if (op.tool === "callout") {
    const fontSize = Math.max(16, op.size * 3),
      labelWidth =
        Math.max(
          50,
          ...(op.text ?? "")
            .split("\n")
            .map((line) => line.length * fontSize * 0.7),
        ) + 16;
    const right = Math.max(x + w, op.x + labelWidth),
      bottom = Math.max(
        y + h,
        op.y + fontSize * 1.25 * (op.text ?? "").split("\n").length + 16,
      );
    x = Math.min(x, op.x - 8);
    y = Math.min(y, op.y - 8);
    w = right - x;
    h = bottom - y;
  }
  let dw = doc.width,
    dh = doc.height;
  for (let i = 0; i < doc.operations.length; i++) {
    const next = doc.operations[i];
    if (i > index && next.tool === "crop") {
      x -= next.x;
      y -= next.y;
    }
    if (i > index && next.tool === "resize") {
      const sx = next.w / dw,
        sy = next.h / dh;
      x *= sx;
      y *= sy;
      w *= sx;
      h *= sy;
      scale *= sx;
      scaleY *= sy;
    }
    if (next.tool === "crop" || next.tool === "resize") {
      dw = next.w;
      dh = next.h;
    }
  }
  return { x, y, w, h, scale, scaleY };
}

export function numberedArrowRadius(op: Operation) {
  return Math.max(14, op.size * 4);
}
export function nextArrowNumber(operations: Operation[]) {
  const used = new Set(
    operations
      .filter((op) => op.tool === "numbered-arrow")
      .map((op) => op.number),
  );
  let number = 1;
  while (used.has(number)) number++;
  return number;
}
