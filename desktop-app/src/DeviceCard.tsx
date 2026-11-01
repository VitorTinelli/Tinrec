import { useEffect, useRef, useState } from "react";
import { ChevronDown, Check, Mic, Video } from "lucide-react";

export function DeviceCard({
  kind,
  enabled,
  disabled,
  value,
  devices,
  loading = false,
  volume,
  onToggle,
  onSelect,
  onVolume,
  onOpen,
}: {
  kind: "microphone" | "webcam";
  enabled: boolean;
  disabled: boolean;
  value: string;
  devices: { id: string; name: string }[];
  loading?: boolean;
  volume?: number;
  onToggle: (enabled: boolean) => void;
  onSelect: (id: string) => void;
  onVolume?: (volume: number) => void;
  onOpen?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const name = kind === "microphone" ? "Microfone" : "Webcam";
  const Icon = kind === "microphone" ? Mic : Video;
  const selected = devices.find((device) => device.id === value)?.name;
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
        trigger.current?.focus();
      }
    };
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  const expanded = open && !disabled;
  function togglePicker() {
    if (disabled) return;
    if (!open) onOpen?.();
    setOpen(!open);
  }
  return (
    <div
      ref={root}
      className={`audio-control device-card ${enabled ? "" : "muted"}`}
      onClick={(event) => {
        if (!(event.target as Element).closest("button,input,[role=dialog]"))
          togglePicker();
      }}
    >
      <div className="audio-heading">
        <button
          ref={trigger}
          className="device-card-trigger"
          type="button"
          disabled={disabled}
          aria-label={`Selecionar ${name.toLowerCase()}`}
          aria-expanded={expanded}
          aria-controls={`${kind}-devices`}
          onClick={togglePicker}
        >
          <span className="audio-icon">
            <Icon size={19} />
          </span>
          <span>{name}</span>
          <ChevronDown size={12} />
        </button>
        <input
          className="toggle"
          type="checkbox"
          checked={enabled}
          disabled={disabled}
          aria-label={kind === "microphone" ? "Gravar microfone" : "Webcam"}
          onChange={(event) => onToggle(event.target.checked)}
        />
      </div>
      <button
        className="device-card-name"
        type="button"
        disabled={disabled}
        onClick={togglePicker}
        aria-label={`Dispositivo de ${name.toLowerCase()}: ${selected ?? (kind === "microphone" ? "Padrão do sistema" : "Escolher câmera")}`}
      >
        {selected ??
          (kind === "microphone" ? "Padrão do sistema" : "Escolher câmera")}
      </button>
      {volume !== undefined && onVolume && (
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
            onChange={(event) => onVolume(Number(event.target.value))}
          />
          <output htmlFor={`${kind}-volume`}>
            {enabled ? `${volume}%` : "Desligado"}
          </output>
        </div>
      )}
      {expanded && (
        <div
          className="device-picker"
          id={`${kind}-devices`}
          role="dialog"
          aria-label={`Dispositivos de ${name.toLowerCase()}`}
        >
          {loading ? (
            <span className="device-picker-empty">Buscando câmeras…</span>
          ) : (
            <>
              {kind === "microphone" && (
                <button
                  type="button"
                  onClick={() => {
                    onSelect("");
                    setOpen(false);
                    trigger.current?.focus();
                  }}
                >
                  <span>Padrão do sistema</span>
                  {!value && <Check size={14} />}
                </button>
              )}
              {devices.map((device) => (
                <button
                  type="button"
                  key={device.id}
                  onClick={() => {
                    onSelect(device.id);
                    setOpen(false);
                    trigger.current?.focus();
                  }}
                >
                  <span>{device.name}</span>
                  {value === device.id && <Check size={14} />}
                </button>
              ))}
              {!devices.length && kind === "webcam" && (
                <span className="device-picker-empty">
                  Nenhuma câmera disponível
                </span>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
