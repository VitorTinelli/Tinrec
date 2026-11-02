import { listen } from "@tauri-apps/api/event";
// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { call, idle, subscribe, type Snapshot } from "./bridge";

vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
  subscribe: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
let shortcutListener: (event: { payload: string }) => void;
let current: Snapshot;
let listener: (snapshot: Snapshot) => void;
let ready: boolean;
let wayland: boolean;
let platform: string;
let startError: string | null;
const file = {
  fileName: "Conversa - 123.mp4",
  title: "Conversa",
  bytes: 1000000,
  modifiedMs: 1000,
  duration: 8,
  status: "completed",
};
async function publish(next: Partial<Snapshot>) {
  current = { ...current, ...next, revision: current.revision + 1 };
  await act(async () => listener(current));
}
beforeEach(() => {
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  localStorage.clear();
  vi.clearAllMocks();
  current = { ...idle };
  ready = true;
  wayland = true;
  platform = "windows";
  startError = null;
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    if (name === "shortcut-action")
      shortcutListener = callback as typeof shortcutListener;
    return () => {};
  });
  vi.mocked(subscribe).mockImplementation(async (callback) => {
    listener = callback;
    return () => {};
  });
  vi.mocked(call).mockImplementation(async (command, args) => {
    switch (command) {
      case "get_snapshot":
        return current;
      case "get_sources":
        return {
          platform,
          wayland,
          windows: [{ id: "window-1", name: "Editor" }],
          displays: [],
          microphones: ["USB Mic"],
          warnings: [],
        };
      case "get_tools":
        return {
          ready,
          ffmpeg: null,
          ffprobe: null,
          message: ready ? null : "Instale FFmpeg com libx264 e ffprobe.",
        };
      case "get_cameras":
        return [{ id: "camera-0", name: "USB Camera" }];
      case "get_folder":
        return "/tmp/Tinrec";
      case "list_recordings":
        return current.phase === "completed" ? [file] : [];
      case "start_recording":
        if (startError) throw new Error(startError);
        current = {
          ...current,
          revision: current.revision + 1,
          phase: "starting",
          fileName: file.fileName,
        };
        return current;
      case "control_recording":
        current = {
          ...current,
          revision: current.revision + 1,
          phase:
            args?.action === "pause"
              ? "paused"
              : args?.action === "resume"
                ? "recording"
                : "finalizing",
        };
        return current;
      default:
        return undefined;
    }
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
describe("gravador desktop", () => {
  it("no Linux mantém os três modos e envia a intenção de área ao portal", async () => {
    platform = "linux";
    const user = userEvent.setup();
    render(<App />);
    const button = await screen.findByRole("button", { name: "Área" });
    await waitFor(() => expect(button).toHaveProperty("disabled", false));
    for (const name of ["Tela", "Janela", "Área"])
      expect(screen.getByRole("button", { name })).not.toBeNull();
    await user.click(button);
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({
        captureMode: "area",
        display: "auto",
      }),
    });
    expect(
      screen.queryByRole("dialog", { name: "Selecionar área de gravação" }),
    ).toBeNull();
  });
  it("mantém três modos no macOS", async () => {
    platform = "macos";
    wayland = false;
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    for (const name of ["Tela", "Janela", "Área"])
      expect(screen.getByRole("button", { name })).not.toBeNull();
  });
  it("inclui a webcam selecionada na gravação de tela", async () => {
    const user = userEvent.setup();
    render(<App />);
    const start = await screen.findByRole("button", { name: "Tela" });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
    await user.click(screen.getByRole("checkbox", { name: "Webcam" }));
    await user.click(screen.getByRole("button", { name: "Selecionar webcam" }));
    await user.click(await screen.findByRole("button", { name: "USB Camera" }));
    await user.click(start);
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({
        captureMode: "screen",
        webcamEnabled: true,
        camera: "camera-0",
      }),
    });
  });
  it("envia dispositivos e volumes, pausa, retoma, finaliza e abre o vídeo salvo", async () => {
    const user = userEvent.setup();
    render(<App />);
    const start = await screen.findByRole("button", {
      name: "Tela",
    });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    await user.clear(screen.getByLabelText(/Nome do vídeo/));
    await user.type(screen.getByLabelText(/Nome do vídeo/), "Conversa");
    await user.click(screen.getByRole("button", { name: "Gravador" }));
    await user.click(
      screen.getByRole("button", { name: "Selecionar microfone" }),
    );
    await user.click(screen.getByRole("button", { name: "USB Mic" }));
    await user.click(start);
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({
        title: "Conversa",
        microphoneDevice: "USB Mic",
        microphoneVolume: 80,
        systemVolume: 80,
      }),
    });
    await publish({ phase: "recording", elapsedSeconds: 5 });
    await user.click(screen.getByRole("button", { name: /Meus vídeos/ }));
    await user.click(screen.getByRole("button", { name: "Gravador" }));
    expect(screen.getByRole("button", { name: "Pausar" })).toHaveProperty(
      "disabled",
      false,
    );
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    expect(screen.getByLabelText(/Nome do vídeo/)).toHaveProperty(
      "disabled",
      true,
    );
    await user.click(screen.getByRole("button", { name: "Gravador" }));
    await user.click(screen.getByRole("button", { name: "Pausar" }));
    await screen.findByRole("button", { name: "Retomar" });
    expect(call).toHaveBeenCalledWith("control_recording", { action: "pause" });
    await user.click(screen.getByRole("button", { name: "Retomar" }));
    await screen.findByRole("button", { name: "Pausar" });
    await user.click(screen.getByRole("button", { name: "Encerrar" }));
    await screen.findByText("Salvando seu vídeo");
    expect(
      vi
        .mocked(call)
        .mock.calls.filter(([name]) => name === "control_recording"),
    ).toEqual(
      ["pause", "resume", "stop"].map((action) => [
        "control_recording",
        { action },
      ]),
    );
    expect(screen.getByRole("button", { name: "Encerrar" })).toHaveProperty(
      "disabled",
      true,
    );
    await publish({ phase: "completed", elapsedSeconds: 8 });
    await user.click(screen.getByRole("button", { name: /Meus vídeos/ }));
    await screen.findByRole("button", { name: "Abrir Conversa" });
    await user.click(screen.getByRole("button", { name: "Abrir Conversa" }));
    expect(call).toHaveBeenCalledWith("open_recording", {
      fileName: file.fileName,
    });
    await user.click(screen.getByRole("button", { name: "Gravador" }));
    expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
      "disabled",
      false,
    );
  });
  it("exige um nome e salva o modelo para a próxima abertura", async () => {
    const user = userEvent.setup();
    const { unmount } = render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    const title = screen.getByRole("textbox", { name: /Nome do vídeo/ });
    expect(title).toHaveProperty("required", true);
    expect(screen.getByLabelText("Qualidade")).toHaveProperty("value", "1080");
    const keepLocal = screen.getByRole("checkbox", {
      name: "Manter uma cópia no computador",
    });
    expect(keepLocal).toHaveProperty("checked", true);
    await user.click(keepLocal);
    expect(title).toHaveProperty(
      "value",
      "{fonte} ({tipo}) - {data} às {hora}",
    );
    await user.clear(title);
    await user.click(screen.getByRole("button", { name: "Gravador" }));
    expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
      "disabled",
      true,
    );
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    await user.type(title, "Reunião");
    expect(screen.queryByRole("button", { name: /\{data\}/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Variáveis" }));
    await user.click(screen.getByRole("button", { name: /\{data\}/ }));
    await user.click(screen.getByRole("button", { name: "Variáveis" }));
    expect(screen.queryByRole("button", { name: /\{data\}/ })).toBeNull();
    expect(title).toHaveProperty("value", "Reunião{data}");
    unmount();
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await user.click(screen.getByRole("button", { name: "Configurações" }));
    expect(
      screen.getByRole("textbox", { name: /Nome do vídeo/ }),
    ).toHaveProperty("value", "Reunião{data}");
    expect(
      screen.getByRole("checkbox", { name: "Manter uma cópia no computador" }),
    ).toHaveProperty("checked", false);
  });
  it("inicia pelos atalhos globais e navega sem disparar outra gravação em andamento", async () => {
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await act(async () => shortcutListener({ payload: "start" }));
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({
        title: expect.stringContaining("Tela selecionada"),
      }),
    });
    await publish({ phase: "recording" });
    const count = vi
      .mocked(call)
      .mock.calls.filter(([name]) => name === "start_recording").length;
    await act(async () => shortcutListener({ payload: "start" }));
    expect(
      vi.mocked(call).mock.calls.filter(([name]) => name === "start_recording"),
    ).toHaveLength(count);
    await act(async () => shortcutListener({ payload: "library" }));
    expect(screen.getByRole("heading", { name: "Meus vídeos" })).not.toBeNull();
    await act(async () => shortcutListener({ payload: "settings" }));
    expect(
      screen.getByRole("heading", { name: "Configurações" }),
    ).not.toBeNull();
  });
  it.each(["linux", "windows", "macos"])(
    "remove Somente câmera e migra a preferência antiga em %s",
    async (os) => {
      platform = os;
      localStorage.setItem(
        "tinrec.recorder.v2",
        JSON.stringify({ captureMode: "camera" }),
      );
      render(<App />);
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
          "disabled",
          false,
        ),
      );
      expect(
        screen.queryByRole("button", { name: "Somente câmera" }),
      ).toBeNull();
      expect(
        screen.getByRole("button", { name: "Selecionar webcam" }),
      ).not.toBeNull();
      expect(
        JSON.parse(localStorage.getItem("tinrec.recorder.v2")!).captureMode,
      ).toBe("screen");
    },
  );
  it("inicia ao escolher a janela quando ainda não existe uma fonte definida", async () => {
    wayland = false;
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await user.click(screen.getByRole("button", { name: "Janela" }));
    expect(
      vi.mocked(call).mock.calls.filter(([name]) => name === "start_recording"),
    ).toHaveLength(0);
    await user.selectOptions(screen.getByLabelText("Janela"), "window-1");
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({
        captureMode: "window",
        window: "window-1",
      }),
    });
  });
  it("usa atalhos separados para janela e área", async () => {
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await act(async () => shortcutListener({ payload: "window" }));
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({ captureMode: "window" }),
    });
    await publish({ phase: "idle" });
    await act(async () => shortcutListener({ payload: "area" }));
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({ captureMode: "area" }),
    });
  });
  it("impede gravação quando falta o encoder e explica como preparar", async () => {
    ready = false;
    render(<App />);
    await screen.findByText("Instale FFmpeg com libx264 e ffprobe.");
    expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
      "disabled",
      true,
    );
  });
  it("mostra falha de início e permite nova tentativa", async () => {
    startError = "Microfone indisponível";
    const user = userEvent.setup();
    render(<App />);
    const start = await screen.findByRole("button", {
      name: "Tela",
    });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
    await user.click(start);
    await screen.findByText("Microfone indisponível");
    expect(start).toHaveProperty("disabled", false);
    startError = null;
    await user.click(start);
    expect(
      vi.mocked(call).mock.calls.filter(([name]) => name === "start_recording"),
    ).toHaveLength(2);
    await publish({ phase: "recording" });
    expect(screen.getByRole("button", { name: "Pausar" })).toHaveProperty(
      "disabled",
      false,
    );
    expect(screen.queryByText("Microfone indisponível")).toBeNull();
  });
  it("ignora eventos antigos após uma transição mais recente", async () => {
    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
        "disabled",
        false,
      ),
    );
    await publish({ phase: "paused", elapsedSeconds: 3 });
    await act(async () =>
      listener({ ...idle, revision: 0, phase: "recording" }),
    );
    expect(screen.getByText("Gravação pausada")).not.toBeNull();
  });
});
