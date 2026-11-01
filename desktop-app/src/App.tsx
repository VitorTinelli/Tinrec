import {
  Circle,
  Monitor,
  Mic,
  Volume2,
  FolderOpen,
  ChevronRight,
  Play,
  Pause,
  Square,
  Check,
  AlertCircle,
  RefreshCw,
  Video,
  ShieldCheck,
  LoaderCircle,
  ArrowUpRight,
  Settings2,
} from "lucide-react";
import { duration, size, type Phase } from "./bridge";
import { CapturePreview } from "./CapturePreview";
import { AudioMuteButtons, DiscardControl } from "./RecordingControls";
import { call } from "./bridge";
import { CloudPanel } from "./CloudPanel";
import { useRecorder } from "./useRecorder";

const labels: Record<Phase, string> = {
  idle: "Pronto para gravar",
  starting: "Preparando captura",
  recording: "Gravando",
  paused: "Gravação pausada",
  stopping: "Encerrando gravação",
  finalizing: "Salvando seu vídeo",
  completed: "Vídeo salvo",
  failed: "Não foi possível concluir",
  discarding: "Descartando vídeo",
  discarded: "Vídeo descartado",
};

function AudioControl({
  kind,
  enabled,
  volume,
  disabled,
  onToggle,
  onVolume,
}: {
  kind: "microphone" | "system";
  enabled: boolean;
  volume: number;
  disabled: boolean;
  onToggle: (value: boolean) => void;
  onVolume: (value: number) => void;
}) {
  const name = kind === "microphone" ? "Microfone" : "Áudio do computador";
  const Icon = kind === "microphone" ? Mic : Volume2;
  return (
    <div className={`audio-control ${enabled ? "" : "muted"}`}>
      <div className="audio-heading">
        <span className="audio-icon">
          <Icon size={19} />
        </span>
        <label htmlFor={`${kind}-enabled`}>{name}</label>
        <input
          id={`${kind}-enabled`}
          className="toggle"
          type="checkbox"
          checked={enabled}
          disabled={disabled}
          onChange={(e) => onToggle(e.target.checked)}
          aria-label={`Gravar ${name.toLowerCase()}`}
        />
      </div>
      <div className="volume-row">
        <label className="sr-only" htmlFor={`${kind}-volume`}>
          Volume do {name.toLowerCase()}
        </label>
        <input
          id={`${kind}-volume`}
          type="range"
          min="0"
          max="100"
          step="1"
          value={volume}
          disabled={disabled || !enabled}
          onChange={(e) => onVolume(Number(e.target.value))}
        />
        <output htmlFor={`${kind}-volume`}>
          {enabled ? `${volume}%` : "Desligado"}
        </output>
      </div>
    </div>
  );
}

