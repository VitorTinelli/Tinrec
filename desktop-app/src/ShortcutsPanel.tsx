import { listen } from "@tauri-apps/api/event";
import {
  useCallback,
  useRef,
  useEffect,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import { Keyboard, X } from "lucide-react";
import { call, message } from "./bridge";
import { PrintShortcutsPanel } from "./screenshots/PrintShortcutsPanel";
import {
  readPrintBindings,
  savePrintBindings,
  validatePrintBindings,
} from "./screenshots/toolShortcuts";

export const shortcutActions = [
  ["screenshot", "Print de área"],
  ["screenshot-screen", "Print de tela"],
  ["screenshot-window", "Print de janela"],
  ["screen", "Gravar tela"],
  ["window", "Gravar janela"],
  ["area", "Gravar área"],
  ["show", "Abrir gravador"],
  ["start", "Iniciar gravação"],
  ["stop", "Parar e salvar"],
  ["pause", "Pausar ou retomar"],
  ["microphone", "Silenciar ou ativar microfone"],
  ["system", "Silenciar ou ativar som do computador"],
  ["controls", "Mostrar mini menu"],
  ["library", "Abrir biblioteca"],
] as const;
const shortcutGroups = [
  {
    title: "Gravação",
    actions: ["screen", "window", "area", "start", "stop", "pause", "controls"],
  },
  {
    title: "Prints",
    actions: ["screenshot-screen", "screenshot-window", "screenshot"],
  },
  { title: "Áudio", actions: ["microphone", "system"] },
  { title: "Navegação", actions: ["show", "library"] },
];
interface ShortcutView {
  bindings: Record<string, string>;
  active: Record<string, string>;
  wayland: boolean;
  error: string | null;
}
const empty: ShortcutView = {
  bindings: {},
  active: {},
  wayland: false,
  error: null,
};
export function captureShortcut(
  event: KeyboardEvent<HTMLInputElement>,
): string | null {
  if (
    !event.code ||
    /^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code) ||
    event.code === "Unidentified"
  )
    return null;
  return withModifiers(event, event.code);
}
function withModifiers(
  event: {
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    metaKey: boolean;
  },
  code: string,
) {
  const keys = [];
  if (event.ctrlKey) keys.push("Ctrl");
  if (event.altKey) keys.push("Alt");
  if (event.shiftKey) keys.push("Shift");
  if (event.metaKey) keys.push("Super");
  keys.push(code.replace(/^Key(?=[A-Z]$)|^Digit(?=[0-9]$)/, ""));
  return keys.join("+");
}
export function ShortcutsPanel() {
  const [view, setView] = useState<ShortcutView>(empty);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [printDraft, setPrintDraft] = useState(readPrintBindings);
  const captureQueue = useRef<Promise<void>>(Promise.resolve());
  const capture = useCallback((enabled: boolean) => {
    const next = captureQueue.current
      .catch(() => {})
      .then(() => call<void>("shortcut_capture", { enabled }));
    captureQueue.current = next;
    return next;
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const resume = () => {
      void capture(false).catch(() => {});
    };
    const pauseFocusedField = () => {
      if (document.activeElement?.matches("[data-shortcut-input]"))
        void capture(true).catch(() => {});
    };
    window.addEventListener("blur", resume);
    window.addEventListener("focus", pauseFocusedField);
    void listen<ShortcutView>("shortcuts-state", (event) => {
      if (!disposed) setView(event.payload);
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    void call<ShortcutView>("get_shortcuts")
      .then((value) => {
        if (!disposed && value) {
          setView(value);
          setDraft(value.bindings);
        }
      })
      .catch((error) => {
        if (!disposed) setError(message(error));
      });
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("blur", resume);
      window.removeEventListener("focus", pauseFocusedField);
      void capture(false).catch(() => {});
    };
  }, [capture]);
  function update(action: string, value: string) {
    setDraft((previous) => ({ ...previous, [action]: value }));
    setSaved(false);
  }
  async function save() {
    setPending(true);
    setError(null);
    setSaved(false);
    try {
      validatePrintBindings(printDraft);
      await capture(false);
      const value = await call<ShortcutView>("save_shortcuts", {
        bindings: draft,
      });
      savePrintBindings(printDraft);
      setView(value);
      setDraft(value.bindings);
      setSaved(true);
    } catch (error) {
      setError(message(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <section
      className="panel settings-panel shortcuts-panel"
      aria-labelledby="shortcuts-heading"
    >
      <div className="panel-heading">
        <h2 id="shortcuts-heading">
          <Keyboard size={17} />
          Atalhos de teclado e mouse
        </h2>
      </div>
      {shortcutGroups.map((group) => (
        <details
          className="shortcut-group"
          key={group.title}
          aria-label={group.title}
        >
          <summary>
            {group.title}
            <span>{group.actions.length} atalhos</span>
          </summary>
          <div className="shortcut-list">
            {group.actions
              .map((id) => shortcutActions.find(([action]) => action === id)!)
              .map(([action, label]) => (
                <div className="shortcut-row" key={action}>
                  <div>
                    <label htmlFor={`shortcut-${action}`}>{label}</label>
                    {view.active[action] ? (
                      <span className="shortcut-active">
                        Ativo: {view.active[action]}
                      </span>
                    ) : null}
                  </div>
                  <input
                    id={`shortcut-${action}`}
                    data-shortcut-input
                    aria-label={`Atalho: ${label}`}
                    placeholder="Não configurado"
                    value={(draft[action] ?? "")
                      .replace(/\bKey([A-Z])\b/g, "$1")
                      .replace(/\bDigit([0-9])\b/g, "$1")}
                    disabled={pending}
                    spellCheck={false}
                    maxLength={80}
                    onFocus={() => void capture(true).catch(() => {})}
                    onBlur={() => void capture(false).catch(() => {})}
                    onChange={(event) => update(action, event.target.value)}
                    onContextMenu={(event) => event.preventDefault()}
                    onMouseDown={(event: MouseEvent<HTMLInputElement>) => {
                      if (document.activeElement !== event.currentTarget) {
                        if (event.button !== 0) event.preventDefault();
                        event.currentTarget.focus();
                        return;
                      }
                      const button = [
                        "MouseLeft",
                        "MouseMiddle",
                        "MouseRight",
                        "MouseX1",
                        "MouseX2",
                      ][event.button];
                      if (button) {
                        event.preventDefault();
                        update(action, withModifiers(event, button));
                        event.currentTarget.blur();
                      }
                    }}
                    onAuxClick={(event) => event.preventDefault()}
                    onKeyDown={(event) => {
                      const value = captureShortcut(event);
                      if (value) {
                        event.preventDefault();
                        event.stopPropagation();
                        if (!event.repeat) {
                          update(action, value);
                          event.currentTarget.blur();
                        }
                      }
                    }}
                  />
                  <button
                    className="icon-button"
                    aria-label={`Remover atalho: ${label}`}
                    disabled={pending || !draft[action]}
                    onClick={() => update(action, "")}
                  >
                    <X size={14} />
                  </button>
                </div>
              ))}
          </div>
        </details>
      ))}
      <PrintShortcutsPanel
        draft={printDraft}
        pending={pending}
        onChange={(bindings) => {
          setPrintDraft(bindings);
          setSaved(false);
          setError(null);
        }}
      />
      {(error ?? view.error) ? (
        <p className="name-error" role="alert">
          {error ?? view.error}
        </p>
      ) : null}
      <div className="shortcut-save">
        <button
          className="button secondary"
          onClick={() => void save()}
          disabled={pending}
        >
          {pending ? "Aplicando atalhos…" : "Salvar atalhos"}
        </button>
        {saved ? (
          <span role="status">
            {view.error ? "Confira o aviso acima." : "Atalhos salvos."}
          </span>
        ) : null}
      </div>
    </section>
  );
}
