// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { selectedPixels } from "./AreaSelector";
describe("seleção de área", () => {
  it("converte o arraste em pixels da fonte original sem usar o tamanho reduzido da imagem", () => {
    expect(
      selectedPixels({ x: 0.25, y: 0.25 }, { x: 0.75, y: 0.75 }, 3840, 2160),
    ).toEqual({ x: 960, y: 540, width: 1920, height: 1080 });
  });
  it("permite arrastar em sentido inverso e mantém a região dentro da fonte", () => {
    expect(
      selectedPixels({ x: 0.75, y: 0.75 }, { x: 0.25, y: 0.25 }, 1920, 1080),
    ).toEqual({ x: 480, y: 270, width: 960, height: 540 });
    expect(selectedPixels({ x: 0, y: 0 }, { x: 1, y: 1 }, 1920, 1080)).toEqual({
      x: 0,
      y: 0,
      width: 1920,
      height: 1080,
    });
  });
});
