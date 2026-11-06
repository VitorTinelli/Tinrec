import { afterEach, expect, it, vi } from "vitest";
vi.mock("./bridge", () => ({ call: vi.fn() }));
vi.mock("./screenshots/render", () => ({
  exportPng: vi.fn(async () => "png"),
}));
import { call } from "./bridge";
afterEach(() => {
  vi.useRealTimers();
  vi.resetModules();
  vi.resetAllMocks();
});
it("exercises native recorder IPC once, including area selection and discard", async () => {
  vi.useFakeTimers();
  let phase = "idle";
  let id = 0;
  let resolvePrint!: (value: unknown) => void;
  let rejectPrint!: (reason: Error) => void;
  vi.mocked(call).mockImplementation(async (command, args) => {
    if (
      command === "self_test_mode" ||
      command === "self_test_capture_border" ||
      command === "self_test_print_ready"
    )
      return true as never;
    if (command === "start_recording") {
      id++;
      phase =
        id <= 2 || (args?.request as { crop?: unknown })?.crop
          ? "recording"
          : "starting";
    }
    if (command === "self_test_area_pick")
      return { count: 2, labels: ["left", "right"] } as never;
    if (command === "choose_area")
      return {
        display: "display:right",
        sourceWidth: 2560,
        sourceHeight: 1440,
        x: 10,
        y: 10,
        width: 320,
        height: 240,
      } as never;
    if (command === "self_test_window")
      return (args?.action === "open" ? "window:123" : true) as never;
    if (command === "capture_print") {
      if (args?.mode === "area")
        return (await new Promise<unknown>((resolve, reject) => {
          resolvePrint = resolve;
          rejectPrint = reject;
        })) as never;
      return { image: args?.action === "save" ? "" : "png" } as never;
    }
    if (command === "export_print") return { fileName: "print.png" } as never;
    if (command === "list_prints") return [{ fileName: "print.png" }] as never;
    if (command === "select_print_area") {
      resolvePrint({ image: "", width: 320, height: 240 });
      return undefined as never;
    }
    if (command === "get_print_area")
      return {
        sessionId: "print",
        image: "png",
        initialSelection: { x: 10, y: 10, width: 320, height: 240 },
        sourceWidth: 640,
        sourceHeight: 480,
      } as never;
    if (command === "self_test_close_selector") {
      if (args?.label === "area") phase = "discarded";
      else rejectPrint(new Error("Captura cancelada"));
    }
    if (command === "select_area") phase = "recording";
    if (command === "control_recording")
      phase = (
        {
          pause: "paused",
          resume: "recording",
          stop: "completed",
          discard: "discarded",
        } as Record<string, string>
      )[String(args?.action)];
    if (command === "get_preview")
      return {
        sessionId: String(id),
        sourceWidth: 640,
        sourceHeight: 480,
      } as never;
    return {
      phase,
      sessionId: String(id),
      fileName: "native.mp4",
      videoFrames: 2,
    } as never;
  });
  const { runInstalledTest } = await import("./selfTest");
  const first = runInstalledTest();
  expect(runInstalledTest()).toBe(first);
  await vi.runAllTimersAsync();
  await first;
  expect(call).toHaveBeenCalledWith("select_area", {
    sessionId: "6",
    x: 0,
    y: 0,
    width: 320,
    height: 240,
  });
  for (const action of ["pause", "resume", "stop", "discard"])
    expect(call).toHaveBeenCalledWith("control_recording", { action });
  expect(call).toHaveBeenCalledWith("self_test_close_selector", {
    label: "area",
  });
  expect(call).toHaveBeenCalledWith("self_test_close_selector", {
    label: "print-area",
  });
  expect(call).toHaveBeenCalledWith(
    "export_print",
    expect.objectContaining({ action: "save" }),
  );
  expect(call).toHaveBeenCalledWith("desktop_ready", { error: null });
});
it("reports native failures instead of accepting the synthetic result", async () => {
  vi.mocked(call).mockImplementation(async (command) => {
    if (command === "self_test_mode") return true as never;
    if (command === "start_recording") throw new Error("WGC unavailable");
    return undefined as never;
  });
  const { runInstalledTest } = await import("./selfTest");
  await runInstalledTest();
  expect(call).toHaveBeenCalledWith("desktop_ready", {
    error: expect.stringContaining("WGC unavailable"),
  });
});

it("does not start recording during normal application startup", async () => {
  vi.mocked(call).mockResolvedValue(false as never);
  const { runInstalledTest } = await import("./selfTest");
  await runInstalledTest();
  expect(vi.mocked(call).mock.calls.map(([command]) => command)).toEqual([
    "self_test_mode",
  ]);
});
