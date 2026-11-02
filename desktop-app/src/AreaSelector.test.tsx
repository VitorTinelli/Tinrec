// @vitest-environment jsdom
import { expect, it } from "vitest";
import { selectedPixels } from "./AreaSelector";

it.each([
  {
    name: "escala para a fonte 4K",
    start: { x: 0.25, y: 0.25 },
    end: { x: 0.75, y: 0.75 },
    width: 3840,
    height: 2160,
    expected: { x: 960, y: 540, width: 1920, height: 1080 },
  },
  {
    name: "aceita arraste inverso",
    start: { x: 0.75, y: 0.75 },
    end: { x: 0.25, y: 0.25 },
    width: 1920,
    height: 1080,
    expected: { x: 480, y: 270, width: 960, height: 540 },
  },
  {
    name: "inclui as bordas da fonte",
    start: { x: 0, y: 0 },
    end: { x: 1, y: 1 },
    width: 1920,
    height: 1080,
    expected: { x: 0, y: 0, width: 1920, height: 1080 },
  },
  {
    name: "arredonda para incluir pixels parcialmente selecionados",
    start: { x: 0.105, y: 0.205 },
    end: { x: 0.309, y: 0.409 },
    width: 100,
    height: 100,
    expected: { x: 10, y: 20, width: 21, height: 21 },
  },
  {
    name: "não transforma um clique sem arraste em região válida",
    start: { x: 0.5, y: 0.5 },
    end: { x: 0.5, y: 0.5 },
    width: 1920,
    height: 1080,
    expected: { x: 960, y: 540, width: 0, height: 0 },
  },
])("seleção de área: $name", ({ start, end, width, height, expected }) => {
  expect(selectedPixels(start, end, width, height)).toEqual(expected);
});
