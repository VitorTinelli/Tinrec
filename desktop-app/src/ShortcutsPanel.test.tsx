// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ShortcutsPanel } from "./ShortcutsPanel";
import { call } from "./bridge";
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
}));
const empty = { bindings: {}, active: {}, wayland: true, error: null };
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(call).mockImplementation(async (command, args) => {
    if (command === "get_shortcuts") return empty;
    if (command === "save_shortcuts")
      return { ...empty, bindings: args?.bindings, active: args?.bindings };
  });
});
afterEach(cleanup);
it("captura a combinação, aplica pelo backend e permite removê-la", async () => {
  const user = userEvent.setup();
  render(<ShortcutsPanel />);
  for (const summary of document.querySelectorAll("summary")) await userEvent.click(summary);
  await waitFor(() => expect(call).toHaveBeenCalledWith("get_shortcuts"));
  const start = screen.getByRole("textbox", {
    name: "Atalho: Iniciar gravação",
  });
  await user.click(start);
  expect(call).toHaveBeenCalledWith("shortcut_capture", { enabled: true });
  fireEvent.keyDown(start, {
    key: "R",
    code: "KeyR",
    ctrlKey: true,
    shiftKey: true,
  });
  expect(start).toHaveProperty("value", "Ctrl+Shift+R");
  const pause = screen.getByRole("textbox", {
    name: "Atalho: Pausar ou retomar",
  });
  await user.click(pause);
  expect(pause).toHaveProperty("value", "");
  fireEvent.keyDown(pause, { key: "F5", code: "F5" });
  expect(pause).toHaveProperty("value", "F5");
  const show = screen.getByRole("textbox", { name: "Atalho: Abrir gravador" });
  await user.click(show);
  fireEvent.keyDown(show, { key: "q", code: "KeyQ" });
  expect(show).toHaveProperty("value", "Q");
  const stop = screen.getByRole("textbox", { name: "Atalho: Parar e salvar" });
  await user.click(stop);
  expect(stop).toHaveProperty("value", "");
  fireEvent.mouseDown(stop, { button: 3, ctrlKey: true });
  expect(stop).toHaveProperty("value", "Ctrl+MouseX1");
  await user.click(screen.getByRole("button", { name: "Salvar atalhos" }));
  await screen.findByText("Atalhos salvos.");
  expect(call).toHaveBeenCalledWith("save_shortcuts", {
    bindings: {
      start: "Ctrl+Shift+R",
      pause: "F5",
      show: "Q",
      stop: "Ctrl+MouseX1",
    },
  });
  await user.click(
    screen.getByRole("button", { name: "Remover atalho: Iniciar gravação" }),
  );
  expect(start).toHaveProperty("value", "");
  await user.click(screen.getByRole("button", { name: "Salvar atalhos" }));
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("save_shortcuts", {
      bindings: { start: "", pause: "F5", show: "Q", stop: "Ctrl+MouseX1" },
    }),
  );
});
it("mostra falhas de registro sem anunciar que os atalhos foram salvos", async () => {
  vi.mocked(call).mockImplementation(async (command) => {
    if (command === "get_shortcuts") return empty;
    if (command === "save_shortcuts") throw new Error("Combinação já em uso");
  });
  const user = userEvent.setup();
  render(<ShortcutsPanel />);
  for (const summary of document.querySelectorAll("summary")) await userEvent.click(summary);
  await user.click(screen.getByRole("button", { name: "Salvar atalhos" }));
  expect(await screen.findByRole("alert")).toHaveProperty(
    "textContent",
    "Combinação já em uso",
  );
  expect(screen.queryByText("Atalhos salvos.")).toBeNull();
});
