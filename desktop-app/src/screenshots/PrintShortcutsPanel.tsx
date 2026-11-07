import {
  printTools,
  type PrintBindings,
} from "./toolShortcuts";

export function PrintShortcutsPanel({
  draft,
  onChange,
  pending,
}: {
  draft: PrintBindings;
  onChange: (bindings: PrintBindings) => void;
  pending: boolean;
}) {
  return (
    <details className="shortcut-group" aria-label="Ferramentas dos prints">
      <summary>
        Ferramentas dos prints<span>{printTools.length} atalhos</span>
      </summary>
      <div className="shortcut-list">
        {printTools.map(([tool, label, fallback]) => (
          <div className="shortcut-row" key={tool}>
            <label htmlFor={`print-shortcut-${tool}`}>{label}</label>
            <input
              id={`print-shortcut-${tool}`}
              aria-label={`Ferramenta do print: ${label}`}
              value={draft[tool] ?? ""}
              placeholder={`Padrão: ${fallback}`}
              readOnly
              disabled={pending}
              onKeyDown={(event) => {
                if (event.key === "Tab") return;
                event.preventDefault();
                event.stopPropagation();
                if (event.repeat || event.nativeEvent.isComposing) return;
                if (event.key === "Backspace" || event.key === "Delete") {
                  onChange({ ...draft, [tool]: "" });
                } else if (
                  /^[a-z]$/i.test(event.key) &&
                  !event.ctrlKey &&
                  !event.metaKey &&
                  !event.altKey
                ) {
                  onChange({
                    ...draft,
                    [tool]: `${event.shiftKey ? "Shift+" : ""}${event.key.toUpperCase()}`,
                  });
                } else return;
              }}
            />
            <button
              className="icon-button"
              aria-label={`Remover atalho da ferramenta: ${label}`}
              disabled={pending}
              onClick={() => {
                onChange({ ...draft, [tool]: "" });
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
    </details>
  );
}
