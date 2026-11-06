import { useCallback, useRef, useState, type SetStateAction } from "react";
const key = "tinrec.print.thickness.v1";
export function usePrintThickness(minimum: number, maximum: number) {
  const clamp = (value: number) =>
    Math.max(minimum, Math.min(maximum, Math.round(value)));
  const [size, setValue] = useState(() => {
    try {
      const saved = localStorage.getItem(key);
      const value = saved === null ? 4 : Number(saved);
      return Number.isFinite(value) && value >= 1 ? clamp(value) : 4;
    } catch {
      return 4;
    }
  });
  const current = useRef(size);
  const setSize = useCallback(
    (value: SetStateAction<number>) => {
      const next = typeof value === "function" ? value(current.current) : value;
      if (!Number.isFinite(next)) return;
      const bounded = Math.max(minimum, Math.min(maximum, Math.round(next)));
      current.current = bounded;
      setValue(bounded);
      try {
        localStorage.setItem(key, String(bounded));
      } catch {
        /* Keep the setting for this editor if storage is unavailable. */
      }
    },
    [minimum, maximum],
  );
  return [size, setSize] as const;
}
