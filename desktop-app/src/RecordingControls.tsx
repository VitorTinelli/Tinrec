import { useEffect, useState } from "react";
import {
  Mic,
  MicOff,
  Volume2,
  VolumeX,
  Trash2,
  Pause,
  Play,
  Square,
  GripVertical,
  PanelTop,
  AlertCircle,
} from "lucide-react";
import {
  call,
  subscribe,
  idle,
  busy,
  message,
  duration,
  type Snapshot,
} from "./bridge";
export function AudioMuteButtons({
  snapshot,
  disabled,
  onControl,
}: {
  snapshot: Snapshot;
  disabled: boolean;
  onControl: (action: string) => void;
}) {
  return (
    <>
      <button
        className={`live-control ${snapshot.microphoneMuted ? "is-muted" : ""}`}
        disabled={disabled || !snapshot.microphoneEnabled}
        title={
          snapshot.microphoneMuted ? "Ativar microfone" : "Silenciar microfone"
        }
        aria-label={
          snapshot.microphoneMuted ? "Ativar microfone" : "Silenciar microfone"
        }
        aria-pressed={!!snapshot.microphoneMuted}
        onClick={() => onControl("mute-microphone")}
      >
        {snapshot.microphoneMuted ? <MicOff size={17} /> : <Mic size={17} />}
      </button>
      <button
        className={`live-control ${snapshot.systemMuted ? "is-muted" : ""}`}
        disabled={disabled || !snapshot.systemEnabled}
        title={
          snapshot.systemMuted
            ? "Ativar áudio do computador"
            : "Silenciar áudio do computador"
        }
        aria-label={
          snapshot.systemMuted
            ? "Ativar áudio do computador"
            : "Silenciar áudio do computador"
        }
        aria-pressed={!!snapshot.systemMuted}
        onClick={() => onControl("mute-system")}
      >
        {snapshot.systemMuted ? <VolumeX size={17} /> : <Volume2 size={17} />}
      </button>
    </>
  );
}
export function DiscardControl({
  disabled,
  onDiscard,
}: {
  disabled: boolean;
  onDiscard: () => void;
}) {
  const [confirm, setConfirm] = useState(false);
  return (
    <>
      <button
        className="live-control discard-control"
        aria-label="Descartar gravação"
        title="Descartar gravação"
        disabled={disabled}
        onClick={() => setConfirm(true)}
      >
        <Trash2 size={17} />
      </button>
      {confirm ? (
        <div
          className="discard-confirm"
          role="alertdialog"
          aria-label="Excluir gravação atual"
        >
          <span>Excluir este vídeo sem enviar?</span>
          <button onClick={() => setConfirm(false)}>Manter</button>
          <button
            className="confirm-delete"
            disabled={disabled}
            onClick={() => {
              setConfirm(false);
              onDiscard();
            }}
          >
            Excluir vídeo
          </button>
        </div>
      ) : null}
    </>
  );
}
export function FloatingRecorder() {
  const [snapshot, setSnapshot] = useState(idle),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false,
      unlisten: (() => void) | undefined,
      checking = false;
    const accept = (value: Snapshot) => {
      if (!disposed)
        setSnapshot((previous) =>
          value.revision >= previous.revision ? value : previous,
        );
    };
    subscribe(accept)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    const get = async () => {
      if (checking) return;
      checking = true;
      try {
        accept(await call<Snapshot>("get_snapshot"));
      } catch (e) {
        if (!disposed) setError(message(e));
      } finally {
        checking = false;
      }
    };
    void get();
    const timer = setInterval(() => void get(), 1000);
    return () => {
      disposed = true;
      clearInterval(timer);
      unlisten?.();
    };
  }, []);
  async function act(action: string) {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      setSnapshot(await call<Snapshot>("control_recording", { action }));
    } catch (e) {
      setError(message(e));
    } finally {
      setPending(false);
    }
  }
  const transition = !["recording", "paused"].includes(snapshot.phase),
    disabled = pending || transition;
  return (
    <div className="floating-recorder" title={error ?? undefined}>
      <div className="floating-row">
        <div className="floating-grip">
          <span
            className="floating-drag-handle"
            data-tauri-drag-region
            title="Arrastar mini menu"
            aria-label="Arrastar mini menu"
          >
            <GripVertical size={18} aria-hidden="true" />
          </span>
          {error ? (
            <span role="alert" aria-label={error}>
              <AlertCircle size={14} />
            </span>
          ) : (
            <span
              className={`floating-status ${snapshot.phase === "recording" ? "on" : ""}`}
            />
          )}
          <strong>{duration(snapshot.elapsedSeconds)}</strong>
        </div>
        <button
          className="live-control"
          aria-label={
            snapshot.phase === "paused" ? "Retomar gravação" : "Pausar gravação"
          }
          title={snapshot.phase === "paused" ? "Retomar" : "Pausar"}
          disabled={disabled}
          onClick={() =>
            void act(snapshot.phase === "paused" ? "resume" : "pause")
          }
        >
          {snapshot.phase === "paused" ? (
            <Play size={18} />
          ) : (
            <Pause size={18} />
          )}
        </button>
        <button
          className="live-control live-stop"
          aria-label="Parar gravação"
          title="Parar e salvar"
          disabled={disabled}
          onClick={() => void act("stop")}
        >
          <Square size={15} fill="currentColor" />
        </button>
        <span className="floating-divider" />
        <AudioMuteButtons
          snapshot={snapshot}
          disabled={disabled}
          onControl={(action) => void act(action)}
        />
        <DiscardControl
          key={snapshot.sessionId ?? "idle"}
          disabled={
            pending || !busy(snapshot.phase) || snapshot.phase === "discarding"
          }
          onDiscard={() => void act("discard")}
        />
        <button
          className="live-control"
          aria-label="Abrir gravador"
          title="Abrir gravador"
          onClick={() =>
            void call("show_recorder").catch((e) => setError(message(e)))
          }
        >
          <PanelTop size={17} />
        </button>
      </div>
    </div>
  );
}
