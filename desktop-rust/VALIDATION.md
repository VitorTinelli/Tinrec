# Validação do protótipo Rust — 31 de outubro de 2026

## Escopo

Esta entrega é a prova de captura anterior à interface Tauri. Não é a migração
completa do aplicativo, não realiza upload e não altera o backend existente.

Ambiente de execução real: Linux x86_64, CachyOS, sessão Wayland; Rust 1.99.1;
FFmpeg/ffprobe n9.0.2. Backends: portal XDG/PipeWire para vídeo e áudio do sistema,
CPAL para microfone. Captura nativa de display 2560×1440.

## Verificações executadas

- Build Linux em release concluído.
- `cargo fmt --check` e Clippy com warnings como erro.
- Testes unitários de corte de pausas, inclusive pacotes atrasados e áudio que
  atravessa a pausa, e normalização/intercalação de áudio planar estéreo.
- Testes de integração com FFmpeg real: quatro combinações de áudio, pausa
  automática, decodificação, presença dos dois tons na mixagem, separação das
  fontes, caminhos com espaços/acentos, recusa de sobrescrita, erro do encoder e
  comandos interativos de pausa/retomada/encerramento.
- `cargo check --target x86_64-pc-windows-gnu` concluído.
- `cargo check --target aarch64-apple-darwin` concluído.

As duas últimas verificações checam compilação/tipos dos backends a partir do
Linux. **Não produzem instaladores nem comprovam execução, permissões ou captura
em hardware Windows/macOS.** O workflow para executar build e testes sintéticos
nativamente nesses sistemas foi preparado, mas não foi disparado no GitHub.

## Gravação sintética

Padrão BGRA em movimento, tons de 440 Hz e 880 Hz, saída 720p. Três segundos
ativos mais uma pausa de um segundo:

| Medida | Resultado |
| --- | --- |
| Tempo de relógio | 4,001 s |
| Duração do vídeo | 3,033 s |
| Quadros contados no MP4 | 91 |
| FPS calculado do arquivo | 30,00 |
| Maior diferença entre finais de áudio/vídeo | 33,625 ms |
| Trilhas de áudio | Mixagem + sistema + microfone |

Esse ensaio valida transporte, encoder e timestamps, sem capturar hardware.
A imagem gerada foi renderizada e inspecionada visualmente. A separação dos tons
é verificada por análise de amplitude no áudio decodificado nos testes.

## Captura real curta em Linux Wayland

Tela + áudio do sistema + microfone, saída 720p, dez segundos ativos com pausa
de dois segundos:

| Medida | Resultado |
| --- | --- |
| Tempo de relógio | 12,007 s |
| Duração do vídeo | 9,957 s |
| Quadros contados no MP4 | 300 |
| FPS calculado do arquivo | 30,13 |
| Maior diferença entre finais de áudio/vídeo | 4,792 ms |
| Perdas nativas / descartes do encoder reportados | 0 / 0 |
| Pico de amostra do áudio do sistema | 0,99997 |
| Pico de amostra do microfone | 0,03121 |
| RSS Rust após aquecimento | aproximadamente 112 MiB |
| RSS FFmpeg no fim da captura | aproximadamente 208 MiB |

As três trilhas foram produzidas e as verificações técnicas passaram. Picos de
amostra distintos de zero comprovam dados de áudio, não inteligibilidade ou
sincronização perceptiva. FPS calculado inclui o quadro final repetido; não é uma
garantia de fluidez em outras máquinas ou sessões.

Uma primeira tentativa real detectou sobrecarga na fila compartilhada durante a
inicialização de FFmpeg. O processo retornou erro explícito e preservou os
artefatos de diagnóstico. A implementação foi corrigida para limitar quatro
quadros de vídeo separadamente e acomodar rajadas de áudio em uma fila limitada
a 256 pacotes; a gravação acima usa essa correção.

Artefatos locais, ignorados pelo Git:

- `artifacts/rust/linux-wayland-10s.capture.json`
- `artifacts/rust/linux-wayland-10s.ffprobe.json`
- `artifacts/rust/synthetic-3s.capture.json`
- `artifacts/rust/tinrec-capture-linux-x64`

## Ensaio prolongado

Gravação de 30 minutos em 1080p, áudio do sistema e microfone, pausa automática
de dois segundos. O MP4 foi finalizado e a inspeção por decodificação concluiu
com `status: completed` e `checks.passed: true`.

