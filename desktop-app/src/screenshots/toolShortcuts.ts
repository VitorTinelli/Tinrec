import { useEffect, useState } from "react";
import type { Tool } from "./document";
export type PrintShortcutAction =
  Tool | "arrow-number-up" | "arrow-number-down";

export const printTools = [
  ["select", "Selecionar", "V"],
  ["pen", "Desenhar", "P"],
  ["arrow", "Seta", "A"],
  ["numbered-arrow", "Seta numerada", "Shift+A"],
  ["line", "Linha", "L"],
  ["rectangle", "Retângulo", "R"],
  ["ellipse", "Elipse", "E"],
  ["text", "Texto", "T"],
  ["callout", "Chamada", "Q"],
  ["highlight", "Destacar", "H"],
  ["fill", "Ocultar / preencher", "B"],
  ["blur", "Desfocar", "F"],
  ["crop", "Recortar", "C"],
  ["grayscale", "Cinza", "G"],
  ["invert", "Inverter cores", "I"],
  ["arrow-number-up", "Aumentar próximo número da seta", "N"],
  ["arrow-number-down", "Diminuir próximo número da seta", "Shift+N"],
] as const satisfies readonly (readonly [
  PrintShortcutAction,
  string,
  string,
])[];
export type PrintBindings = Partial<Record<PrintShortcutAction, string>>;
const storageKey = "tinrec.print.shortcuts.v1";
const changed = "tinrec-print-shortcuts-changed";
export const defaultPrintBindings: PrintBindings = Object.fromEntries(
  printTools.map(([tool, , key]) => [tool, key]),
);
export function readPrintBindings(): PrintBindings {
  try {
    const saved = JSON.parse(localStorage.getItem(storageKey) ?? "null");
    const bindings = { ...defaultPrintBindings };
    for (const [tool] of printTools) {
      if (
        typeof saved?.[tool] === "string" &&
        /^(Shift\+)?[A-Z]$|^$/.test(saved[tool])
      )
        bindings[tool] = saved[tool];
    }
    return bindings;
  } catch {
    return { ...defaultPrintBindings };
  }
}
export function validatePrintBindings(bindings: PrintBindings) {
  const used = new Set<string>();
  for (const [tool] of printTools) {
    const value = bindings[tool] ?? "";
    if (!/^(Shift\+)?[A-Z]$|^$/.test(value))
      throw new Error("Use uma letra de A a Z, com Shift opcional.");
    if (value && used.has(value))
      throw new Error(`O atalho ${value} está repetido.`);
    if (value) used.add(value);
  }
}
export function savePrintBindings(bindings: PrintBindings) {
  validatePrintBindings(bindings);
  localStorage.setItem(storageKey, JSON.stringify(bindings));
  window.dispatchEvent(new Event(changed));
}
export function usePrintBindings() {
  const [bindings, setBindings] = useState(readPrintBindings);
  useEffect(() => {
    const update = () => setBindings(readPrintBindings());
    window.addEventListener("storage", update);
    window.addEventListener(changed, update);
    window.addEventListener("focus", update);
    return () => {
      window.removeEventListener("storage", update);
      window.removeEventListener(changed, update);
      window.removeEventListener("focus", update);
    };
  }, []);
  return bindings;
}
export function printToolForKey(
  event: KeyboardEvent,
  bindings: PrintBindings,
): PrintShortcutAction | undefined {
  if (
    event.repeat ||
    event.isComposing ||
    event.ctrlKey ||
    event.metaKey ||
    event.altKey
  )
    return;
  if (
    event.target instanceof Element &&
    event.target.closest("input,textarea,select,[contenteditable=true]")
  )
    return;
  if (!/^[a-z]$/i.test(event.key)) return;
  const key = `${event.shiftKey ? "Shift+" : ""}${event.key.toUpperCase()}`;
  return printTools.find(([tool]) => bindings[tool] === key)?.[0];
}
