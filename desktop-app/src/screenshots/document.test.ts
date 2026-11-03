import { describe, expect, it } from "vitest";
import {
  annotationBounds,
  dimensions,
  moveOperation,
  region,
  validateDocument,
  type PrintDocument,
  type Operation,
} from "./document";
import { pixelEffect, gaussianBlur } from "./render";
const operation = (
  tool: Operation["tool"],
  extra: Partial<Operation> = {},
): Operation => ({
  id: tool,
  tool,
  x: 0,
  y: 0,
  w: 20,
  h: 10,
  color: "#000000",
  size: 4,
  ...extra,
});
const doc = (operations: Operation[] = []): PrintDocument => ({
  version: 1,
  source: "data:image/png;base64,YQ==",
  width: 800,
  height: 600,
  operations,
});
describe("print document", () => {
  it("maps annotations through crop and non-uniform resize", () => {
    const d = doc([
      operation("rectangle", { x: 100, y: 50, w: 100, h: 60 }),
      operation("crop", { x: 80, y: 30, w: 400, h: 300 }),
      operation("resize", { w: 800, h: 300 }),
    ]);
    expect(dimensions(d)).toEqual({ width: 800, height: 300 });
    expect(annotationBounds(d, 0)).toMatchObject({
      x: 40,
      y: 20,
      w: 200,
      h: 60,
    });
  });
  it("moves freehand points with their annotation", () => {
    expect(
      moveOperation(
        operation("pen", {
          x: 10,
          y: 20,
          points: [
            { x: 10, y: 20 },
            { x: 15, y: 25 },
          ],
        }),
        5,
        -3,
      ).points,
    ).toEqual([
      { x: 15, y: 17 },
      { x: 20, y: 22 },
    ]);
  });
  it("normalizes reverse drags without extending past the image", () => {
    expect(region({ x: 900, y: 650 }, { x: 100, y: 50 }, 800, 600)).toEqual({
      x: 100,
      y: 50,
      w: 700,
      h: 550,
    });
  });
  it("rejects unbounded dimensions and external inserted images", () => {
    expect(() => validateDocument({ ...doc(), width: 100000 })).toThrow();
    expect(() =>
      validateDocument(
        doc([operation("image", { image: "https://example.com/private.png" })]),
      ),
    ).toThrow();
    expect(() =>
      validateDocument(doc([operation("resize", { w: NaN })])),
    ).toThrow();
    expect(validateDocument(doc([operation("fill")]))).toEqual(
      doc([operation("fill")]),
    );
  });
  it("transforms exported pixels rather than CSS", () => {
    const data = new Uint8ClampedArray([10, 20, 30, 255]);
    pixelEffect(data, 1, "invert");
    expect([...data]).toEqual([245, 235, 225, 255]);
    pixelEffect(data, 1, "grayscale");
    expect(data[0]).toBe(data[1]);
    expect(data[1]).toBe(data[2]);
    expect(data[3]).toBe(255);
  });
});

it("Gaussian approximation smooths contrast while preserving opacity", () => {
  const data = new Uint8ClampedArray([
    0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255,
  ]);
  gaussianBlur(data, 3, 1, 4);
  expect(data[4]).toBeLessThan(255);
  expect(data[0]).toBeGreaterThan(0);
  expect([data[3], data[7], data[11]]).toEqual([255, 255, 255]);
});