| Medida | Resultado |
| --- | --- |
| Tempo ativo / de relógio | 1.800,000 / 1.802,011 s |
| Duração do vídeo | 1.799,944 s |
| Codec / resolução de vídeo | H.264, 1920×1080 |
| Quadros decodificados no MP4 | 53.992 |
| FPS calculado do arquivo | 29,9965 |
| Maior diferença entre finais de áudio/vídeo | 1,042 ms |
| Diferença entre fim do vídeo e tempo ativo | 56 ms |
| Trilhas de áudio | AAC: mixagem, sistema e entrada padrão |
| Perdas nativas / descartes do encoder reportados | 0 / 0 |
| Quadros repetidos pela aplicação | 1 (finalização) |
| Amostras de diagnóstico | 1.796 |
| RSS Rust: média entre minutos 1–6 / últimos 5 minutos | 123,26 / 126,13 MiB |
| RSS Rust: pico / última amostra | 168,29 / 111,79 MiB |
| RSS FFmpeg: média entre minutos 1–6 / últimos 5 minutos | 338,05 / 354,57 MiB |
| RSS FFmpeg: pico / última amostra | 376,39 / 368,98 MiB |
| CPU média conjunta após 60 s, 100% = um núcleo lógico | 70,99% |

O tempo de pausa foi removido das trilhas. A série de RSS do Rust ficou próxima
de 126 MiB no trecho final e caiu após liberar a captura. FFmpeg apresentou
crescimento moderado ao longo do ensaio: aproximadamente 16,5 MiB entre as médias
das janelas acima. Isso não demonstra ausência de vazamentos, nem permite
comparação direta com a memória privada do desktop antigo. Os processos de
serviço do sistema e a memória da GPU não estão incluídos nessas medidas.

Os picos brutos de áudio chegaram a 1,170 no sistema e 1,160 na entrada padrão,
acima da faixa nominal ±1 do PCM float. Ganho, possível saturação e necessidade
de limitador precisam de avaliação antes da homologação de qualidade do áudio.
Não foi feita avaliação auditiva da inteligibilidade nem por marcadores de
sincronização; proximidade das durações não comprova sincronização perceptiva.

O ensaio usa conteúdo da sessão real, sem padrão controlado de movimento
contínuo. O resultado é evidência favorável de viabilidade no Linux testado,
sem constituir homologação completa de desempenho ou das três plataformas.

Artefatos locais:

- `artifacts/rust/linux-wayland-30min.mp4`
- `artifacts/rust/linux-wayland-30min.mp4.capture.json`
- `artifacts/rust/linux-wayland-30min.mp4.ffprobe.json`
- `artifacts/rust/linux-wayland-30min.summary.json`

O binário exato do ensaio foi preservado em
`artifacts/rust/tinrec-capture-linux-x64-benchmark`, SHA-256
`c164ff08a6133592c56d5da38196d18981856f8fdfc5c45db2c5a9d37180329a`.
A CLI final em `artifacts/rust/tinrec-capture-linux-x64` também concluiu um teste
sintético de dois segundos com as duas fontes, três trilhas de áudio e checagens
técnicas aprovadas (`final-build-smoke.mp4.capture.json` na mesma pasta).

## Limites restantes

Correção posterior ao ensaio prolongado: a gravação `teste-audio.mp4` revelou
microfone estéreo com sinal apenas à esquerda e canal direito zerado. O caminho
CPAL passou a converter os canais de entrada para uma trilha mono, centralizando
a voz na reprodução e na mixagem, sem alterar o estéreo do áudio do sistema.
Testes cobrem entrada ativa só à esquerda, só à direita, mono e estéreo idêntico,
e verificam que o MP4 gerado contém microfone mono. O ensaio de 30 minutos acima
antecede essa correção; não foi repetido para esta mudança de canais.

- Falta execução real e ensaio prolongado em Windows, macOS e Linux X11.
- Não foi reproduzido um padrão controlado em movimento contínuo no desktop
  nativo, nem feita validação perceptiva por marcadores de sincronização.
- Seleção de área/janela, múltiplos monitores/DPI, reconexão de dispositivos,
  codecs por hardware, interface Tauri e instaladores não integram esta prova.
- FFmpeg é uma dependência externa de execução. O empacotamento final precisa
  incluir ou localizar uma distribuição compatível.
- Ainda não há base para afirmar homologação dos três sistemas nem substituir
  o desktop Windows existente.
