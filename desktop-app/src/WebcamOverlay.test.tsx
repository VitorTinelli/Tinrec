// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { call, idle, subscribe, type Snapshot } from "./bridge";
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
    return null;
  });
});
afterEach(cleanup);
it("rejeita quadros antigos ou de outra sessão e limpa ao desativar", async () => {
  const { container } = render(<WebcamOverlay />);
  await waitFor(() => expect(listen).toHaveBeenCalled());
  const frame = {
    sessionId: "recording-1",
    sequence: 2,
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
  act(() => {
    frameListener({ payload: { ...frame, sequence: 1, image: "stale-frame" } });
    frameListener({
      payload: {
        ...frame,
        sessionId: "other",
        sequence: 3,
        image: "wrong-session",
      },
    });
  });
  expect(container.querySelector("img")?.getAttribute("src")).toBe(frame.image);
  act(() => stateListener({ ...state, webcamEnabled: false }));
  expect(container.querySelector("img")).toBeNull();
  act(() => frameListener({ payload: frame }));
  expect(container.querySelector("img")).toBeNull();
});
