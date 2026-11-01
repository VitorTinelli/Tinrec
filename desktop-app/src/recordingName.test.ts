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
    expect(resolveRecordingName("Reunião do time", new Date(), "Tela")).toBe(
      "Reunião do time",
    );
    expect(
      resolveRecordingName(
        "{dia}-{mes}-{ano}-{aleatorio}-{aleatorio}",
        new Date(2026, 0, 2),
        "Tela",
        "abc123",
      ),
    ).toBe("02-01-2026-abc123-abc123");
  });
  it("rejeita nomes vazios, variáveis desconhecidas e nomes incompatíveis com arquivos", () => {
    for (const template of [
      "",
      "   ",
      "...",
      "{desconhecida}",
      "{data",
      "Reunião/cliente",
      "x".repeat(91),
    ]) {
      expect(
        recordingNameError(
          template,
          resolveRecordingName(template, new Date(), "Tela"),
        ),
      ).not.toBeNull();
    }
  });
});
