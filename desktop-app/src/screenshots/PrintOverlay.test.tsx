// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PrintOverlay } from "./PrintOverlay";
import { call } from "../bridge";
import { exportPng } from "./render";
vi.mock("../bridge", async (original) => ({
  ...(await original<typeof import("../bridge")>()),
  call: vi.fn(),
}));
vi.mock("./render", () => ({
  renderDocument: vi.fn(async () => document.createElement("canvas")),
  exportPng: vi.fn(async () => "edited-png"),
}));
const preview = {
  sessionId: "print-1",
  sequence: 1,
  image: "original",
  width: 1000,
  height: 800,
  sourceWidth: 1000,
  sourceHeight: 800,
};
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.stubGlobal("PointerEvent", MouseEvent);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  vi.mocked(call).mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
function setup() {
  const done = vi.fn();
  const cancel = vi.fn();
  const { container } = render(
    <PrintOverlay preview={preview} onDone={done} onCancel={cancel} />,
  );
  const surface = container.querySelector(".print-overlay-surface")!;
  vi.spyOn(surface, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    width: 1000,
    height: 800,
  } as DOMRect);
  return { surface, done, cancel };
}
function drag(
  surface: Element,
  x: number,
  y: number,
  endX: number,
  endY: number,
) {
  fireEvent.pointerDown(surface, { button: 0, clientX: x, clientY: y });
  fireEvent.pointerUp(surface, { clientX: endX, clientY: endY });
}
it.each(["Copiar", "Enviar ao site"])(
  "keeps selection on screen and exports edited crop for %s",
  async (label) => {
    const { surface, done } = setup();
    drag(surface, 100, 100, 500, 400);
    expect(screen.getByRole("toolbar")).toBeTruthy();
    expect(call).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Seta" }));
    drag(surface, 140, 150, 300, 250);
    await userEvent.click(screen.getByRole("button", { name: label }));
    await waitFor(() => expect(done).toHaveBeenCalledOnce());
    expect(exportPng).toHaveBeenCalledWith(
      expect.objectContaining({
        operations: [
          expect.objectContaining({
            tool: "crop",
            x: 100,
            y: 100,
            w: 400,
            h: 300,
          }),
          expect.objectContaining({
            tool: "arrow",
            x: 40,
            y: 50,
            w: 160,
            h: 100,
          }),
        ],
      }),
    );
    expect(call).toHaveBeenNthCalledWith(
      1,
      "export_print",
      expect.objectContaining({
        image: "edited-png",
        action: label === "Copiar" ? "copy" : "upload",
      }),
    );
    expect(call).toHaveBeenNthCalledWith(2, "select_print_area", {
      sessionId: "print-1",
      returnImage: false,
      x: 100,
      y: 100,
      width: 400,
      height: 300,
    });
  },
);
it("preserves annotations on failed upload and allows retry", async () => {
  const { surface, done } = setup();
  drag(surface, 100, 100, 500, 400);
  vi.mocked(call).mockRejectedValueOnce(new Error("offline"));
  await userEvent.click(screen.getByRole("button", { name: "Enviar ao site" }));
  expect((await screen.findByRole("alert")).textContent).toContain("offline");
  expect(done).not.toHaveBeenCalled();
  expect(screen.getByRole("toolbar")).toBeTruthy();
  await userEvent.click(screen.getByRole("button", { name: "Copiar" }));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
});
it("exports highlighter strokes as points rather than an empty rectangle", async () => {
  const { surface, done } = setup();
  drag(surface, 100, 100, 500, 400);
  await userEvent.click(screen.getByRole("button", { name: "Destacar" }));
  drag(surface, 140, 150, 300, 250);
  await userEvent.click(screen.getByRole("button", { name: "Copiar" }));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  expect(exportPng).toHaveBeenCalledWith(
    expect.objectContaining({
      operations: [
        expect.anything(),
        expect.objectContaining({
          tool: "highlight",
          points: [
            { x: 40, y: 50 },
            { x: 200, y: 150 },
          ],
        }),
      ],
    }),
  );
});
it("keeps the toolbar inside the viewport and above the taskbar margin", () => {
  const { surface } = setup();
  drag(surface, 500, 500, 990, 790);
  const toolbar = screen.getByRole("toolbar");
  vi.spyOn(toolbar, "getBoundingClientRect").mockReturnValue({
    width: 700,
    height: 100,
  } as DOMRect);
  fireEvent(window, new Event("resize"));
  const left = Number.parseFloat(toolbar.style.left),
    top = Number.parseFloat(toolbar.style.top);
  expect(left).toBeGreaterThanOrEqual(16);
  expect(left + 700).toBeLessThanOrEqual(window.innerWidth - 16);
  expect(top).toBeGreaterThanOrEqual(16);
  expect(top + 100).toBeLessThanOrEqual(window.innerHeight - 80);
});

it("exports the monitor picker region without requiring a second drag", async () => {
  const initialSelection = { x: 100, y: 80, width: 400, height: 300 };
  const done = vi.fn();
  render(
    <PrintOverlay
      preview={{ ...preview, initialSelection }}
      onDone={done}
      onCancel={() => {}}
    />,
  );
  await userEvent.click(screen.getByRole("button", { name: "Copiar" }));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  expect(call).toHaveBeenCalledWith("select_print_area", {
    sessionId: preview.sessionId,
    returnImage: false,
    ...initialSelection,
  });
  expect(exportPng).not.toHaveBeenCalled();
  expect(call).toHaveBeenCalledWith("export_print_selection", {
    request: expect.objectContaining({
      sessionId: preview.sessionId,
      rect: initialSelection,
      action: "copy",
    }),
  });
});

