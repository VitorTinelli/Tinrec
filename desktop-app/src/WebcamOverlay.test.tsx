// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { call, idle, subscribe, type Snapshot } from "./bridge";
import { FloatingRecorder } from "./RecordingControls";
import { WebcamOverlay } from "./WebcamOverlay";
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
  subscribe: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
let state: Snapshot;
let stateListener: (value: Snapshot) => void;
let frameListener: (event: { payload: unknown }) => void;
beforeEach(() => {
  vi.clearAllMocks();
  state = {
    ...idle,
    revision: 1,
    phase: "recording",
    sessionId: "recording-1",
    webcamAvailable: true,
    webcamEnabled: false,
  };
  vi.mocked(subscribe).mockImplementation(async (callback) => {
    stateListener = callback;
    return () => {};
  });
  vi.mocked(listen).mockImplementation(async (_, callback) => {
    frameListener = (event) =>
      callback({ ...event, event: "webcam-preview", id: 1 });
    return () => {};
  });
  vi.mocked(call).mockImplementation(async (command) => {
    if (command === "get_snapshot") return state;
    if (command === "control_recording")
      return { ...state, revision: 2, webcamEnabled: true };
    return null;
  });
});
afterEach(cleanup);
it("ativa a webcam pelo mini menu e atualiza o botão", async () => {
  render(<FloatingRecorder />);
  const button = await screen.findByRole("button", { name: "Ativar webcam" });
  await waitFor(() => expect(button).toHaveProperty("disabled", false));
  await userEvent.click(button);
  expect(call).toHaveBeenCalledWith("control_recording", {
    action: "toggle-webcam",
  });
  expect(
    screen
      .getByRole("button", { name: "Desativar webcam" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
});
it("mostra quadros da sessão ativa e limpa ao desativar", async () => {
  const { container } = render(<WebcamOverlay />);
  await waitFor(() => expect(listen).toHaveBeenCalled());
  const frame = {
    sessionId: "recording-1",
    sequence: 1,
    image: "data:image/jpeg;base64,YQ==",
    width: 320,
    height: 320,
    sourceWidth: 320,
    sourceHeight: 320,
  };
  act(() => {
    stateListener({ ...state, webcamEnabled: true });
    frameListener({ payload: frame });
  });
  expect(container.querySelector("img")?.getAttribute("src")).toBe(frame.image);
  expect(container.querySelector("[data-tauri-drag-region]")).not.toBeNull();
  act(() => stateListener({ ...state, webcamEnabled: false }));
  expect(container.querySelector("img")).toBeNull();
  act(() => frameListener({ payload: frame }));
  expect(container.querySelector("img")).toBeNull();
});
