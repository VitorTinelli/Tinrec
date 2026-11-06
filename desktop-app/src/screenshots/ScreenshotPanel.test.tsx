// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ScreenshotPanel } from "./ScreenshotPanel";
import { call } from "../bridge";
vi.mock("../bridge", async (original) => ({
  ...(await original<typeof import("../bridge")>()),
  call: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
const props = {
  sources: null,
  display: "auto",
  windowId: "",
  disabled: false,
  onWindow: vi.fn(),
  onDisplay: vi.fn(),
  shortcut: 0,
};
it("copies through native storage policy without creating a duplicate save", async () => {
  vi.mocked(call).mockImplementation(async (command) => {
    if (command === "list_prints") return [] as never;
    if (command === "capture_print")
      return { image: "data:image/png;base64,original" } as never;
    return undefined as never;
  });
  render(<ScreenshotPanel {...props} />);
  await userEvent.click(screen.getByRole("button", { name: "Print da tela" }));
  await screen.findByText("Print copiado.");
  expect(call).not.toHaveBeenCalledWith(
    "export_print",
    expect.objectContaining({ action: "save" }),
  );
  expect(call).toHaveBeenCalledWith(
    "export_print",
    expect.objectContaining({
      action: "copy",
      image: "data:image/png;base64,original",
    }),
  );
  await waitFor(() => expect(call).toHaveBeenCalledWith("close_print"));
});
it("preserves the saved print and releases capture if clipboard fails", async () => {
  vi.mocked(call).mockImplementation(async (command, args) => {
    if (command === "list_prints") return [] as never;
    if (command === "capture_print") return { image: "png" } as never;
    if (command === "export_print" && args?.action === "copy")
      throw new Error(
        "Print salvo. Não foi possível copiar: clipboard unavailable",
      );
    return undefined as never;
  });
  render(<ScreenshotPanel {...props} />);
  await userEvent.click(screen.getByRole("button", { name: "Print da tela" }));
  await waitFor(() => expect(call).toHaveBeenCalledWith("close_print"));
  expect(screen.getByRole("alert").textContent).toContain("Print salvo");
  await waitFor(() =>
    expect(
      vi
        .mocked(call)
        .mock.calls.filter(([command]) => command === "list_prints"),
    ).toHaveLength(2),
  );
});

it("releases controls after cancellation without waiting for library thumbnails", async () => {
  let reads = 0;
  let release!: (files: never[]) => void;
  const thumbnails = new Promise<never[]>((resolve) => {
    release = resolve;
  });
  vi.mocked(call).mockImplementation(async (command) => {
    if (command === "list_prints") return ++reads === 1 ? [] : thumbnails;
    if (command === "capture_print") throw new Error("Captura cancelada");
    return undefined;
  });
  const busy = vi.fn();
  render(<ScreenshotPanel {...props} onBusyChange={busy} />);
  await waitFor(() => expect(reads).toBe(1));
  const capture = screen.getByRole("button", { name: "Print de uma área" });
  await userEvent.click(capture);
  await waitFor(() => expect(reads).toBe(2));
  await waitFor(() => expect(capture).toHaveProperty("disabled", false));
  expect(busy).toHaveBeenLastCalledWith(false);
  await userEvent.click(capture);
  await waitFor(() =>
    expect(
      vi
        .mocked(call)
        .mock.calls.filter(([command]) => command === "capture_print"),
    ).toHaveLength(2),
  );
  await act(async () => release([]));
});

it("uses the native capture/export path without a second clipboard command", async () => {
  vi.mocked(call).mockImplementation(async (command) =>
    command === "capture_print" ? { image: "", width: 100, height: 100 } : [],
  );
  render(<ScreenshotPanel {...props} />);
  await userEvent.click(screen.getByRole("button", { name: "Print da tela" }));
  await screen.findByText("Print copiado.");
  expect(call).toHaveBeenCalledWith(
    "capture_print",
    expect.objectContaining({ mode: "screen", action: "copy" }),
  );
  expect(
    vi.mocked(call).mock.calls.some(([command]) => command === "export_print"),
  ).toBe(false);
});