it("numbers arrows automatically, preserves reverse direction and reuses the undone number on export", async () => {
  const { surface, done } = setup();
  drag(surface, 100, 100, 500, 400);
  await userEvent.click(screen.getByRole("button", { name: "Seta numerada" }));
  drag(surface, 400, 150, 140, 250);
  drag(surface, 140, 150, 300, 250);
  await userEvent.click(screen.getByRole("button", { name: "Desfazer" }));
  drag(surface, 180, 180, 280, 280);
  await userEvent.click(screen.getByRole("button", { name: "Copiar" }));
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  const document = vi.mocked(exportPng).mock.calls.at(-1)![0];
  const arrows = document.operations.filter(
    (op) => op.tool === "numbered-arrow",
  );
  expect(arrows.map((op) => op.number)).toEqual([1, 2]);
  expect(arrows[0]).toMatchObject({ x: 300, y: 50, w: -260, h: 100 });
});

it("handles copy/upload keys, thickness wheel and two-stage Escape locally", async () => {
  const { surface, cancel, done } = setup();
  drag(surface, 100, 100, 500, 400);
  await userEvent.click(screen.getByRole("button", { name: "Seta" }));
  const thickness = screen.getByRole("slider", { name: "Espessura" });
  fireEvent.wheel(window, { ctrlKey: true, deltaY: -100 });
  expect(thickness).toHaveProperty("value", "5");
  fireEvent.wheel(window, { ctrlKey: true, deltaY: 100 });
  expect(thickness).toHaveProperty("value", "4");
  fireEvent.wheel(window, { deltaY: -100 });
  expect(thickness).toHaveProperty("value", "4");
  fireEvent.keyDown(window, { key: "Escape" });
  expect(cancel).not.toHaveBeenCalled();
  expect(
    screen.getByRole("button", { name: "Seta" }).getAttribute("aria-pressed"),
  ).toBe("false");
  fireEvent.keyDown(window, { key: "Escape", repeat: true });
  expect(cancel).not.toHaveBeenCalled();
  drag(surface, 140, 140, 300, 250);
  fireEvent.keyDown(window, { ctrlKey: true, key: "c" });
  await waitFor(() => expect(done).toHaveBeenCalledTimes(1));
  expect(exportPng).not.toHaveBeenCalled();
  fireEvent.keyDown(window, { ctrlKey: true, key: "s" });
  await waitFor(() => expect(done).toHaveBeenCalledTimes(2));
  expect(call).toHaveBeenCalledWith("export_print_selection", {
    request: expect.objectContaining({ action: "upload" }),
  });
  fireEvent.keyDown(window, { key: "Escape" });
  expect(cancel).toHaveBeenCalledOnce();
});
it("moves the compact toolbar, clamps it on screen and preserves its position when changing tools", async () => {
  const { surface } = setup();
  drag(surface, 100, 100, 500, 400);
  const toolbar = screen.getByRole("toolbar");
  vi.spyOn(toolbar, "getBoundingClientRect").mockReturnValue({
    left: 100, top: 412, width: 580, height: 40,
  } as DOMRect);
  const grip = screen.getByRole("button", { name: "Mover barra de ferramentas" });
  fireEvent.pointerDown(grip, { button: 0, clientX: 105, clientY: 417 });
  fireEvent.pointerMove(grip, { clientX: 205, clientY: 217 });
  fireEvent.pointerUp(grip);
  expect(toolbar.style.left).toBe("200px");
  expect(toolbar.style.top).toBe("212px");
  await userEvent.click(screen.getByRole("button", { name: "Desfocar" }));
  expect(screen.getByText("Desfocar")).toBeTruthy();
  expect(toolbar.style.left).toBe("200px");
  expect(toolbar.style.top).toBe("212px");
  fireEvent.keyDown(grip, { key: "ArrowRight" });
  expect(toolbar.style.left).toBe("208px");
  fireEvent.pointerDown(grip, { button: 0, clientX: 105, clientY: 417 });
  fireEvent.pointerMove(grip, { clientX: -500, clientY: -500 });
  fireEvent.pointerCancel(grip);
  expect(toolbar.style.left).toBe("8px");
  expect(toolbar.style.top).toBe("8px");
  fireEvent.pointerMove(grip, { clientX: 300, clientY: 300 });
  expect(toolbar.style.left).toBe("8px");
  expect(exportPng).not.toHaveBeenCalled();
});
it("types text at the clicked point and exports an unfinished entry with Ctrl+C", async () => {
  const { surface, done } = setup();
  drag(surface, 100, 100, 500, 400);
  await userEvent.click(screen.getByRole("button", { name: "Texto" }));
  expect(screen.queryByRole("textbox")).toBeNull();
  drag(surface, 150, 180, 150, 180);
  const input = screen.getByRole("textbox", { name: "Texto da anotação" });
  expect(document.activeElement).toBe(input);
  await userEvent.type(input, "Minha anotação");
  fireEvent.keyDown(input, { key: "c", ctrlKey: true });
  await waitFor(() => expect(done).toHaveBeenCalledOnce());
  expect(exportPng).toHaveBeenCalledWith(expect.objectContaining({
    operations: expect.arrayContaining([expect.objectContaining({ tool: "text", x: 50, y: 80, text: "Minha anotação" })]),
  }));
});