export default function App() {
  const recorder = useRecorder();
  const {
    settings,
    update,
    snapshot,
    sources,
    tools,
    folder,
    files,
    error,
    pending,
    loading,
    active,
  } = recorder;
  const phase = snapshot.phase;
  const locked = active || pending !== null;
  const transitioning = [
    "starting",
    "stopping",
    "finalizing",
    "discarding",
  ].includes(phase);
  const issue = error ?? snapshot.error;
  const currentFile = files.find((file) => file.fileName === snapshot.fileName);
  const summary =
    [
      settings.microphone ? "Microfone" : null,
      settings.systemAudio ? "Som do computador" : null,
    ]
      .filter(Boolean)
      .join(" + ") || "Sem áudio";
  return (
    <div className="app-shell">
      <header className="topbar">
        <div className="brand">
          <span className="brand-symbol">
            <Circle size={18} fill="currentColor" strokeWidth={0} />
          </span>
          <span>
            Tinrec<span className="brand-dot">.</span>
          </span>
        </div>
        <span className="local-badge">
          <ShieldCheck size={14} />{" "}
          {sources?.synthetic
            ? "Teste com imagem e áudio gerados"
            : "Salvo no seu computador"}
        </span>
      </header>
      <main>
        <div className="page-heading">
          <div>
            <div className="eyebrow">SEU ESPAÇO DE GRAVAÇÃO</div>
            <h1>Uma ideia. Um novo vídeo.</h1>
            <p>Capture sua tela e sua voz, sem complicação.</p>
          </div>
          <button
            className="button secondary folder-top"
            onClick={() => void recorder.openFolder()}
            disabled={pending !== null}
          >
            <FolderOpen size={17} /> Abrir pasta
          </button>
        </div>
        {issue ? (
          <div className="notice error" role="alert">
            <AlertCircle size={19} />
            <div>
              <strong>Algo precisa de atenção</strong>
              <p>{issue}</p>
              {snapshot.fileName && phase === "failed" ? (
                <p>
                  Os arquivos locais foram preservados. Abra a pasta para
                  conferir o vídeo ou o arquivo parcial.
                </p>
              ) : null}
            </div>
          </div>
        ) : null}
        {tools && !tools.ready ? (
          <div className="notice" role="alert">
            <Settings2 size={19} />
            <div>
              <strong>Prepare a gravação</strong>
              <p>{tools.message}</p>
              <button
                className="text-button"
                onClick={() => void recorder.refresh()}
                disabled={loading}
              >
                Verificar novamente
              </button>
            </div>
          </div>
        ) : null}
        <div className="recorder-layout">
          <section
            className="panel configuration"
            aria-labelledby="capture-heading"
          >
            <div className="panel-heading">
              <h2 id="capture-heading">Sua gravação</h2>
              <span className="step-label">01 — CONFIGURAR</span>
            </div>
            <label className="field-label" htmlFor="title">
              Nome do vídeo <span>opcional</span>
            </label>
            <input
              id="title"
              className="text-input"
              placeholder="Ex.: Apresentação do projeto"
              maxLength={90}
              value={settings.title}
              disabled={locked}
              onChange={(e) => update("title", e.target.value)}
            />
            <CapturePreview snapshot={snapshot} />
            <p className="preview-source-note">
              {sources?.wayland
                ? "Escolha a tela no diálogo do sistema ao iniciar."
                : "A prévia mostra o monitor selecionado abaixo."}
            </p>
            <div className="field-grid">
              <div>
                <label className="field-label" htmlFor="display">
                  Tela
                </label>
                <select
                  id="display"
                  value={sources?.wayland ? "auto" : settings.display}
                  disabled={locked || !!sources?.wayland}
                  onChange={(e) => update("display", e.target.value)}
                >
                  <option value="auto">
                    {sources?.wayland
                      ? "Escolher ao iniciar"
                      : "Monitor principal"}
                  </option>
                  {sources?.displays.map((display) => (
                    <option key={display.id} value={display.id}>
                      {display.name} · {display.width} × {display.height}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="field-label" htmlFor="resolution">
                  Qualidade
                </label>
                <select
                  id="resolution"
                  value={settings.resolution}
                  disabled={locked}
                  onChange={(e) => update("resolution", Number(e.target.value))}
                >
                  <option value={1080}>1080p · Full HD</option>
                  <option value={720}>720p · Arquivo menor</option>
                </select>
              </div>
            </div>
            <div className="section-divider">
              <h3>Áudio</h3>
              <span>Sua voz, nos dois lados</span>
            </div>
            <div className="audio-grid">
              <AudioControl
                kind="microphone"
                enabled={settings.microphone}
                volume={settings.microphoneVolume}
                disabled={locked}
                onToggle={(value) => update("microphone", value)}
                onVolume={(value) => update("microphoneVolume", value)}
              />
              <AudioControl
                kind="system"
                enabled={settings.systemAudio}
                volume={settings.systemVolume}
                disabled={locked}
                onToggle={(value) => update("systemAudio", value)}
                onVolume={(value) => update("systemVolume", value)}
              />
            </div>
            <label
              className="field-label microphone-label"
              htmlFor="microphone-device"
            >
              Entrada do microfone
            </label>
            <select
              id="microphone-device"
              value={settings.microphoneDevice ?? ""}
              disabled={locked || !settings.microphone}
              onChange={(e) =>
                update("microphoneDevice", e.target.value || null)
              }
            >
              <option value="">Padrão do sistema</option>
              {sources?.microphones.map((name, index) => (
                <option key={`${name}-${index}`} value={name}>
                  {name}
                </option>
              ))}
            </select>
            {sources?.warnings.map((warning) => (
              <p className="device-warning" key={warning}>
                {warning}
              </p>
            ))}
          </section>
          <aside
            className={`panel recording-panel phase-${phase}`}
            aria-labelledby="recording-heading"
          >
            <div className="panel-heading">
              <h2 id="recording-heading">Vamos gravar?</h2>
              <span className={`status-dot ${active ? "active" : ""}`} />
            </div>
            <div
              className={`record-mark ${phase === "recording" ? "pulsing" : ""}`}
              aria-hidden="true"
            >
              {phase === "completed" ? (
                <Check size={30} />
              ) : transitioning ? (
                <LoaderCircle size={30} className="spin" />
              ) : phase === "failed" ? (
                <AlertCircle size={30} />
              ) : (
                <Circle size={29} fill="currentColor" strokeWidth={0} />
              )}
            </div>
            <p className="recording-status" role="status" aria-live="polite">
              {labels[phase]}
            </p>
            <div
              className="timer"
              aria-label={`Tempo de gravação: ${duration(snapshot.elapsedSeconds)}`}
            >
              {duration(snapshot.elapsedSeconds)}
            </div>
            <p className="record-hint">
              {phase === "starting"
                ? "Autorize a captura no diálogo do sistema. Para cancelar, feche esse diálogo."
                : phase === "paused"
                  ? "Uma pausa para respirar. Seu vídeo espera."
                  : ["stopping", "finalizing", "discarding"].includes(phase)
                    ? "Finalizando e conferindo seu arquivo. Aguarde antes de fechar."
                    : phase === "recording"
                      ? "Tudo pronto. A sua tela está sendo gravada."
                      : phase === "discarded"
                        ? "O vídeo foi excluído e não será enviado."
                        : phase === "completed"
                          ? "Tudo salvo. Seu vídeo está pronto para abrir."
                          : "Até 2 horas por vídeo. Você controla o começo, a pausa e o fim."}
            </p>
            <div className="record-actions">
              {!active ? (
                <button
                  className="button primary start-button"
                  onClick={() => void recorder.start()}
                  disabled={!!pending || loading || !tools?.ready}
                >
                  <Circle size={15} fill="currentColor" strokeWidth={0} />
                  {pending === "start"
                    ? "Preparando…"
                    : phase === "completed"
                      ? "Nova gravação"
                      : "Iniciar gravação"}
                </button>
              ) : (
                <>
                  {!transitioning ? (
                    <button
                      className="button secondary"
                      onClick={() =>
                        void recorder.control(
                          phase === "paused" ? "resume" : "pause",
                        )
                      }
                      disabled={!!pending}
                    >
                      {phase === "paused" ? (
                        <Play size={16} />
                      ) : (
                        <Pause size={16} />
                      )}
                      {phase === "paused" ? "Retomar" : "Pausar"}
                    </button>
                  ) : null}
                  <button
                    className="button stop-button"
                    onClick={() => void recorder.control("stop")}
                    disabled={
                      !!pending ||
                      ["stopping", "finalizing", "discarding"].includes(phase)
                    }
                  >
                    <Square size={14} fill="currentColor" />
                    {phase === "starting" ? "Cancelar" : "Encerrar"}
                  </button>
                </>
              )}
            </div>
            {active ? (
              <div className="main-live-controls">
                <AudioMuteButtons
                  snapshot={snapshot}
                  disabled={!!pending || transitioning}
                  onControl={(action) => void recorder.control(action)}
                />
                <DiscardControl
                  key={snapshot.sessionId ?? "idle"}
                  disabled={!!pending || phase === "discarding"}
                  onDiscard={() => void recorder.control("discard")}
                />
                <button
                  className="text-button"
                  onClick={() => void call("show_controls").catch(() => {})}
                >
                  Mini menu ↗
                </button>
              </div>
            ) : null}
            {phase === "completed" && currentFile ? (
              <button
                className="text-button open-current"
                onClick={() => void recorder.openFile(currentFile.fileName)}
                disabled={!!pending}
              >
                Abrir vídeo <ArrowUpRight size={15} />
              </button>
            ) : null}
            <div className="record-summary">
              <span>
                <Monitor size={14} />
                {settings.resolution}p · MP4
              </span>
              <span>
                <Mic size={14} />
                {summary}
              </span>
            </div>
            <div className="destination">
              <span>SEUS VÍDEOS FICAM AQUI</span>
              <button
                title={folder || "Pasta de gravações"}
                onClick={() => void recorder.chooseFolder()}
                disabled={locked || loading}
              >
                <FolderOpen size={16} />
                <span>{folder || "Carregando pasta…"}</span>
                <ChevronRight size={15} />
              </button>
            </div>
            <p className="privacy-note">
              <ShieldCheck size={13} /> Seu arquivo local é preservado após o
              envio.
            </p>
          </aside>
        </div>
        <CloudPanel files={files} />
        <section className="panel library" aria-labelledby="library-heading">
          <div className="panel-heading">
            <div className="library-title">
              <h2 id="library-heading">Gravações recentes</h2>
              <span className="count-badge">{files.length}</span>
            </div>
            <button
              className="icon-button"
              aria-label="Atualizar dispositivos e gravações"
              title="Atualizar"
              onClick={() => void recorder.refresh()}
              disabled={loading || locked}
            >
              <RefreshCw size={16} className={loading ? "spin" : ""} />
            </button>
          </div>
          {files.length === 0 ? (
            <div className="empty-state">
              <span>
                <Video size={24} />
              </span>
              <div>
                <strong>Seu primeiro vídeo começa acima</strong>
                <p>As gravações salvas nesta pasta aparecem aqui.</p>
              </div>
            </div>
          ) : (
            <div className="file-list">
              {files.map((file) => (
                <div className="file-row" key={file.fileName}>
                  <span className="file-icon">
                    <Video size={19} />
                  </span>
                  <div className="file-info">
                    <strong title={file.title}>{file.title}</strong>
                    <p>
                      {new Date(file.modifiedMs).toLocaleString("pt-BR", {
                        dateStyle: "short",
                        timeStyle: "short",
                      })}
                      <span>·</span>
                      {file.duration === null
                        ? "Duração não verificada"
                        : duration(file.duration)}
                      <span>·</span>
                      {size(file.bytes)}
                      {file.status === "failed" ? (
                        <span className="file-warning">Conferir arquivo</span>
                      ) : null}
                    </p>
                  </div>
                  <button
                    className="button file-open"
                    onClick={() => void recorder.openFile(file.fileName)}
                    disabled={!!pending}
                    aria-label={`Abrir ${file.title}`}
                  >
                    <Play size={14} /> Abrir
                  </button>
                </div>
              ))}
            </div>
          )}
        </section>
        <p className="background-note">
          Ao fechar no X, o Tinrec continua na bandeja com as gravações e envios
          ativos.
        </p>
        <footer className="footer">
          <span>
            Tinrec <span>—</span> Feito para mostrar, explicar e compartilhar
            ideias.
          </span>
          <span>
            {sources?.platform === "linux"
              ? "Linux"
              : sources?.platform === "windows"
                ? "Windows"
                : sources?.platform === "macos"
                  ? "macOS"
                  : "Desktop"}{" "}
            · v0.2
          </span>
          <button
            className="text-button"
            onClick={() => void recorder.hide()}
            disabled={!!pending}
          >
            Ocultar
          </button>
          <button
            className="text-button"
            onClick={() => void recorder.quit()}
            disabled={!!pending}
          >
            Sair do Tinrec
          </button>
        </footer>
      </main>
    </div>
  );
}
