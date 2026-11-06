import { listen } from "@tauri-apps/api/event";
// @vitest-environment jsdom
import {
  act,
  cleanup,
  render,
  renderHook,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useRecorder } from "./useRecorder";
import { call, idle, subscribe, type Snapshot } from "./bridge";

vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
  subscribe: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
let desktopErrorListener: (event: { payload: string }) => void;
let shortcutListener: (event: { payload: string }) => void;
let current: Snapshot;
let listener: (snapshot: Snapshot) => void;
let ready: boolean;
let startError: string | null;
let cancelArea = false;
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
  startError = null;
  cancelArea = false;
  vi.mocked(listen).mockImplementation(async (name, callback) => {
    if (name === "desktop-error")
      desktopErrorListener = callback as typeof desktopErrorListener;
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
          platform: "windows",
          wayland: false,
          windows: [{ id: "window-1", name: "Editor" }],
          displays: [
            {
              id: "monitor-right",
              name: "Monitor direito",
              width: 1920,
              height: 1080,
            },
          ],
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
      case "choose_area":
        if (cancelArea) return null;
        return { display: "monitor-right", sourceWidth: 1920, sourceHeight: 1080, x: 20, y: 30, width: 320, height: 240 };
      case "capture_print":
        return { image: "data:image/png;base64,YQ==" };
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
    localStorage.setItem(
      "tinrec.recorder.v2",
      JSON.stringify({ title: "Conversa" }),
    );
    render(<App />);
    const start = await screen.findByRole("button", {
      name: "Tela",
    });
    await waitFor(() => expect(start).toHaveProperty("disabled", false));
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
    await user.click(screen.getByRole("button", { name: /Biblioteca/ }));
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
    await user.click(screen.getByRole("button", { name: /Biblioteca/ }));
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
      name: "Salvar vídeos no computador",
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
      screen.getByRole("checkbox", { name: "Salvar vídeos no computador" }),
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
        title: expect.stringContaining("Tela principal (Tela)"),
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
    expect(screen.getByRole("heading", { name: "Biblioteca" })).not.toBeNull();
    await act(async () => shortcutListener({ payload: "settings" }));
    expect(
      screen.getByRole("heading", { name: "Configurações" }),
    ).not.toBeNull();
  });
  it("migra a preferência antiga de câmera para tela no Windows", async () => {
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
    expect(screen.queryByRole("button", { name: "Somente câmera" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Selecionar webcam" }),
    ).not.toBeNull();
    expect(
      JSON.parse(localStorage.getItem("tinrec.recorder.v2")!).captureMode,
    ).toBe("screen");
  });
  it("inicia ao escolher a janela quando ainda não existe uma fonte definida", async () => {
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
    localStorage.setItem(
      "tinrec.recorder.v2",
      JSON.stringify({ window: "window-1" }),
    );
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
    cancelArea = true;
    await act(async () => shortcutListener({ payload: "area" }));
    expect(vi.mocked(call).mock.calls.filter(([name]) => name === "start_recording")).toHaveLength(1);
    cancelArea = false;
    await act(async () => shortcutListener({ payload: "area" }));
    expect(call).toHaveBeenCalledWith("start_recording", {
      request: expect.objectContaining({ captureMode: "area", display: "monitor-right", crop: expect.objectContaining({ x:20, y:30, width:320, height:240 }) }),
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
it("mantém preferências de armazenamento de vídeo e print independentes", async () => {
  render(<App />);
  await userEvent.click(screen.getByRole("button", { name: "Configurações" }));
  const videos = screen.getByRole("checkbox", {
    name: "Salvar vídeos no computador",
  });
  const prints = screen.getByRole("checkbox", {
    name: "Salvar prints no computador",
  });
  await userEvent.click(prints);
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("set_print_keep_local", { keep: false }),
  );
  expect(videos).toHaveProperty("checked", true);
  expect(prints).toHaveProperty("checked", false);
  expect(screen.getByText("/tmp/Tinrec/Videos")).toBeTruthy();
  expect(screen.getByText("/tmp/Tinrec/Prints")).toBeTruthy();
});
it("encaminha cada atalho ao modo de print correspondente", async () => {
  render(<App />);
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Tela" })).toHaveProperty(
      "disabled",
      false,
    ),
  );
  const modes = [
    ["screenshot-screen", "screen"],
    ["screenshot-window", "window"],
    ["screenshot", "area"],
  ];
  for (const [index, [action, mode]] of modes.entries()) {
    await act(async () => shortcutListener({ payload: action }));
    await waitFor(() =>
      expect(
        vi.mocked(call).mock.calls.filter(([name]) => name === "close_print"),
      ).toHaveLength(index + 1),
    );
    expect(call).toHaveBeenCalledWith(
      "capture_print",
      expect.objectContaining({ mode }),
    );
  }
});

it("reports background browser failures while browsing the print library", async () => {
  render(<App />);
  await userEvent.click(screen.getByRole("button", { name: "Prints" }));
  await waitFor(() => expect(desktopErrorListener).toBeTypeOf("function"));
  await act(async () =>
    desktopErrorListener({
      payload: "Não foi possível abrir o navegador. Use Abrir no site.",
    }),
  );
  expect(screen.getByRole("alert").textContent).toContain(
    "Não foi possível abrir o navegador",
  );
});

it("aguarda a descoberta pendente antes de iniciar e reutiliza uma única consulta", async () => {
  const original = vi.mocked(call).getMockImplementation()!;
  let resolveCameras!: (value: { id: string; name: string }[]) => void;
  const discovery = new Promise<{ id: string; name: string }[]>((resolve) => {
    resolveCameras = resolve;
  });
  vi.mocked(call).mockImplementation((command, args) =>
    command === "get_cameras" ? (discovery as never) : original(command, args),
  );
  const { result } = renderHook(() => useRecorder());
  await waitFor(() => expect(result.current.loading).toBe(false));
  let start!: Promise<unknown>;
  act(() => {
    start = result.current.start("screen");
  });
  expect(call).not.toHaveBeenCalledWith("start_recording", expect.anything());
  expect(
    vi.mocked(call).mock.calls.filter(([name]) => name === "get_cameras"),
  ).toHaveLength(1);
  await act(async () => {
    resolveCameras([{ id: "slow-camera", name: "USB" }]);
    await start;
  });
  expect(call).toHaveBeenCalledWith("start_recording", {
    request: expect.objectContaining({ camera: "slow-camera" }),
  });
});

it("mantém a gravação de tela disponível quando a descoberta opcional de câmera falha", async () => {
  const original = vi.mocked(call).getMockImplementation()!;
  let rejectCameras!: (error: Error) => void;
  const discovery = new Promise<never>((_, reject) => {
    rejectCameras = reject;
  });
  vi.mocked(call).mockImplementation((command, args) =>
    command === "get_cameras" ? discovery : original(command, args),
  );
  const { result } = renderHook(() => useRecorder());
  await waitFor(() => expect(result.current.loading).toBe(false));
  let start!: Promise<unknown>;
  act(() => {
    start = result.current.start("screen");
  });
  await act(async () => {
    rejectCameras(new Error("Camera indisponível"));
    await start;
  });
  expect(call).toHaveBeenCalledWith("start_recording", {
    request: expect.objectContaining({ camera: "", webcamEnabled: false }),
  });
});
