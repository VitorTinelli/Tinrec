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
import { PrintEditor } from "./PrintEditor";
import { call } from "../bridge";
import { exportPng } from "./render";
vi.mock("../bridge", async (original) => ({
  ...(await original<typeof import("../bridge")>()),
  call: vi.fn(),
}));
vi.mock("./render", () => ({
  renderDocument: vi.fn(async () => document.createElement("canvas")),
  exportPng: vi.fn(async () => "data:image/png;base64,ZmluYWw="),
  loadImage: vi.fn(),
}));
const capture = {
  id: "capture",
  image: "data:image/png;base64,b3JpZ2luYWw=",
  width: 100,
  height: 100,
  needsCrop: false,
};
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  vi.stubGlobal("PointerEvent", MouseEvent);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    setLineDash: vi.fn(),
    strokeRect: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  vi.mocked(call).mockResolvedValue({ fileName: null, uploadId: null });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it("copies, saves and uploads the final composition, never the original", async () => {
  const user = userEvent.setup();
  const { container } = render(
    <PrintEditor capture={capture} onClose={() => {}} onSaved={() => {}} />,
  );
  await user.click(screen.getByRole("button", { name: "Texto" }));
  const canvas = container.querySelector("canvas")!;
  canvas.setPointerCapture = vi.fn();
  canvas.getBoundingClientRect = () => ({
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    width: 100,
    height: 100,
    right: 100,
    bottom: 100,
    toJSON: () => ({}),
  });
  fireEvent.pointerDown(canvas, { clientX: 20, clientY: 20, button: 0 });
  for (const label of ["Copiar imagem", "Salvar", "Enviar ao site"])
    await user.click(screen.getByRole("button", { name: label }));
  const outputs = vi
    .mocked(call)
    .mock.calls.filter(([command]) => command === "export_print");
  expect(outputs.map(([, args]) => args?.action)).toEqual([
    "copy",
    "save",
    "upload",
  ]);
  expect(
    outputs.every(
      ([, args]) => args?.image === "data:image/png;base64,ZmluYWw=",
    ),
  ).toBe(true);
  expect(exportPng).toHaveBeenLastCalledWith(
    expect.objectContaining({
      operations: [expect.objectContaining({ tool: "text", text: "Texto" })],
    }),
  );
});
it("undo and redo alter the document that is actually exported", async () => {
  const user = userEvent.setup();
  render(
    <PrintEditor capture={capture} onClose={() => {}} onSaved={() => {}} />,
  );
  await user.click(screen.getByRole("button", { name: "Cabeçalho" }));
  await user.click(screen.getByRole("button", { name: "Desfazer" }));
  await user.click(screen.getByRole("button", { name: "Copiar imagem" }));
  expect(exportPng).toHaveBeenLastCalledWith(
    expect.objectContaining({ operations: [] }),
  );
  await user.click(screen.getByRole("button", { name: "Refazer" }));
  await user.click(screen.getByRole("button", { name: "Copiar imagem" }));
  expect(exportPng).toHaveBeenLastCalledWith(
    expect.objectContaining({
      operations: [expect.objectContaining({ tool: "text" })],
    }),
  );
});

it("uses local print keys and clears the tool before Escape closes the editor", async () => {
  const close = vi.fn();
  render(<PrintEditor capture={capture} onClose={close} onSaved={() => {}} />);
  await userEvent.click(screen.getByRole("button", { name: "Seta numerada" }));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(close).not.toHaveBeenCalled();
  expect(
    screen
      .getByRole("button", { name: "Seta numerada" })
      .getAttribute("aria-pressed"),
  ).toBe("false");
  const size = screen.getByRole("spinbutton", { name: /Tamanho/ });
  fireEvent.wheel(window, { ctrlKey: true, deltaY: -100 });
  expect(size).toHaveProperty("value", "5");
  fireEvent.keyDown(window, { ctrlKey: true, key: "c" });
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith(
      "export_print",
      expect.objectContaining({ action: "copy" }),
    ),
  );
  await screen.findByText("Imagem copiada.");
  fireEvent.keyDown(window, { ctrlKey: true, key: "s" });
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith(
      "export_print",
      expect.objectContaining({ action: "upload" }),
    ),
  );
  await screen.findByText("Print na fila de envio.");
  fireEvent.keyDown(window, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
});
