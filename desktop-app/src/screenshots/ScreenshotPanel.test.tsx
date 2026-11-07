// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ScreenshotPanel } from "./ScreenshotPanel";
import { PrintSourceWindow } from "./PrintSourceWindow";
import { call } from "../bridge";
vi.mock("../bridge", async (original) => ({
  ...(await original<typeof import("../bridge")>()),
  call: vi.fn(),
}));
const sources = {
  wayland: false,
  displays: [{ id: "display:1", name: "Tela 1", width: 1920, height: 1080 }],
  windows: [
    { id: "window:1", name: "Documento" },
    { id: "window:2", name: "Navegador" },
  ],
};
beforeEach(() => {
  window.history.replaceState({}, "", "/?window=print-source&mode=window");
  vi.mocked(call).mockImplementation(async (command) =>
    command === "get_sources"
      ? sources
      : command === "list_prints"
        ? []
        : undefined,
  );
});
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it("opens the independent capture window from the app and shortcut", async () => {
  const { rerender } = render(
    <ScreenshotPanel
      sources={null}
      display="auto"
      windowId=""
      disabled={false}
      onDisplay={vi.fn()}
      onWindow={vi.fn()}
      shortcut={0}
    />,
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Print da janela" }),
  );
  expect(call).toHaveBeenCalledWith("start_print_capture", { mode: "window" });
  rerender(
    <ScreenshotPanel
      sources={null}
      display="auto"
      windowId=""
      disabled={false}
      onDisplay={vi.fn()}
      onWindow={vi.fn()}
      shortcut={1}
      shortcutMode="screen"
    />,
  );
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("start_print_capture", {
      mode: "screen",
    }),
  );
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(
    vi.mocked(call).mock.calls.some(([name]) => name === "show_recorder"),
  ).toBe(false);
});
it("chooses a window and hides only the picker before opening the floating editor", async () => {
  render(<PrintSourceWindow />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Navegador" }),
  );
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("capture_print", {
      mode: "window",
      display: "auto",
      window: "window:2",
      action: "edit",
    }),
  );
  expect(call).toHaveBeenCalledWith("print_source_visibility", {
    visible: false,
  });
  await waitFor(() => expect(call).toHaveBeenCalledWith("close_print_source"));
  expect(
    vi.mocked(call).mock.calls.some(([name]) => name === "show_recorder"),
  ).toBe(false);
});
it("captures a sole monitor directly without showing a source selector", async () => {
  window.history.replaceState({}, "", "/?window=print-source&mode=screen");
  render(<PrintSourceWindow />);
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith(
      "capture_print",
      expect.objectContaining({
        mode: "screen",
        display: "display:1",
        action: "edit",
      }),
    ),
  );
  expect(call).not.toHaveBeenCalledWith("print_source_visibility", {
    visible: true,
  });
});
it("asks which monitor when more than one is available", async () => {
  window.history.replaceState({}, "", "/?window=print-source&mode=screen");
  vi.mocked(call).mockImplementation(async (command) =>
    command === "get_sources"
      ? {
          ...sources,
          displays: [
            ...sources.displays,
            { id: "display:2", name: "Tela 2", width: 2560, height: 1440 },
          ],
        }
      : undefined,
  );
  render(<PrintSourceWindow />);
  await userEvent.click(await screen.findByRole("button", { name: /Tela 2/ }));
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith(
      "capture_print",
      expect.objectContaining({ display: "display:2" }),
    ),
  );
});
it("cancels the small window without capturing", async () => {
  render(<PrintSourceWindow />);
  await screen.findByRole("button", { name: "Documento" });
  await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(call).toHaveBeenCalledWith("close_print_source");
  expect(
    vi.mocked(call).mock.calls.some(([name]) => name === "capture_print"),
  ).toBe(false);
});
