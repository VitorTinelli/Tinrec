import { useEffect, useState, useRef } from "react";
export interface Playback {
  title: string;
  mediaType?: "video" | "image";
  embedUrl: string | null;
  playbackUrl?: string | null;
  playbackExpiresAt?: number | null;
  driveReady?: boolean;
  canConfirmDrive?: boolean;
}
export function VideoPlayer({
  video,
  publicId,
  onRefresh,
}: {
  video: Playback;
  publicId: string;
  onRefresh: () => void;
}) {
  const playback = useRef({ position: 0, playing: false, loading: false });
  const [testDrive, setTestDrive] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    setTestDrive(false);
    setError("");
  }, [publicId]);
  async function confirm() {
    setPending(true);
    setError("");
    try {
      const r = await fetch(
        `/api/public/${encodeURIComponent(publicId)}/drive-ready`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: "{}",
        },
      );
      if (!r.ok) {
        const e = await r.json();
        throw new Error(
          e.message ?? "Não foi possível confirmar a reprodução.",
        );
      }
      onRefresh();
      setTestDrive(false);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setPending(false);
    }
  }
  const temporary = !!video.playbackUrl && !video.driveReady && !testDrive;
  return (
    <>
      <div className="player">
        {temporary ? (
          <video
            src={video.playbackUrl!}
            onLoadStart={() => {
              playback.current.loading = true;
            }}
            onTimeUpdate={(e) => {
              if (!playback.current.loading)
                playback.current.position = e.currentTarget.currentTime;
            }}
            onPlay={() => {
              playback.current.playing = true;
            }}
            onPause={(e) => {
              if (e.currentTarget.readyState > 0 && !playback.current.loading)
                playback.current.playing = false;
            }}
            onLoadedMetadata={(e) => {
              const state = playback.current,
                element = e.currentTarget;
              if (state.position > 0 && state.position < element.duration)
                element.currentTime = state.position;
              state.loading = false;
              if (state.playing) void element.play().catch(() => {});
            }}
            controls
            playsInline
            preload="metadata"
            crossOrigin="anonymous"
            onError={() => {
              setError(
                "Não foi possível carregar a cópia temporária. Tente atualizar o vídeo.",
              );
            }}
          />
        ) : (
          <iframe
            src={video.embedUrl!}
            title={video.title}
            allow="autoplay; fullscreen"
            allowFullScreen
          />
        )}
      </div>
      {error ? (
        <p role="alert">
          {error}{" "}
          <button
            onClick={() => {
              setError("");
              onRefresh();
            }}
          >
            Atualizar vídeo
          </button>
        </p>
      ) : null}
      <div className="player-note">
        <p>
          {temporary
            ? "Reprodução temporária disponível. O arquivo também está sendo salvo no Drive do autor."
            : "Reprodução pelo Google Drive."}
        </p>
      </div>
      {video.canConfirmDrive &&
      video.playbackUrl &&
      video.embedUrl &&
      !video.driveReady ? (
        <section className="drive-confirmation">
          <strong>Confira o player definitivo</strong>
          <p>
            Quando o vídeo reproduzir no Drive, confirme abaixo. A cópia
            temporária será removida automaticamente após 24 horas.
          </p>
          <button
            className="text-button"
            onClick={() => setTestDrive((value) => !value)}
          >
            {testDrive ? "Voltar à cópia temporária" : "Testar player do Drive"}
          </button>
          {testDrive ? (
            <button
              className="primary-button"
              disabled={pending}
              onClick={() => void confirm()}
            >
              {pending
                ? "Confirmando…"
                : "Assisti no Drive; confirmar reprodução"}
            </button>
          ) : null}
        </section>
      ) : null}
    </>
  );
}
