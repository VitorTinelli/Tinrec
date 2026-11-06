// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { call } from "./bridge";
import { MonitorAreaWindow } from "./MonitorAreaWindow";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(),
}));
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
}));
vi.mock("./screenshots/PrintOverlay", () => ({
  PrintOverlay: ({ onCancel }: { onCancel: () => void }) => (
    <div role="dialog" aria-label="Editor pronto">
      <button onClick={onCancel}>Cancelar edição</button>
    </div>
  ),
}));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.resetAllMocks();
});
it.each(["monitor-area-0", "monitor-area-1"])(
  "isolates %s events and keeps the selection until the editor decodes",
  async (label) => {
    vi.mocked(getCurrentWebviewWindow).mockReturnValue({ label } as ReturnType<
      typeof getCurrentWebviewWindow
    >);
    const callbacks = new Map<string, (event: { payload: unknown }) => void>();
    let receive!: (event: { payload: unknown }) => void;
    vi.mocked(listen).mockImplementation(async (name, callback) => {
      callbacks.set(name, callback as (event: { payload: unknown }) => void);
      if (name === "print-area-selection") receive = callback as typeof receive;
      return () => {};
    });
    const preview = {
      sessionId: "pick",
      sequence: 1,
      image: "",
      width: 1920,
      height: 1080,
      sourceWidth: 1920,
      sourceHeight: 1080,
    };
    vi.mocked(call).mockImplementation(async (command) =>
      command === "get_area_pick" ? preview : null,
    );
    let decoded!: () => void;
    vi.stubGlobal(
      "Image",
      class {
        onload: (() => void) | null = null;
        set src(_value: string) {
          queueMicrotask(() => this.onload?.());
        }
        decode() {
          return new Promise<void>((resolve) => {
            decoded = resolve;
          });
        }
      },
    );
    vi.stubGlobal("PointerEvent", MouseEvent);
    const { container } = render(<MonitorAreaWindow />);
    await screen.findByRole("dialog", { name: "Selecionar área de gravação" });
    expect(screen.queryByRole("button", { name: "Cancelar" })).toBeNull();
    expect(
      screen.getByText("Arraste para selecionar uma área neste monitor."),
    ).toBeTruthy();
    for (const name of ["print-area-selection", "area-pick", "area-reset"]) {
      expect(listen).toHaveBeenCalledWith(name, expect.any(Function), {
        target: { kind: "WebviewWindow", label },
      });
    }
    const canvas = container.querySelector(".area-canvas") as HTMLDivElement;
    canvas.getBoundingClientRect = () => ({
      left: 0,
      top: 0,
      width: 1920,
      height: 1080,
      right: 1920,
      bottom: 1080,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });
    act(() => {
      fireEvent.pointerDown(canvas, { button: 0, clientX: 480, clientY: 270 });
      fireEvent.pointerUp(canvas, { clientX: 960, clientY: 540 });
    });
    await waitFor(() =>
      expect(call).toHaveBeenCalledWith("finish_area_pick", {
        sessionId: "pick",
        x: 480,
        y: 270,
        width: 480,
        height: 270,
      }),
    );
    expect(container.querySelector(".area-rectangle")).toBeTruthy();
    expect(container.querySelector(".monitor-area-hint")).toBeNull();
    await act(async () => {
      receive({
        payload: {
          ...preview,
          sessionId: "print",
          image: `data:image/png;base64,${label}`,
        },
      });
    });
    expect(screen.queryByRole("dialog", { name: "Editor pronto" })).toBeNull();
    expect(container.querySelector(".area-rectangle")).toBeTruthy();
    await act(async () => {
      decoded();
    });
    await screen.findByRole("dialog", { name: "Editor pronto" });
    fireEvent.click(screen.getByRole("button", { name: "Cancelar edição" }));
    expect(call).toHaveBeenCalledWith("close_print");
    await act(async () => callbacks.get("area-reset")!({ payload: null }));
    expect(screen.queryByRole("dialog", { name: "Editor pronto" })).toBeNull();
    await act(async () =>
      callbacks.get("area-pick")!({
        payload: { ...preview, sessionId: "pick-next" },
      }),
    );
    await screen.findByRole("dialog", { name: "Selecionar área de gravação" });
    await act(async () =>
      receive({
        payload: {
          ...preview,
          sessionId: "print-next",
          image: `data:image/png;base64,${label}`,
        },
      }),
    );
    await act(async () => callbacks.get("area-reset")!({ payload: null }));
    await act(async () => decoded());
    expect(screen.queryByRole("dialog", { name: "Editor pronto" })).toBeNull();
  },
);
