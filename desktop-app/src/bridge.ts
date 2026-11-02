import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Phase =
  | "idle"
  | "starting"
  | "recording"
  | "paused"
  | "stopping"
  | "finalizing"
  | "completed"
  | "failed"
  | "discarding"
  | "discarded";
export interface Snapshot {
  revision: number;
  sessionId?: string | null;
  microphoneEnabled?: boolean;
  systemEnabled?: boolean;
  microphoneMuted?: boolean;
  systemMuted?: boolean;
  webcamAvailable?: boolean;
  webcamEnabled?: boolean;
  phase: Phase;
  elapsedSeconds: number;
  videoFrames: number;
  audioPeak: [number, number];
  fileName: string | null;
  error: string | null;
}
export type CaptureMode = "screen" | "window" | "area";
export interface Sources {
  synthetic?: boolean;
  platform: string;
  wayland: boolean;
  displays: { id: string; name: string; width: number; height: number }[];
  windows?: { id: string; name: string }[];
  cameras?: { id: string; name: string }[];
  microphones: string[];
  warnings: string[];
}
export interface Tools {
  ready: boolean;
  ffmpeg: string | null;
  ffprobe: string | null;
  message: string | null;
}
export interface Settings {
  captureMode: CaptureMode;
  window: string;
  camera: string;
  webcamEnabled: boolean;
  keepLocal: boolean;
  title: string;
  display: string;
  microphoneDevice: string | null;
  microphone: boolean;
  systemAudio: boolean;
  microphoneVolume: number;
  systemVolume: number;
  resolution: number;
}
export interface RecordingFile {
  fileName: string;
  title: string;
  bytes: number;
  modifiedMs: number;
  duration: number | null;
  status: string;
}

export const idle: Snapshot = {
  revision: 0,
  phase: "idle",
  elapsedSeconds: 0,
  videoFrames: 0,
  audioPeak: [0, 0],
  fileName: null,
  error: null,
};
export const busy = (phase: Phase) =>
  [
    "starting",
    "recording",
    "paused",
    "stopping",
    "finalizing",
    "discarding",
  ].includes(phase);
export function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    return Promise.reject(
      new Error(
        "Abra o aplicativo Tinrec para acessar a tela, o microfone e os arquivos locais.",
      ),
    );
  return invoke<T>(command, args);
}
export const subscribe = (callback: (snapshot: Snapshot) => void) =>
  listen<Snapshot>("recorder-state", (event) => callback(event.payload));
export function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
export function duration(seconds: number): string {
  const value = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor(value / 60) % 60;
  return `${hours ? `${String(hours).padStart(2, "0")}:` : ""}${String(minutes).padStart(2, "0")}:${String(value % 60).padStart(2, "0")}`;
}
export function size(bytes: number): string {
  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(1)} GB`
    : `${(bytes / 1024 ** 2).toFixed(1)} MB`;
}

export interface Preview {
  sessionId: string;
  sequence: number;
  image: string;
  width: number;
  height: number;
  sourceWidth: number;
  sourceHeight: number;
}
export const subscribePreview = (callback: (preview: Preview) => void) =>
  listen<Preview>("recorder-preview", (event) => callback(event.payload));
