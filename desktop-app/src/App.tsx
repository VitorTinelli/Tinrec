import { ScreenshotPanel } from "./screenshots/ScreenshotPanel";
import { DeviceCard } from "./DeviceCard";
import { ShortcutsPanel } from "./ShortcutsPanel";
import { listen } from "@tauri-apps/api/event";
import {
  defaultRecordingName,
  captureType,
  resolveRecordingName,
  recordingSource,
  recordingNameError,
  nameVariables,
} from "./recordingName";
import { useEffect, useRef, useState } from "react";
import { RecordingLibrary } from "./RecordingLibrary";
import {
  Library,
  Circle,
  Monitor,
  Mic,
  Volume2,
  FolderOpen,
  ChevronRight,
  Play,
  Pause,
  Square,
  AlertCircle,
  ShieldCheck,
  Settings2,
  AppWindow,
  Scan,
  Image,
} from "lucide-react";
import { duration, type Phase, type CaptureMode } from "./bridge";
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
      <span className="device-card-name">Padrão do sistema</span>
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
  const [printMode, setPrintMode] = useState(false);
  const [printBusy, setPrintBusy] = useState(false);
  const printBusyRef = useRef(false);
  printBusyRef.current = printBusy;
  const [printShortcut, setPrintShortcut] = useState(0);
  const [printShortcutMode, setPrintShortcutMode] = useState<
    "screen" | "window" | "area"
  >("area");
  const mainRef = useRef<HTMLElement>(null);
  const [variablesOpen, setVariablesOpen] = useState(false);
  const [view, setView] = useState<
    "recorder" | "library" | "prints" | "settings"
  >("recorder");
  useEffect(() => {
    if (mainRef.current) mainRef.current.scrollTop = 0;
  }, [view]);
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
  const shortcutRecorder = useRef(recorder);
  shortcutRecorder.current = recorder;
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<string>("shortcut-action", (event) => {
      if (disposed || printBusyRef.current) return;
      if (
        ["screenshot", "screenshot-screen", "screenshot-window"].includes(
          event.payload,
        )
      ) {
        setPrintShortcutMode(
          event.payload === "screenshot-screen"
            ? "screen"
            : event.payload === "screenshot-window"
              ? "window"
              : "area",
        );
        setView("recorder");
        setPrintMode(true);
        setPrintShortcut((value) => value + 1);
        return;
      }
      setView(
        event.payload === "library"
          ? "library"
          : event.payload === "settings"
            ? "settings"
            : "recorder",
      );
      const mode = ["screen", "window", "area"].includes(event.payload)
        ? (event.payload as CaptureMode)
        : undefined;
      if (mode && shortcutRecorder.current.active) return;
      if (mode) shortcutRecorder.current.update("captureMode", mode);
      if (
        (event.payload === "start" || mode) &&
        !shortcutRecorder.current.active &&
        !shortcutRecorder.current.pending &&
        !shortcutRecorder.current.loading
      ) {
        if (
          mode === "window" &&
          !shortcutRecorder.current.sources?.wayland &&
          !shortcutRecorder.current.settings.window
        )
          return;
        void shortcutRecorder.current.start(mode);
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  const phase = snapshot.phase;
  const locked = active || pending !== null;
  const transitioning = [
    "starting",
    "stopping",
    "finalizing",
    "discarding",
  ].includes(phase);
  const namePreview = resolveRecordingName(
    settings.title,
    new Date(),
    recordingSource(settings, sources),
    "a1b2c3",
    captureType(settings.captureMode),
  );
  const nameError = recordingNameError(settings.title, namePreview);
  const issue = error ?? snapshot.error;
  const startingMode = useRef(false);
  async function startMode(
    mode: CaptureMode,
    overrides: Partial<typeof settings> = {},
  ) {
    if (locked || loading || !tools?.ready || startingMode.current) return;
    update("captureMode", mode);
    if (overrides.camera !== undefined) update("camera", overrides.camera);
    if (overrides.window !== undefined) update("window", overrides.window);
    if (nameError) {
      setView("settings");
      return;
    }
    const next = { ...settings, ...overrides, captureMode: mode };
    if (
      mode === "window" &&
      !sources?.wayland &&
      (!next.window ||
        !(sources?.windows ?? []).some((window) => window.id === next.window))
    )
      return;
    startingMode.current = true;
    try {
      await recorder.start(mode, overrides);
    } finally {
      startingMode.current = false;
    }
  }
  return (
    <div
      className={`app-shell view-${view}`}
      data-synthetic={sources?.synthetic ? "true" : "false"}
    >
      <header className="topbar">
        <div className="brand">
          <img
            className="brand-symbol"
            src="/tinrec-icon.svg"
            alt=""
            width="36"
            height="36"
          />
          <span>
            Tinrec<span className="brand-dot">.</span>
          </span>
        </div>
      </header>
      <nav className="workspace-nav" aria-label="Espaço de trabalho">
        <span className="workspace-caption">ESPAÇO PESSOAL</span>
        <button
          disabled={printBusy}
          className={view === "recorder" ? "selected" : ""}
          aria-current={view === "recorder" ? "page" : undefined}
          onClick={() => setView("recorder")}
        >
          <Circle size={17} />
          Gravador{active ? <i className="nav-recording-dot" /> : null}
        </button>
        <button
          disabled={printBusy}
          className={view === "library" ? "selected" : ""}
          aria-current={view === "library" ? "page" : undefined}
          onClick={() => setView("library")}
        >
          <Library size={17} />
          Biblioteca
        </button>
        <button
          disabled={printBusy}
          className={view === "prints" ? "selected" : ""}
          aria-current={view === "prints" ? "page" : undefined}
          onClick={() => setView("prints")}
        >
          <Image size={17} />
          Prints
        </button>
        <button
          disabled={printBusy}
          className={view === "settings" ? "selected" : ""}
          aria-current={view === "settings" ? "page" : undefined}
          onClick={() => setView("settings")}
        >
          <Settings2 size={17} />
          Configurações
        </button>
        <div className="workspace-bottom">
          <ShieldCheck size={17} />
          <strong>Seu espaço, suas ideias.</strong>
          <p>
            Grave no computador.
            <br />
            Compartilhe quando quiser.
          </p>
        </div>
      </nav>
      <main ref={mainRef}>
        <div className="page-heading">
          <div>
            <div className="eyebrow">SEU ESPAÇO DE GRAVAÇÃO</div>
            <h1>
              {view === "recorder"
                ? "Vamos criar algo?"
                : view === "library"
                  ? "Biblioteca"
                  : view === "prints"
                    ? "Prints"
                    : "Configurações"}
            </h1>
            <p>
              {view === "recorder"
                ? "Uma tela, uma ideia. Aperte gravar."
                : view === "library"
                  ? "Suas ideias ganham um lugar aqui."
                  : view === "prints"
                    ? "Capture, anote e compartilhe."
                    : "Personalize sua gravação e conecte sua conta."}
            </p>
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
        {tools && !tools.ready && view === "recorder" && !printMode ? (
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
        {view === "recorder" && (
          <div
            className="capture-kind"
            role="group"
            aria-label="Tipo de captura"
          >
            <button
              aria-pressed={!printMode}
              disabled={active || printBusy}
              onClick={() => setPrintMode(false)}
            >
              Vídeo
            </button>
            <button
              aria-pressed={printMode}
              disabled={active || printBusy}
              onClick={() => setPrintMode(true)}
            >
              Print
            </button>
          </div>
        )}
        {view === "recorder" && printMode && (
          <ScreenshotPanel
            sources={sources}
            display={settings.display}
            windowId={settings.window}
            disabled={active || loading}
            onWindow={(value) => update("window", value)}
            onDisplay={(value) => update("display", value)}
            shortcut={printShortcut}
            shortcutMode={printShortcutMode}
            captureOnly
            onBusyChange={setPrintBusy}
          />
        )}
        {view === "prints" && (
          <ScreenshotPanel
            sources={sources}
            display={settings.display}
            windowId={settings.window}
            disabled={active || loading}
            onWindow={(value) => update("window", value)}
            onDisplay={(value) => update("display", value)}
            shortcut={0}
            libraryOnly
            onBusyChange={setPrintBusy}
          />
        )}
        <div
          className="recorder-layout"
          hidden={view !== "recorder" || printMode}
        >
          <section
            className="panel configuration"
            aria-labelledby="capture-heading"
          >
            <div className="panel-heading">
              <h2 id="capture-heading">Sua gravação</h2>
              <span className="step-label">01 — CONFIGURAR</span>
            </div>
            {nameError && !active ? (
              <p className="name-error" role="alert">
                {nameError}{" "}
                <button
                  className="text-button"
                  onClick={() => setView("settings")}
                >
                  Configurar nome
                </button>
              </p>
            ) : null}
            {phase !== "idle" ? (
              <div className="compact-recording">
                <p
                  className="recording-status"
                  role="status"
                  aria-live="polite"
                >
                  {labels[phase]}
                </p>
                {active ? (
                  <>
                    <span
                      className="timer"
                      aria-label={`Tempo de gravação: ${duration(snapshot.elapsedSeconds)}`}
                    >
                      {duration(snapshot.elapsedSeconds)}
                    </span>
                    <div className="record-actions">
                      {!transitioning ? (
                        <button
                          className="button secondary"
                          disabled={!!pending}
                          onClick={() =>
                            void recorder.control(
                              phase === "paused" ? "resume" : "pause",
                            )
                          }
                        >
                          {phase === "paused" ? (
                            <Play size={14} />
                          ) : (
                            <Pause size={14} />
                          )}{" "}
                          {phase === "paused" ? "Retomar" : "Pausar"}
                        </button>
                      ) : null}
                      <button
                        className="button stop-button"
                        disabled={
                          !!pending ||
                          ["stopping", "finalizing", "discarding"].includes(
                            phase,
                          )
                        }
                        onClick={() => void recorder.control("stop")}
                      >
                        <Square size={12} fill="currentColor" />
                        {phase === "starting" ? "Cancelar" : "Encerrar"}
                      </button>
                    </div>
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
                        onClick={() =>
                          void call("show_controls").catch(() => {})
                        }
                      >
                        Mini menu ↗
                      </button>
                    </div>
                  </>
                ) : null}
              </div>
            ) : null}
            <div className="capture-modes" aria-label="O que capturar">
              {(
                [
                  ["screen", "Tela", Monitor],
                  ["window", "Janela", AppWindow],
                  ["area", "Área", Scan],
                ] as const
              ).map(([mode, label, Icon]) => (
                <button
                  type="button"
                  key={mode}
                  className={settings.captureMode === mode ? "selected" : ""}
                  aria-pressed={settings.captureMode === mode}
                  disabled={locked || loading || !tools?.ready || !!nameError}
                  data-capture-mode={mode}
                  onClick={() => void startMode(mode)}
                >
                  <Icon size={20} />
                  <span>{label}</span>
                </button>
              ))}
            </div>
            {sources?.wayland ? (
              <p className="source-mode-hint">
                Escolha o que gravar no diálogo do sistema.
              </p>
            ) : settings.captureMode === "window" ? (
              <div className="source-choice">
                <label className="field-label" htmlFor="window">
                  Janela
                </label>
                <select
                  id="window"
                  value={settings.window}
                  disabled={locked}
                  onChange={(event) => {
                    if (event.target.value)
                      void startMode("window", { window: event.target.value });
                    else update("window", "");
                  }}
                >
                  <option value="">Escolha uma janela</option>
                  {sources?.windows?.map((window) => (
                    <option key={window.id} value={window.id}>
                      {window.name}
                    </option>
                  ))}
                </select>
                <button
                  className="text-button"
                  disabled={locked || loading}
                  onClick={() => void recorder.refresh()}
                >
                  Atualizar janelas
                </button>
              </div>
            ) : (
              <div className="source-choice">
                <label className="field-label" htmlFor="display">
                  Tela
                </label>
                <select
                  id="display"
                  value={settings.display}
                  disabled={locked}
                  onChange={(event) => update("display", event.target.value)}
                >
                  <option value="auto">Tela principal</option>
                  {sources?.displays.map((display) => (
                    <option key={display.id} value={display.id}>
                      {display.name} · {display.width} × {display.height}
                    </option>
                  ))}
                </select>
                {settings.captureMode === "area" ? (
                  <p className="source-mode-hint">
                    Ao iniciar, arraste para marcar a área na tela escolhida.
                  </p>
                ) : null}
              </div>
            )}
            <div className="section-divider">
              <h3>Dispositivos</h3>
              <span>Sua voz, nos dois lados</span>
            </div>
            <div className="audio-grid has-webcam">
              <DeviceCard
                kind="microphone"
                enabled={settings.microphone}
                volume={settings.microphoneVolume}
                disabled={locked}
                value={settings.microphoneDevice ?? ""}
                devices={(sources?.microphones ?? []).map((name) => ({
                  id: name,
                  name,
                }))}
                onToggle={(value) => update("microphone", value)}
                onVolume={(value) => update("microphoneVolume", value)}
                onSelect={(value) => update("microphoneDevice", value || null)}
              />
              <AudioControl
                kind="system"
                enabled={settings.systemAudio}
                volume={settings.systemVolume}
                disabled={locked}
                onToggle={(value) => update("systemAudio", value)}
                onVolume={(value) => update("systemVolume", value)}
              />
              <DeviceCard
                kind="webcam"
                enabled={settings.webcamEnabled}
                disabled={locked}
                value={settings.camera}
                devices={sources?.cameras ?? []}
                loading={recorder.cameraLoading}
                onToggle={(value) => update("webcamEnabled", value)}
                onSelect={(value) => update("camera", value)}
                onOpen={() => void recorder.refreshCameras().catch(() => {})}
              />
            </div>
            {sources?.warnings.map((warning) => (
              <p className="device-warning" key={warning}>
                {warning}
              </p>
            ))}
          </section>
        </div>
        <div className="settings-layout" hidden={view !== "settings"}>
          <section
            className="panel settings-panel"
            aria-labelledby="settings-heading"
          >
            <div className="panel-heading">
              <h2 id="settings-heading">Gravação</h2>
            </div>
            <div className="name-field-heading">
              <label className="field-label" htmlFor="title">
                Nome do vídeo
              </label>
              <button
                type="button"
                className="text-button name-variables-toggle"
                aria-expanded={variablesOpen}
                aria-controls="name-variables-panel"
                onClick={() => setVariablesOpen((open) => !open)}
              >
                Variáveis
              </button>
            </div>
            <input
              id="title"
              className="text-input"
              placeholder={defaultRecordingName}
              maxLength={180}
              required
              aria-invalid={!!nameError}
              aria-describedby={nameError ? "name-error" : undefined}
              value={settings.title}
              disabled={locked}
              onChange={(e) => update("title", e.target.value)}
            />
            {nameError ? (
              <p className="name-error" id="name-error" role="alert">
                {nameError}
              </p>
            ) : null}
            <div id="name-variables-panel" hidden={!variablesOpen}>
              <div
                className="name-variables"
                aria-label="Variáveis para o nome"
              >
                {nameVariables.map(([variable, description]) => (
                  <button
                    key={variable}
                    type="button"
                    title={description}
                    disabled={locked}
                    onClick={() =>
                      update("title", settings.title + `{${variable}}`)
                    }
                  >
                    <code>{`{${variable}}`}</code>
                    <span>{description}</span>
                  </button>
                ))}
              </div>
              <div className="name-preview" id="name-preview">
                <span>EXEMPLO DO NOME</span>
                <strong>{nameError ?? namePreview}</strong>
              </div>
            </div>
            <button
              className="text-button"
              disabled={locked}
              onClick={() => update("title", defaultRecordingName)}
            >
              Restaurar padrão
            </button>
          </section>
          <section
            className="panel settings-panel"
            aria-labelledby="storage-heading"
          >
            <div className="panel-heading">
              <h2 id="storage-heading">Qualidade e armazenamento</h2>
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
            <label className="save-local-option">
              <span>Salvar vídeos no computador</span>
              <input
                className="toggle"
                type="checkbox"
                checked={settings.keepLocal}
                disabled={locked}
                onChange={(e) => update("keepLocal", e.target.checked)}
              />
            </label>
            <p className="storage-path">
              {folder ? `${folder}/Videos` : "Carregando pasta…"}
            </p>
            <label className="save-local-option">
              <span>Salvar prints no computador</span>
              <input
                className="toggle"
                type="checkbox"
                checked={settings.keepPrintLocal !== false}
                disabled={locked}
                onChange={(e) => update("keepPrintLocal", e.target.checked)}
              />
            </label>
            <p className="storage-path">
              {folder ? `${folder}/Prints` : "Carregando pasta…"}
            </p>
            <div className="destination">
              <span>PASTA DO TINREC</span>
              <button
                title={folder || "Pasta de gravações"}
                onClick={() => void recorder.chooseFolder()}
                disabled={locked || loading}
              >
                <FolderOpen size={16} />
                <span>{folder || "Carregando pasta…"}</span>
                <ChevronRight size={15} />
              </button>
            </div>{" "}
          </section>
          <ShortcutsPanel />
          <CloudPanel files={files} connectionOnly />
        </div>
        <div hidden={view !== "library"}>
          <RecordingLibrary
            files={files}
            folder={folder}
            pending={!!pending}
            loading={loading || locked}
            onOpen={(name) => void recorder.openFile(name)}
            onRefresh={() => void recorder.refresh()}
            onRecord={() => setView("recorder")}
          />
        </div>
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
            · v0.3
          </span>
        </footer>
      </main>
    </div>
  );
}
