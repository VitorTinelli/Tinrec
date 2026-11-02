import {
  defaultRecordingName,
  captureType,
  resolveRecordingName,
  recordingSource,
  recordingNameError,
} from "./recordingName";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  busy,
  call,
  idle,
  message,
  subscribe,
  type Snapshot,
  type Sources,
  type CaptureMode,
  type Tools,
  type Settings,
  type RecordingFile,
} from "./bridge";

const defaults: Settings = {
  captureMode: "screen",
  window: "",
  camera: "",
  webcamEnabled: false,
  keepLocal: true,
  title: defaultRecordingName,
  display: "auto",
  microphoneDevice: null,
  microphone: true,
  systemAudio: true,
  microphoneVolume: 80,
  systemVolume: 80,
  resolution: 1080,
};
function loadSettings(): Settings {
  try {
    const saved = localStorage.getItem("tinrec.recorder.v2");
    const data: Partial<Settings> = JSON.parse(
      saved ?? localStorage.getItem("tinrec.recorder.v1") ?? "{}",
    );
    return {
      ...defaults,
      captureMode: ["screen", "window", "area"].includes(data.captureMode ?? "")
        ? data.captureMode!
        : "screen",
      window: typeof data.window === "string" ? data.window : "",
      webcamEnabled: data.webcamEnabled === true,
      camera: typeof data.camera === "string" ? data.camera : "",
      keepLocal: typeof data.keepLocal === "boolean" ? data.keepLocal : true,
      title:
        typeof data.title === "string" && (saved !== null || data.title.trim())
          ? data.title
          : defaultRecordingName,
      display: typeof data.display === "string" ? data.display : "auto",
      microphoneDevice:
        typeof data.microphoneDevice === "string"
          ? data.microphoneDevice
          : null,
      microphone: typeof data.microphone === "boolean" ? data.microphone : true,
      systemAudio:
        typeof data.systemAudio === "boolean" ? data.systemAudio : true,
      resolution: data.resolution === 720 ? 720 : 1080,
      microphoneVolume: Number.isFinite(data.microphoneVolume)
        ? Math.max(0, Math.min(100, Number(data.microphoneVolume)))
        : 80,
      systemVolume: Number.isFinite(data.systemVolume)
        ? Math.max(0, Math.min(100, Number(data.systemVolume)))
        : 80,
    };
  } catch {
    return defaults;
  }
}
export function useRecorder() {
  const [settings, setSettings] = useState(loadSettings);
  const [snapshot, setSnapshot] = useState(idle);
  const [sources, setSources] = useState<Sources | null>(null);
  const [tools, setTools] = useState<Tools | null>(null);
  const [folder, setFolder] = useState("");
  const [files, setFiles] = useState<RecordingFile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mounted = useRef(false);
  const [cameraLoading, setCameraLoading] = useState(false);
  const [cameras, setCameras] = useState<{ id: string; name: string }[]>([]);
  const captureSources = sources ? { ...sources, cameras } : null;
  const refreshCameras = useCallback(async () => {
    setCameraLoading(true);
    try {
      const cameras = await call<{ id: string; name: string }[]>("get_cameras");
      if (mounted.current) setCameras(cameras);
    } catch (error) {
      if (mounted.current) setError(message(error));
    } finally {
      if (mounted.current) setCameraLoading(false);
    }
  }, []);
  useEffect(() => {
    if (settings.webcamEnabled) void refreshCameras();
  }, [settings.webcamEnabled, refreshCameras]);
  const accept = useCallback((next: Snapshot) => {
    if (mounted.current)
      setSnapshot((previous) =>
        next.revision >= previous.revision ? next : previous,
      );
  }, []);
  const refreshFiles = useCallback(async () => {
    const files = await call<RecordingFile[]>("list_recordings");
    if (mounted.current) setFiles(files);
  }, []);
  const refresh = useCallback(async () => {
    setError(null);
    setLoading(true);
    const results = await Promise.allSettled([
      call<Sources>("get_sources"),
      call<Tools>("get_tools"),
      call<string>("get_folder"),
      refreshFiles(),
      call<Snapshot>("get_snapshot"),
    ]);
    if (!mounted.current) return;
    const [sourceResult, toolsResult, folderResult, , snapshotResult] = results;
    if (sourceResult.status === "fulfilled") setSources(sourceResult.value);
    if (toolsResult.status === "fulfilled") setTools(toolsResult.value);
    if (folderResult.status === "fulfilled") setFolder(folderResult.value);
    if (snapshotResult.status === "fulfilled") accept(snapshotResult.value);
    const failed = results.find((result) => result.status === "rejected");
    if (failed?.status === "rejected") setError(message(failed.reason));
    setLoading(false);
  }, [accept, refreshFiles]);
  useEffect(() => {
    mounted.current = true;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    subscribe(accept)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    void refresh();
    let checking = false;
    const timer = setInterval(async () => {
      if (checking) return;
      checking = true;
      try {
        accept(await call<Snapshot>("get_snapshot"));
      } catch {
        /* Initial load/action reports unavailable runtime. */
      } finally {
        checking = false;
      }
    }, 1000);
    return () => {
      disposed = true;
      mounted.current = false;
      unlisten?.();
      clearInterval(timer);
    };
  }, [accept, refresh]);
  useEffect(() => {
    try {
      localStorage.setItem("tinrec.recorder.v2", JSON.stringify(settings));
    } catch {
      /* Settings remain available in memory. */
    }
  }, [settings]);
  useEffect(() => {
    if (["completed", "failed"].includes(snapshot.phase))
      void refreshFiles().catch((e) => setError(message(e)));
  }, [snapshot.phase, refreshFiles]);
  async function act(name: string, operation: () => Promise<void>) {
    if (pending) return;
    setPending(name);
    setError(null);
    try {
      await operation();
    } catch (e) {
      setError(message(e));
    } finally {
      if (mounted.current) setPending(null);
    }
  }
  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    setSettings((previous) => ({ ...previous, [key]: value }));
  }
  return {
    settings,
    cameraLoading,
    refreshCameras,
    update,
    snapshot,
    sources: captureSources,
    tools,
    folder,
    files,
    error,
    pending,
    loading,
    active: busy(snapshot.phase),
    refresh,
    start: (mode?: CaptureMode, overrides: Partial<Settings> = {}) =>
      act("start", async () => {
        const request = {
          ...settings,
          ...overrides,
          captureMode: mode ?? settings.captureMode,
        };
        if (sources?.wayland) request.display = "auto";
        if (
          request.captureMode === "window" &&
          !sources?.wayland &&
          !request.window
        )
          throw new Error("Escolha uma janela para gravar.");
        if (request.webcamEnabled && !request.camera)
          throw new Error("Escolha uma câmera para gravar.");
        const title = resolveRecordingName(
          request.title,
          new Date(),
          recordingSource(request, captureSources),
          crypto.randomUUID().replaceAll("-", "").slice(0, 6),
          captureType(request.captureMode),
        );
        const error = recordingNameError(settings.title, title);
        if (error) throw new Error(error);
        accept(
          await call<Snapshot>("start_recording", {
            request: { ...request, title },
          }),
        );
      }),
    control: (action: string) =>
      act(action, async () =>
        accept(await call<Snapshot>("control_recording", { action })),
      ),
    chooseFolder: () =>
      act("folder", async () => {
        const folder = await call<string | null>("choose_folder");
        if (folder) {
          setFolder(folder);
          await refreshFiles();
        }
      }),
    hide: () =>
      act("hide", async () => {
        await call("hide_to_tray");
      }),
    quit: () =>
      act("quit", async () => {
        await call("quit_app");
      }),
    openFolder: () =>
      act("open-folder", async () => {
        await call("open_folder");
      }),
    openFile: (fileName: string) =>
      act("open-file", async () => {
        await call("open_recording", { fileName });
      }),
  };
}
