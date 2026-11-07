// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ShortcutsPanel } from "../ShortcutsPanel";
import { readPrintBindings } from "./toolShortcuts";
import { call } from "../bridge";
vi.mock("../bridge", async (original) => ({
  ...(await original<typeof import("../bridge")>()),
  call: vi.fn(async (command, args) => {
    if (command === "get_shortcuts")
      return { bindings: {}, active: {}, error: null };
    if (command === "save_shortcuts")
      return { bindings: args?.bindings, active: {}, error: null };
  }),
}));
beforeEach(() => localStorage.clear());
afterEach(cleanup);
it("saves a custom binding and rejects duplicates", async () => {
  render(<ShortcutsPanel />);
  for (const summary of document.querySelectorAll("summary")) await userEvent.click(summary);
  expect(screen.queryByText(/Funcionam somente durante/)).toBeNull();
  expect(screen.queryByText(/Clique no campo e pressione/)).toBeNull();
  expect(screen.queryByText("Mostra seus vídeos no aplicativo.")).toBeNull();
  expect(screen.getAllByRole("button", { name: /^Salvar/ })).toHaveLength(1);
  const text = screen.getByRole("textbox", {
    name: "Ferramenta do print: Texto",
  });
  fireEvent.keyDown(text, { key: "p", shiftKey: true });
  await userEvent.click(screen.getByRole("button", { name: "Salvar atalhos" }));
  expect(readPrintBindings().text).toBe("Shift+P");
  expect(call).toHaveBeenCalledWith("save_shortcuts", { bindings: {} });
  fireEvent.keyDown(text, { key: "p" });
  await userEvent.click(screen.getByRole("button", { name: "Salvar atalhos" }));
  expect(screen.getByRole("alert").textContent).toContain("repetido");
  expect(readPrintBindings().text).toBe("Shift+P");
  expect(screen.queryByRole("button", { name: "Restaurar padrões" })).toBeNull();
});
