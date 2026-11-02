// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { call, idle } from "./bridge";
import { AreaSelector } from "./AreaSelector";
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
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

it.each([false, true])(
  "inicia no primeiro arraste mesmo com eventos agrupados e captura rejeitada=%s",
  async (captureRejected) => {
    vi.mocked(call).mockClear();
    vi.stubGlobal("PointerEvent", MouseEvent);
    vi.mocked(call).mockImplementation(async (command) =>
      command === "get_preview"
        ? {
            sessionId: "area-1",
            sequence: 1,
            image: "data:image/jpeg;base64,YQ==",
            width: 400,
            height: 200,
            sourceWidth: 1920,
            sourceHeight: 1080,
          }
        : undefined,
    );
    const { container } = render(
      <AreaSelector
        snapshot={{ ...idle, phase: "starting", sessionId: "area-1" }}
        enabled
        onCancel={() => {}}
      />,
    );
    await screen.findByRole("dialog", { name: "Selecionar área de gravação" });
    expect(screen.queryByRole("button")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("dialog")).toBeNull();
    const canvas = container.querySelector(".area-canvas") as HTMLDivElement;
    canvas.setPointerCapture = vi.fn(() => {
      if (captureRejected)
        throw new DOMException("Pointer capture unavailable");
    });
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 400,
      height: 200,
      right: 400,
      bottom: 200,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    act(() => {
      fireEvent.pointerDown(canvas, { clientX: 100, clientY: 50 });
      fireEvent.pointerMove(canvas, { clientX: 300, clientY: 150 });
      fireEvent.pointerUp(canvas, { clientX: 300, clientY: 150 });
    });

    expect(call).toHaveBeenCalledWith("select_area", {
      sessionId: "area-1",
      x: 480,
      y: 270,
      width: 960,
      height: 540,
    });
  },
);
