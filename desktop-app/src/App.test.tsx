// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { call, idle, subscribe, type Snapshot } from "./bridge";

vi.mock("./bridge", async original => ({ ...await original<typeof import("./bridge")>(), call: vi.fn(), subscribe: vi.fn() }));
let current: Snapshot;
let listener: (snapshot: Snapshot) => void;
let ready: boolean;
let startError: string | null;
const file = { fileName: "Conversa - 123.mp4", title: "Conversa", bytes: 1000000, modifiedMs: 1000, duration: 8, status: "completed" };
async function publish(next: Partial<Snapshot>) {
  current = { ...current, ...next, revision: current.revision + 1 };
  await act(async () => listener(current));
}
beforeEach(() => {
  localStorage.clear(); vi.clearAllMocks(); current = { ...idle }; ready = true; startError = null;
  vi.mocked(subscribe).mockImplementation(async callback => { listener = callback; return () => {}; });
  vi.mocked(call).mockImplementation(async (command, args) => {
    switch (command) {
      case "get_snapshot": return current;
      case "get_sources": return { platform: "linux", wayland: true, displays: [], microphones: ["USB Mic"], warnings: [] };
      case "get_tools": return { ready, ffmpeg: null, ffprobe: null, message: ready ? null : "Instale FFmpeg com libx264 e ffprobe." };
      case "get_folder": return "/tmp/Tinrec";
      case "list_recordings": return current.phase === "completed" ? [file] : [];
      case "start_recording":
        if (startError) throw new Error(startError);
        current = { ...current, revision: current.revision + 1, phase: "starting", fileName: file.fileName }; return current;
      case "control_recording":
        current = { ...current, revision: current.revision + 1, phase: args?.action === "pause" ? "paused" : args?.action === "resume" ? "recording" : "finalizing" }; return current;
      default: return undefined;
    }
  });
});
afterEach(cleanup);
describe("gravador desktop", () => {
  it("envia dispositivos e volumes, pausa, retoma, finaliza e abre o vídeo salvo", async () => {
    const user = userEvent.setup(); render(<App />);
    const start = await screen.findByRole("button", { name: "Iniciar gravação" });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
    await user.type(screen.getByLabelText(/Nome do vídeo/), "Conversa");
    await user.selectOptions(screen.getByLabelText("Entrada do microfone"), "USB Mic");
    await user.click(start);
    expect(call).toHaveBeenCalledWith("start_recording", { request: expect.objectContaining({ title: "Conversa", microphoneDevice: "USB Mic", microphoneVolume: 80, systemVolume: 80 }) });
    await publish({ phase: "recording", elapsedSeconds: 5 });
    expect(screen.getByRole("button", { name: "Pausar" })).toHaveProperty("disabled", false);
    expect(screen.getByLabelText(/Nome do vídeo/)).toHaveProperty("disabled", true);
    await user.click(screen.getByRole("button", { name: "Pausar" }));
    await screen.findByRole("button", { name: "Retomar" });
    await user.click(screen.getByRole("button", { name: "Retomar" }));
    await screen.findByRole("button", { name: "Pausar" });
    await user.click(screen.getByRole("button", { name: "Encerrar" }));
    await screen.findByText("Salvando seu vídeo");
    expect(screen.getByRole("button", { name: "Encerrar" })).toHaveProperty("disabled", true);
    await publish({ phase: "completed", elapsedSeconds: 8 });
    await screen.findByText("Conversa");
    await user.click(screen.getByRole("button", { name: "Abrir Conversa" }));
    expect(call).toHaveBeenCalledWith("open_recording", { fileName: file.fileName });
    expect(screen.getByRole("button", { name: "Nova gravação" })).toHaveProperty("disabled", false);
  });
  it("impede gravação quando falta o encoder e explica como preparar", async () => {
    ready = false; render(<App />);
    await screen.findByText("Instale FFmpeg com libx264 e ffprobe.");
    expect(screen.getByRole("button", { name: "Iniciar gravação" })).toHaveProperty("disabled", true);
  });
  it("mostra falha de início e permite nova tentativa", async () => {
    startError = "Microfone indisponível";
    const user = userEvent.setup(); render(<App />);
    const start = await screen.findByRole("button", { name: "Iniciar gravação" });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
    await user.click(start);
    await screen.findByText("Microfone indisponível");
    expect(start).toHaveProperty("disabled", false);
  });
  it("ignora eventos antigos após uma transição mais recente", async () => {
    render(<App />); await screen.findByText("USB Mic");
    await publish({ phase: "paused", elapsedSeconds: 3 });
    await act(async () => listener({ ...idle, revision: 0, phase: "recording" }));
    expect(screen.getByText("Gravação pausada")).not.toBeNull();
  });
});
