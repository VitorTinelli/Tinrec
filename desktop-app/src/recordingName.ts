import type { Settings, Sources } from "./bridge";

export const defaultRecordingName = "{fonte} ({tipo}) - {data} às {hora}";
export const nameVariables = [
  ["fonte", "Nome da fonte"],
  ["tipo", "Tipo de captura"],
  ["data", "Data: DD-MM-AAAA"],
  ["hora", "Hora: HHhMMmSSs"],
  ["dia", "Dia"],
  ["mes", "Mês"],
  ["ano", "Ano"],
  ["aleatorio", "Código de 6 caracteres"],
] as const;

export function resolveRecordingName(
  template: string,
  date: Date,
  source: string,
  random = "a1b2c3",
  kind = "Tela",
): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  const sourceText =
    source.replace(/[<>:"/\\|?*{}\u0000-\u001f\u007f]/g, "").trim() ||
    "Fonte selecionada";
  let sourceName = "";
  for (const char of sourceText) {
    if (
      Array.from(sourceName).length >= 40 ||
      new TextEncoder().encode(sourceName + char).length > 77
    )
      break;
    sourceName += char;
  }
  if (sourceName !== sourceText) sourceName += "…";
  const values: Record<string, string> = {
    fonte: sourceName,
    tipo: kind,
    dia: pad(date.getDate()),
    mes: pad(date.getMonth() + 1),
    ano: String(date.getFullYear()),
    data: `${pad(date.getDate())}-${pad(date.getMonth() + 1)}-${date.getFullYear()}`,
    hora: `${pad(date.getHours())}h${pad(date.getMinutes())}m${pad(date.getSeconds())}s`,
    aleatorio: random,
  };
  return template
    .replace(/\{([^{}]+)\}/g, (token, variable: string) =>
      Object.hasOwn(values, variable) ? values[variable] : token,
    )
    .trim();
}
export function recordingSource(
  settings: Settings,
  sources: Sources | null,
): string {
  if (settings.captureMode === "window")
    return sources?.wayland
      ? "Janela selecionada"
      : (sources?.windows?.find((window) => window.id === settings.window)
          ?.name ?? "Janela selecionada");
  if (settings.captureMode === "area") return "Área selecionada";
  if (sources?.wayland) return "Tela selecionada";
  return (
    sources?.displays.find((display) => display.id === settings.display)
      ?.name ?? "Tela principal"
  );
}
export function recordingNameError(
  template: string,
  resolved: string,
): string | null {
  if (!template.trim())
    return "Informe um nome ou modelo para o vídeo em Configurações.";
  if (/[{}]/.test(resolved))
    return "Use apenas as variáveis disponíveis abaixo.";
  if (resolved.length > 90 || new TextEncoder().encode(resolved).length > 160)
    return "O nome gerado deve ter no máximo 90 caracteres.";
  if (
    /[<>:"/\\|?*\u0000-\u001f\u007f]/.test(resolved) ||
    !resolved.replace(/[.\s]/g, "") ||
    resolved.startsWith(".") ||
    resolved.endsWith(".")
  )
    return "O nome não pode conter caracteres reservados para arquivos, como /, : ou ?.";
  return null;
}

export const captureType = (mode: string) =>
  ({ screen: "Tela", window: "Janela", area: "Área", camera: "Câmera" })[
    mode
  ] ?? "Tela";
