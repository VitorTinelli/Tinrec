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
