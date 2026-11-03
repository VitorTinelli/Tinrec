import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, type KeyboardEvent } from "react";
import { Keyboard, X } from "lucide-react";
import { call, message } from "./bridge";

export const shortcutActions = [
  ["screenshot", "Print de área", "Seleciona e anota uma área da tela."],
  ["screenshot-screen", "Print de tela", "Captura o monitor escolhido."],
  ["screenshot-window", "Print de janela", "Captura uma janela."],
  ["screen", "Gravar tela", "Escolhe uma tela e inicia a captura."],
  ["window", "Gravar janela", "Abre a escolha de janela para gravação."],
  ["area", "Gravar área", "Abre a seleção da área para gravação."],
  ["show", "Abrir gravador", "Mostra a janela de gravação."],
  [
    "start",
    "Iniciar gravação",
    "Usa o nome, a qualidade e o áudio configurados.",
  ],
  ["stop", "Parar e salvar", "Finaliza a gravação atual."],
  ["pause", "Pausar ou retomar", "Alterna a pausa da gravação atual."],
  [
    "microphone",
    "Silenciar ou ativar microfone",
    "Controla o microfone durante a gravação.",
  ],
  [
    "system",
    "Silenciar ou ativar som do computador",
    "Controla o áudio do computador durante a gravação.",
  ],
  ["controls", "Mostrar mini menu", "Abre os controles da gravação atual."],
  ["library", "Abrir biblioteca", "Mostra seus vídeos no aplicativo."],
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
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return null;
  if (
    !/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2])|Space|Enter|Tab|Comma|Period|Arrow(Up|Down|Left|Right))$/.test(
      event.code,
    )
  )
    return null;
  const keys = [];
  if (event.ctrlKey) keys.push("Ctrl");
  if (event.altKey) keys.push("Alt");
  if (event.shiftKey) keys.push("Shift");
  if (event.metaKey) keys.push("Super");
  keys.push(event.code);
  return keys.join("+");
}
export function ShortcutsPanel() {
  const [view, setView] = useState<ShortcutView>(empty);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const resume = () => {
      void call("shortcut_capture", { enabled: false }).catch(() => {});
    };
    window.addEventListener("blur", resume);
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
      void call("shortcut_capture", { enabled: false }).catch(() => {});
    };
  }, []);
  function update(action: string, value: string) {
    setDraft((previous) => ({ ...previous, [action]: value }));
    setSaved(false);
  }
  async function save() {
    setPending(true);
    setError(null);
    setSaved(false);
    try {
      await call("shortcut_capture", { enabled: false });
      const value = await call<ShortcutView>("save_shortcuts", {
        bindings: draft,
      });
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
          Atalhos de teclado
        </h2>
      </div>
      {shortcutGroups.map((group) => (
        <section
          className="shortcut-group"
          key={group.title}
          aria-label={group.title}
        >
          <h3>{group.title}</h3>
          <div className="shortcut-list">
            {group.actions
              .map((id) => shortcutActions.find(([action]) => action === id)!)
              .map(([action, label, description]) => (
                <div className="shortcut-row" key={action}>
                  <div>
                    <label htmlFor={`shortcut-${action}`}>{label}</label>
                    <p>{description}</p>
                    {view.active[action] ? (
                      <span className="shortcut-active">
                        Ativo: {view.active[action]}
                      </span>
                    ) : null}
                  </div>
                  <input
                    id={`shortcut-${action}`}
                    aria-label={`Atalho: ${label}`}
                    placeholder="Não configurado"
                    value={draft[action] ?? ""}
                    disabled={pending}
                    spellCheck={false}
                    maxLength={80}
                    onFocus={() =>
                      void call("shortcut_capture", { enabled: true }).catch(
                        () => {},
                      )
                    }
                    onBlur={() =>
                      void call("shortcut_capture", { enabled: false }).catch(
                        () => {},
                      )
                    }
                    onChange={(event) => update(action, event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        event.currentTarget.blur();
                        return;
                      }
                      const value = captureShortcut(event);
                      if (value) {
                        event.preventDefault();
                        event.stopPropagation();
                        if (!event.repeat) update(action, value);
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
        </section>
      ))}
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
