# Testes

| Camada                        | Comando                                                                                                                                             | Contrato verificado                                                                                                                                                         |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Site, API e interface desktop | `npm test`                                                                                                                                          | Autorização, compartilhamento, multipart, retenção, reprodução e comandos da interface. Cada teste de API usa um PostgreSQL PGlite próprio.                                 |
| Motor de captura              | `cargo test --locked --manifest-path desktop-rust/Cargo.toml -- --include-ignored --nocapture --test-threads=1`                                     | MP4, trilhas de áudio, pausas, mute, recorte, webcam, falha do encoder e proteção contra sobrescrita. Requer FFmpeg/libx264 e ffprobe.                                      |
| Backend Tauri                 | `cargo test --locked --manifest-path desktop-app/src-tauri/Cargo.toml --features custom-protocol -- --include-ignored --nocapture --test-threads=1` | Fila persistente, isolamento de contas, retomada, descarte, atalhos e miniaturas. Requer dependências nativas, FFmpeg e frontend compilado com `npm run desktop:web:build`. |
| Interface nativa Linux        | `python3 scripts/test-desktop.py --controls` e `python3 scripts/test-desktop.py --background`                                                       | Fluxo WebDriver → IPC → motor → arquivo; janela flutuante, descarte e ciclo de vida na bandeja. Requer binário debug, tauri-driver, WebKitWebDriver e sessão gráfica/D-Bus. |
| Fila Windows WPF              | `dotnet run --project desktop/Tinrec.Tests/Tinrec.Tests.csproj -p:Platform=x64`                                                                     | DPAPI, retomada, sessão expirada, backoff e retenção da implementação WPF. Requer Windows e SDK .NET 8.                                                                     |

A CI executa JavaScript uma vez no workflow `Application tests`. O motor Rust é
validado nos três sistemas no workflow `Rust capture prototype`; o workflow
`Tinrec desktop` valida o backend Tauri e a interface nativa Linux. O instalador
Windows só empacota. A fila WPF tem um workflow próprio e continua coberta porque
é uma implementação diferente da fila Rust.

## Critérios da revisão

- Os testes de controles flutuantes foram consolidados: cada comando e sua
  confirmação são verificados no mesmo fluxo. Foram removidas as verificações de
  elementos de layouts antigos e as repetições de proteção contra sobrescrita para
  cada combinação de áudio.
- Casos com entradas diferentes usam tabelas nomeadas quando isso melhora o
  diagnóstico. Mais casos nomeados não significam mais cenários redundantes.
- A criptografia é testada sem inicializar banco de dados. API e armazenamento
  temporário compartilham a preparação, mas nunca a instância de banco.
- Asserções verificam comandos, ordem, valores e arquivos; títulos de testes não
  prometem validar hardware, imagens reais ou credenciais quando há mocks.
- Esperas de transição têm prazo total. Os servidores HTTP simulados falham em
  EOF e têm limites de espera; a CI também limita os testes Rust a dez minutos.
- Autorização, isolamento de contas, retomada e integridade de mídia foram
  preservados: testar essas propriedades em camadas distintas protege integrações
  diferentes.

Os testes de captura usam fontes geradas. `--native` no script Linux habilita
captura real e requer consentimento do sistema; testes sintéticos aprovados não
comprovam captura física, permissões nem desempenho sustentado em Windows/macOS.
