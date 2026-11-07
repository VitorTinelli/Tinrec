import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { nextArrowNumber, type Operation } from "./document";

export function useArrowNumber(operations: Operation[]) {
  const [number, setNumber] = useState(() => nextArrowNumber(operations));
  const previous = useRef(operations);
  useLayoutEffect(() => {
    const old = previous.current;
    previous.current = operations;
    const added = operations.filter(
      (op) =>
        op.tool === "numbered-arrow" &&
        !old.some((before) => before.id === op.id),
    );
    const removed = old.filter(
      (op) =>
        op.tool === "numbered-arrow" &&
        !operations.some((after) => after.id === op.id),
    );
    if (added.length)
      setNumber(
        Math.min(
          Number.MAX_SAFE_INTEGER - 1,
          added[added.length - 1].number! + 1,
        ),
      );
    else if (removed.length) setNumber(removed[removed.length - 1].number!);
  }, [operations]);
  const update = (value: number) => {
    if (Number.isSafeInteger(value))
      setNumber(Math.max(1, Math.min(Number.MAX_SAFE_INTEGER - 1, value)));
  };
  return [number, update] as const;
}

export function ArrowNumberControl({
  value,
  onChange,
  disabled,
}: {
  value: number;
  onChange: (number: number) => void;
  disabled: boolean;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  return (
    <div className="arrow-number-control">
      <label htmlFor="next-arrow-number">Próximo número</label>
      <button
        type="button"
        aria-label="Diminuir próximo número"
        disabled={disabled || value <= 1}
        onClick={() => onChange(value - 1)}
      >
        −
      </button>
      <input
        id="next-arrow-number"
        aria-label="Próximo número"
        type="number"
        min="1"
        step="1"
        value={draft}
        disabled={disabled}
        onChange={(event) => {
          const text = event.target.value;
          setDraft(text);
          const next = Number(text);
          if (Number.isSafeInteger(next) && next >= 1) onChange(next);
        }}
        onBlur={() => setDraft(String(value))}
      />
      <button
        type="button"
        aria-label="Aumentar próximo número"
        disabled={disabled || value >= Number.MAX_SAFE_INTEGER - 1}
        onClick={() => onChange(value + 1)}
      >
        +
      </button>
    </div>
  );
}
