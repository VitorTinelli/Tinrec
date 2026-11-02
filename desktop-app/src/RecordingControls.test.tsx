// @vitest-environment jsdom
import React from "react";
import { afterEach, beforeEach, it, expect, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  waitFor,
  cleanup,
  act,
} from "@testing-library/react";
import { DiscardControl, FloatingRecorder } from "./RecordingControls";
import { CapturePreview } from "./CapturePreview";
import {
  call,
  idle,
  subscribe,
  subscribePreview,
  type Preview,
  type Snapshot,
} from "./bridge";
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  call: vi.fn(),
  subscribe: vi.fn(),
  subscribePreview: vi.fn(),
}));
let snapshot: Snapshot, previewListener: (p: Preview) => void;
beforeEach(() => {
  vi.clearAllMocks();
  snapshot = {
    ...idle,
    sessionId: "one",
    phase: "recording",
    microphoneEnabled: true,
    systemEnabled: true,
    webcamAvailable: true,
    webcamEnabled: false,
  };
  vi.mocked(subscribe).mockResolvedValue(() => {});
  vi.mocked(subscribePreview).mockImplementation(async (callback) => {
    previewListener = callback;
    return () => {};
  });
  vi.mocked(call).mockImplementation(async (command, args) => {
    if (command === "get_snapshot") return snapshot;
    if (command === "get_preview") return null;
    if (command === "control_recording") {
      snapshot = {
        ...snapshot,
        revision: snapshot.revision + 1,
        microphoneMuted:
          args?.action === "mute-microphone"
            ? !snapshot.microphoneMuted
            : snapshot.microphoneMuted,
        systemMuted:
          args?.action === "mute-system"
            ? !snapshot.systemMuted
            : snapshot.systemMuted,
        webcamEnabled:
          args?.action === "toggle-webcam"
            ? !snapshot.webcamEnabled
            : snapshot.webcamEnabled,
      };
      return snapshot;
    }
  });
});
afterEach(cleanup);
it("requires explicit confirmation before discarding the current recording", () => {
  const discard = vi.fn();
  render(<DiscardControl disabled={false} onDiscard={discard} />);
  fireEvent.click(screen.getByLabelText("Descartar gravação"));
  expect(discard).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Manter"));
  expect(screen.queryByText("Excluir vídeo")).toBeNull();
  fireEvent.click(screen.getByLabelText("Descartar gravação"));
  fireEvent.click(screen.getByText("Excluir vídeo"));
  expect(discard).toHaveBeenCalledOnce();
});
it("sends each floating control once and reflects the backend acknowledgment", async () => {
  render(<FloatingRecorder />);
  await waitFor(() =>
    expect(screen.getByLabelText("Silenciar microfone")).toHaveProperty(
      "disabled",
      false,
    ),
  );
  for (const [before, after, action] of [
    ["Silenciar microfone", "Ativar microfone", "mute-microphone"],
    [
      "Silenciar áudio do computador",
      "Ativar áudio do computador",
      "mute-system",
    ],
    ["Ativar webcam", "Desativar webcam", "toggle-webcam"],
  ]) {
    fireEvent.click(screen.getByLabelText(before));
    await screen.findByLabelText(after);
    expect(call).toHaveBeenCalledWith("control_recording", { action });
  }
  fireEvent.click(screen.getByLabelText("Parar gravação"));
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("control_recording", { action: "stop" }),
  );
  expect(
    vi.mocked(call).mock.calls.filter(([name]) => name === "control_recording"),
  ).toEqual(
    ["mute-microphone", "mute-system", "toggle-webcam", "stop"].map(
      (action) => ["control_recording", { action }],
    ),
  );
});
it("shows the real captured frame and rejects previews from a previous recording", async () => {
  const { rerender } = render(<CapturePreview snapshot={snapshot} />);
  const preview: Preview = {
    sessionId: "one",
    sequence: 2,
    image: "data:image/jpeg;base64,aGVsbG8=",
    width: 480,
    height: 270,
    sourceWidth: 1920,
    sourceHeight: 1080,
  };
  await act(async () => previewListener(preview));
  expect(
    screen
      .getByAltText("Prévia da tela realmente selecionada para gravação")
      .getAttribute("src"),
  ).toBe(preview.image);
  expect(screen.getByText("1920 × 1080")).not.toBeNull();
  await act(async () =>
    previewListener({ ...preview, sequence: 1, image: "stale-frame" }),
  );
  expect(screen.getByRole("img").getAttribute("src")).toBe(preview.image);
  rerender(<CapturePreview snapshot={{ ...snapshot, sessionId: "two" }} />);
  await act(async () => previewListener(preview));
  expect(screen.queryByRole("img")).toBeNull();
});
