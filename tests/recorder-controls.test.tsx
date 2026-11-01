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
import {
  AudioMuteButtons,
  DiscardControl,
  FloatingRecorder,
} from "../desktop-app/src/RecordingControls";
import { CapturePreview } from "../desktop-app/src/CapturePreview";
import {
  call,
  idle,
  subscribe,
  subscribePreview,
  type Preview,
  type Snapshot,
} from "../desktop-app/src/bridge";
vi.mock("../desktop-app/src/bridge", async (original) => ({
  ...(await original<typeof import("../desktop-app/src/bridge")>()),
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
      };
      return snapshot;
    }
  });
});
afterEach(cleanup);
it("offers distinct microphone and system mute without changing capture settings", () => {
  const control = vi.fn();
  render(
    <AudioMuteButtons
      snapshot={snapshot}
      disabled={false}
      onControl={control}
    />,
  );
  fireEvent.click(screen.getByLabelText("Silenciar microfone"));
  fireEvent.click(screen.getByLabelText("Silenciar áudio do computador"));
  expect(control.mock.calls).toEqual([["mute-microphone"], ["mute-system"]]);
});
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
it("floating controls send mute and stop commands and reflect the acknowledged state", async () => {
  render(<FloatingRecorder />);
  await waitFor(() =>
    expect(
      screen.getByLabelText("Silenciar microfone").hasAttribute("disabled"),
    ).toBe(false),
  );
  fireEvent.click(screen.getByLabelText("Silenciar microfone"));
  await screen.findByLabelText("Ativar microfone");
  expect(call).toHaveBeenCalledWith("control_recording", {
    action: "mute-microphone",
  });
  fireEvent.click(screen.getByLabelText("Parar gravação"));
  await waitFor(() =>
    expect(call).toHaveBeenCalledWith("control_recording", { action: "stop" }),
  );
});
it("shows the real captured frame and rejects previews from a previous recording", async () => {
  const { rerender } = render(<CapturePreview snapshot={snapshot} />);
  const preview: Preview = {
    sessionId: "one",
    sequence: 1,
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
  expect(screen.getByText("1920 × 1080")).toBeTruthy();
  rerender(<CapturePreview snapshot={{ ...snapshot, sessionId: "two" }} />);
  await act(async () => previewListener(preview));
  expect(screen.queryByRole("img")).toBeNull();
});
