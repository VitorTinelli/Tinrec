import { describe, expect, it } from "vitest";
import {
  defaultRecordingName,
  recordingNameError,
  resolveRecordingName,
} from "./recordingName";

describe("nomes das gravações", () => {
  it("resolve o modelo padrão em pt-BR usando a data e hora locais de início", () => {
    const title = resolveRecordingName(
      defaultRecordingName,
      new Date(2026, 10, 1, 14, 5, 9),
      "Tela selecionada",
    );
    expect(title).toBe("Tela selecionada (Tela) - 01-11-2026 às 14h05m09s");
    expect(recordingNameError(defaultRecordingName, title)).toBeNull();
  });
  it("mantém nomes fixos e resolve variáveis repetidas com o mesmo valor", () => {
    expect(
      resolveRecordingName("Reunião do time", new Date(2026, 0, 2), "Tela"),
    ).toBe("Reunião do time");
    expect(
      resolveRecordingName(
        "{dia}-{mes}-{ano}-{aleatorio}-{aleatorio}",
        new Date(2026, 0, 2),
        "Tela",
        "abc123",
      ),
    ).toBe("02-01-2026-abc123-abc123");
  });
  it.each([
    ["", "Informe um nome ou modelo para o vídeo em Configurações."],
    ["   ", "Informe um nome ou modelo para o vídeo em Configurações."],
    ["{desconhecida}", "Use apenas as variáveis disponíveis abaixo."],
    ["{data", "Use apenas as variáveis disponíveis abaixo."],
    ["x".repeat(91), "O nome gerado deve ter no máximo 90 caracteres."],
    ["界".repeat(54), "O nome gerado deve ter no máximo 90 caracteres."],
    ...["...", "Reunião/cliente", ".oculto", "final."].map((template) => [
      template,
      "O nome não pode conter caracteres reservados para arquivos, como /, : ou ?.",
    ]),
  ])("rejeita o modelo %j com o motivo correto", (template, error) => {
    expect(
      recordingNameError(
        template,
        resolveRecordingName(template, new Date(2026, 0, 2), "Tela"),
      ),
    ).toBe(error);
  });
  it.each(["x".repeat(90), "界".repeat(53)])(
    "aceita o limite válido %j",
    (template) => {
      expect(recordingNameError(template, template)).toBeNull();
    },
  );
});
