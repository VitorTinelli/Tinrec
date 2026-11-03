import { type PrintDocument, type Operation } from "./document";
const images = new Map<string, Promise<HTMLImageElement>>();
export function loadImage(src: string) {
  let pending = images.get(src);
  if (!pending) {
    pending = new Promise<HTMLImageElement>((resolve, reject) => {
      const image = new Image();
      image.onload = () => resolve(image);
      image.onerror = () =>
        reject(new Error("Não foi possível abrir a imagem."));
      image.src = src;
    });
    if (images.size > 12) images.delete(images.keys().next().value!);
    images.set(src, pending);
  }
  return pending;
}
function canvas(width: number, height: number) {
  if (width < 1 || height < 1 || width * height > 40_000_000)
    throw new Error("Imagem acima do limite de 40 megapixels.");
  const result = document.createElement("canvas");
  result.width = Math.round(width);
  result.height = Math.round(height);
  return result;
}
export function pixelEffect(
  data: Uint8ClampedArray,
  width: number,
  tool: string,
) {
  for (let i = 0; i < data.length; i += 4) {
    if (tool === "grayscale") {
      const v = Math.round(
        data[i] * 0.2126 + data[i + 1] * 0.7152 + data[i + 2] * 0.0722,
      );
      data[i] = data[i + 1] = data[i + 2] = v;
    }
    if (tool === "invert") {
      data[i] = 255 - data[i];
      data[i + 1] = 255 - data[i + 1];
      data[i + 2] = 255 - data[i + 2];
    }
    if (tool === "interlace" && Math.floor(i / 4 / width) % 4 < 2) {
      data[i] *= 0.5;
      data[i + 1] *= 0.5;
      data[i + 2] *= 0.5;
    }
  }
}
/** Three separable box passes approximate Gaussian blur in linear time. */
export function gaussianBlur(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  strength: number,
) {
  const radius = Math.max(1, Math.min(30, Math.round(strength / 2))),
    buffer = new Uint8ClampedArray(data.length);
  for (let pass = 0; pass < 3; pass++) {
    for (let row = 0; row < height; row++)
      for (let channel = 0; channel < 4; channel++) {
        let sum = 0;
        for (let k = -radius; k <= radius; k++)
          sum +=
            data[
              (row * width + Math.max(0, Math.min(width - 1, k))) * 4 + channel
            ];
        for (let column = 0; column < width; column++) {
          buffer[(row * width + column) * 4 + channel] = sum / (radius * 2 + 1);
          sum +=
            data[
              (row * width + Math.min(width - 1, column + radius + 1)) * 4 +
                channel
            ] -
            data[(row * width + Math.max(0, column - radius)) * 4 + channel];
        }
      }
    for (let column = 0; column < width; column++)
      for (let channel = 0; channel < 4; channel++) {
        let sum = 0;
        for (let k = -radius; k <= radius; k++)
          sum +=
            buffer[
              (Math.max(0, Math.min(height - 1, k)) * width + column) * 4 +
                channel
            ];
        for (let row = 0; row < height; row++) {
          data[(row * width + column) * 4 + channel] = sum / (radius * 2 + 1);
          sum +=
            buffer[
              (Math.min(height - 1, row + radius + 1) * width + column) * 4 +
                channel
            ] -
            buffer[(Math.max(0, row - radius) * width + column) * 4 + channel];
        }
      }
  }
}
async function draw(ctx: CanvasRenderingContext2D, op: Operation) {
  ctx.save();
  ctx.globalAlpha = op.opacity ?? 1;
  ctx.strokeStyle = op.color;
  ctx.fillStyle = op.color;
  ctx.lineWidth = op.size;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const left = Math.min(op.x, op.x + op.w),
    top = Math.min(op.y, op.y + op.h),
    width = Math.abs(op.w),
    height = Math.abs(op.h);
  switch (op.tool) {
    case "rectangle":
      ctx.strokeRect(left, top, width, height);
      if (op.filled) ctx.fillRect(left, top, width, height);
      break;
    case "ellipse":
      ctx.beginPath();
      ctx.ellipse(
        left + width / 2,
        top + height / 2,
        width / 2,
        height / 2,
        0,
        0,
        Math.PI * 2,
      );
      ctx.stroke();
      if (op.filled) ctx.fill();
      break;
    case "fill":
      ctx.fillRect(left, top, width, height);
      break;
    case "line":
    case "arrow":
      ctx.beginPath();
      ctx.moveTo(op.x, op.y);
      ctx.lineTo(op.x + op.w, op.y + op.h);
      ctx.stroke();
      if (op.tool === "arrow") {
        const angle = Math.atan2(op.h, op.w),
          length = Math.max(12, op.size * 4);
        ctx.beginPath();
        ctx.moveTo(op.x + op.w, op.y + op.h);
        ctx.lineTo(
          op.x + op.w - length * Math.cos(angle - 0.5),
          op.y + op.h - length * Math.sin(angle - 0.5),
        );
        ctx.lineTo(
          op.x + op.w - length * Math.cos(angle + 0.5),
          op.y + op.h - length * Math.sin(angle + 0.5),
        );
        ctx.closePath();
        ctx.fill();
      }
      break;
    case "pen":
    case "highlight":
      ctx.globalAlpha = op.tool === "highlight" ? 0.3 : 1;
      ctx.lineWidth =
        op.tool === "highlight" ? Math.max(12, op.size * 4) : op.size;
      ctx.beginPath();
      op.points?.forEach((p, i) =>
        i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y),
      );
      ctx.stroke();
      break;
    case "callout": {
      const fontSize = Math.max(16, op.size * 3),
        label = op.text ?? "";
      ctx.lineWidth = Math.max(2, op.size);
      ctx.beginPath();
      ctx.moveTo(op.x, op.y);
      ctx.lineTo(op.x + op.w, op.y + op.h);
      ctx.stroke();
      const angle = Math.atan2(op.h, op.w),
        length = Math.max(12, op.size * 4);
      ctx.beginPath();
      ctx.moveTo(op.x + op.w, op.y + op.h);
      ctx.lineTo(
        op.x + op.w - length * Math.cos(angle - 0.5),
        op.y + op.h - length * Math.sin(angle - 0.5),
      );
      ctx.lineTo(
        op.x + op.w - length * Math.cos(angle + 0.5),
        op.y + op.h - length * Math.sin(angle + 0.5),
      );
      ctx.closePath();
      ctx.fill();
      ctx.font = `${fontSize}px ${op.font ?? "Arial"}`;
      const boxWidth =
          Math.max(
            50,
            ...label.split("\n").map((line) => ctx.measureText(line).width),
          ) + 16,
        boxHeight = fontSize * 1.25 * label.split("\n").length + 16;
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(op.x - 8, op.y - 8, boxWidth, boxHeight);
      ctx.strokeRect(op.x - 8, op.y - 8, boxWidth, boxHeight);
      ctx.fillStyle = op.color;
      ctx.textBaseline = "top";
      label
        .split("\n")
        .forEach((line, i) =>
          ctx.fillText(line, op.x, op.y + i * fontSize * 1.25),
        );
      break;
    }
    case "text":
      ctx.font = `${op.size}px ${op.font ?? "Arial"}`;
      ctx.textBaseline = "top";
      (op.text ?? "")
        .split("\n")
        .forEach((line, i) =>
          ctx.fillText(line, op.x, op.y + i * op.size * 1.25),
        );
      break;
    case "image":
      if (op.image)
        ctx.drawImage(await loadImage(op.image), op.x, op.y, op.w, op.h);
      break;
    case "blur":
    case "grayscale":
    case "invert":
    case "glow":
    case "interlace": {
      const x = Math.max(0, Math.floor(left)),
        y = Math.max(0, Math.floor(top)),
        w = Math.min(ctx.canvas.width - x, Math.ceil(width)),
        h = Math.min(ctx.canvas.height - y, Math.ceil(height));
      if (w < 1 || h < 1) break;
      if (op.tool === "blur") {
        // Blur actual pixels, including on WebKit versions without canvas.filter.
        const original = ctx.getImageData(x, y, w, h);
        gaussianBlur(original.data, w, h, op.size);
        ctx.putImageData(original, x, y);
      } else if (op.tool === "glow") {
        const data = ctx.getImageData(x, y, w, h),
          src = data.data.slice();
        for (let row = 1; row < h - 1; row++)
          for (let col = 1; col < w - 1; col++)
            for (let c = 0; c < 3; c++) {
              const i = (row * w + col) * 4 + c;
              data.data[i] = Math.min(
                255,
                Math.abs(
                  src[i] * 4 -
                    src[i - 4] -
                    src[i + 4] -
                    src[i - w * 4] -
                    src[i + w * 4],
                ) * 2,
              );
            }
        ctx.putImageData(data, x, y);
      } else {
        const data = ctx.getImageData(x, y, w, h);
        pixelEffect(data.data, w, op.tool);
        ctx.putImageData(data, x, y);
      }
      break;
    }
  }
  ctx.restore();
}
export async function renderDocument(doc: PrintDocument) {
  await document.fonts?.ready;
  let output = canvas(doc.width, doc.height);
  output
    .getContext("2d")!
    .drawImage(await loadImage(doc.source), 0, 0, doc.width, doc.height);
  for (const op of doc.operations) {
    if (op.tool === "crop" || op.tool === "resize") {
      const next = canvas(op.w, op.h),
        ctx = next.getContext("2d")!;
      if (op.tool === "crop")
        ctx.drawImage(output, op.x, op.y, op.w, op.h, 0, 0, op.w, op.h);
      else ctx.drawImage(output, 0, 0, op.w, op.h);
      output = next;
    } else
      await draw(output.getContext("2d", { willReadFrequently: true })!, op);
  }
  return output;
}
export async function exportPng(doc: PrintDocument) {
  const result = await renderDocument(doc);
  return result.toDataURL("image/png");
}
