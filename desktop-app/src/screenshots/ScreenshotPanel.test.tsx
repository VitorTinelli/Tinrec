// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByText("Abrir rascunho editável")).toBeNull();
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
});
