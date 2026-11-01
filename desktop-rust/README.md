# Protótipo de captura Tinrec em Rust

Motor compartilhado de captura do Tinrec: tela, microfone, áudio do sistema,
pausas, encoder e diagnóstico. A [interface Tauri](../desktop-app/README.md) já
utiliza a biblioteca; esta pasta também mantém a CLI de diagnóstico.

Este executável é uma ferramenta de diagnóstico local. A CLI não tem interface
gráfica; vínculo de conta, fila e upload ainda não foram portados para o Rust.
O site e a API TypeScript continuam sendo aproveitados pela arquitetura proposta.

## Arquitetura

- `pinray` **0.2.7**, versão fixada e dependências registradas em `Cargo.lock`:
  ScreenCaptureKit no macOS, DXGI/WGC e WASAPI no Windows, portal/PipeWire no
  Wayland e captura por polling no X11.
- `cpal` 0.16: microfone. A entrada é convertida para mono pela média dos canais,
  para que interfaces com apenas um canal ativo não enviem a voz a um único lado
  do fone. O áudio do sistema mantém seus canais. O microfone da API `pinray`
  ainda não é implementado.
- Transporte Matroska em streaming com timestamps explícitos, BGRA sem padding e
  PCM float. O transporte só passa pelo pipe; não grava vídeo bruto no disco.
- FFmpeg externo: H.264 por software (`libx264`) e AAC. Com dois áudios, o MP4
  contém mixagem como primeira trilha e as fontes separadas como segunda e terceira.
- Relógio nativo compartilhado para vídeo/áudio do sistema. O microfone usa o
  relógio monotônico do host, descontando o atraso de callback disponível no CPAL.
  A sincronização entre APIs precisa de teste perceptivo em cada plataforma.
- Fila de encoder limitada a 256 pacotes, com no máximo quatro quadros de vídeo
  pendentes; fila do microfone limitada a 32 callbacks. Vídeo pode ser descartado
  sob pressão e é contado. Sobrecarga de áudio encerra com erro explícito.

O backend é **experimental**. Sua API nativa é encapsulada em `recording.rs`, para
permitir substituição sem vincular a futura interface ao fornecedor de captura.
Não há promessa de zero-copy nem aceleração de hardware nesta fase.

Referências: [pinray](https://github.com/Itz-Agasta/pinray),
[CPAL](https://github.com/RustAudio/cpal),
[Matroska](https://www.matroska.org/technical/elements.html),
[FFmpeg](https://ffmpeg.org/ffmpeg.html).

## Preparação

Rust estável (mínimo declarado: 1.88), FFmpeg 6+ com `libx264` e AAC, e `ffprobe` no
PATH. O protótipo **não inclui FFmpeg no binário**. A distribuição final terá que
definir empacotamento e cumprir as licenças das bibliotecas e do build de FFmpeg.

Linux Debian/Ubuntu:

```sh
sudo apt-get install build-essential pkg-config clang libpipewire-0.3-dev libasound2-dev ffmpeg
```

Linux Arch/CachyOS:

```sh
sudo pacman -S --needed base-devel pkgconf clang pipewire alsa-lib ffmpeg
```

Wayland exige uma sessão PipeWire e um backend XDG Desktop Portal compatível com
o compositor. A seleção de tela é feita pelo diálogo do sistema; `--display` não
força a seleção no Wayland. Para testar X11, use uma sessão **X11 real**. Capturar
o servidor XWayland não comprova captura do desktop Wayland.

Windows: Rust com toolchain MSVC e Visual Studio Build Tools (C++), FFmpeg e
ffprobe no PATH. Testar tanto DXGI como WGC, inclusive múltiplos monitores/DPI.

macOS: macOS 13+, Xcode Command Line Tools, Rust e `brew install ffmpeg`.
Autorize Gravação de Tela/Áudio do Sistema e Microfone para o processo/terminal
conforme solicitado. Aplicativo assinado e permissões da futura distribuição
Tauri ainda precisam ser definidos e validados.

## Compilar e executar

Na raiz do repositório:

```sh
cargo build --locked --release --manifest-path desktop-rust/Cargo.toml
cargo run --locked --release --manifest-path desktop-rust/Cargo.toml -- doctor
cargo run --locked --release --manifest-path desktop-rust/Cargo.toml -- sources
```

Teste sem hardware, com padrão em movimento e tons distintos de 440/880 Hz:

```sh
cargo run --locked --release --manifest-path desktop-rust/Cargo.toml -- synthetic \
  --output artifacts/rust/synthetic.mp4 --duration 10 --system-audio --microphone \
  --pause-after 3 --pause-for 2
```

Gravação real curta:

```sh
cargo run --locked --release --manifest-path desktop-rust/Cargo.toml -- record \
  --output artifacts/rust/native.mp4 --duration 10 --system-audio --microphone
```

Use os mesmos argumentos em uma linha no PowerShell. O executável compilado fica
em `desktop-rust/target/release/tinrec-capture` (Windows: `.exe`).

Durante a gravação, envie `p` + Enter para pausar, `r` + Enter para retomar e `q`
+ Enter ou Ctrl+C para finalizar. Pausas são removidas de todas as trilhas por
tempo de captura, inclusive em pacotes de áudio que atravessam uma pausa.
`--duration` mede tempo ativo e aceita até 7200 segundos; não há envio de arquivos.

Opções adicionais:

| Opção | Comportamento |
| --- | --- |
| `--resolution 720` / `1080` | Altura de saída; largura acompanha a proporção da tela |
| `--microphone-device "nome exato"` | Dispositivo listado por `sources`; requer `--microphone` |
| `--display ID` | Display nativo fora do Wayland; padrão `auto` |
| `--backend auto/wayland/x11/wgc/dxgi` | Força um backend ou retorna erro explícito |
| `--ffmpeg CAMINHO` / `--ffprobe CAMINHO` | Usa ferramentas de um local específico |
| `--pause-after N --pause-for N` | Pausa automática para reproduzir o diagnóstico |

Sem flags de áudio, grava só vídeo. Qualquer fonte solicitada precisa funcionar:
não há fallback silencioso para uma gravação sem áudio. Mudança de resolução ou
formato durante a captura encerra com erro nesta versão. Ainda não há seleção de
área/janela na CLI.

## Arquivos e diagnóstico

Para `native.mp4`, são produzidos:

- `native.mp4`: publicação final após FFmpeg concluir com sucesso.
- `native.mp4.capture.json`: resultado, contagens, picos de áudio, verificações e
  amostras de RSS/CPU do Rust **e** do FFmpeg a cada segundo.
- `native.mp4.ffprobe.json`: metadados e contagem de quadros decodificados.
- `native.mp4.ffmpeg.log`: diagnósticos do encoder.
- `native.mp4.partial.mp4`: permanece caso o encoder falhe/interrompa; não é
  promovido automaticamente a gravação completa.

Destinos e relatórios existentes são recusados. Escolha um nome novo para repetir.
Mesmo quando um arquivo é finalizado, consulte `status` e `checks.passed` no
relatório: falhas posteriores de inspeção mantêm o MP4 e resultam em saída não zero.

`actual_file_fps` é calculado pelos quadros contados pelo ffprobe divididos pela
duração do vídeo. Não é FPS de captura sem repetições: quadros repetidos para
preservar cenas estáticas são contados separadamente no relatório. O teste precisa
de conteúdo em movimento para avaliar fluidez. Pausas, quadros recebidos, quadros
enviados, perdas nativas e descartes do encoder não são a mesma medida.

RSS é memória residente e inclui processos separados; não equivale à memória
privada medida no diagnóstico antigo do Windows. CPU usa 100% para um núcleo
lógico, podendo passar de 100%; a primeira amostra de CPU é uma referência inicial.

As checagens automáticas exigem vídeo decodificável, duração final a até 250 ms do
tempo ativo, diferença entre finais de áudio/vídeo a até 250 ms e a quantidade
esperada de trilhas. Isso **não** comprova audibilidade, sincronização perceptiva,
estabilidade de memória ou desempenho sustentado.

## Homologação de 30–60 minutos

```sh
cargo run --locked --release --manifest-path desktop-rust/Cargo.toml -- record \
  --output artifacts/rust/native-30min.mp4 --duration 1800 --resolution 1080 \
  --system-audio --microphone --pause-after 60 --pause-for 2
```

Em **cada** plataforma/sessão:

1. Confirmar permissão e o monitor escolhido no diálogo, se aplicável.
2. Mostrar padrão em movimento contínuo e reproduzir tom no áudio do sistema.
3. Fazer marcadores visuais/sonoros audíveis no microfone no início e no fim.
4. Testar as quatro combinações de áudio, pausa, retomada, encerramento e dispositivo
   ausente. Testar dispositivos alternativos e monitores com escalas diferentes.
5. Conferir RSS dos dois processos após aquecimento e ao longo do ensaio, perdas,
   FPS efetivo e sincronização inicial/final. Investigar crescimento continuado;
   um teste curto não aprova estabilidade de memória.
6. Reproduzir a mixagem e as fontes separadas; não inferir som da existência de AAC.

Não comparar os números desta implementação com o desktop antigo como benchmark
equivalente: captura, codec, carga, resolução e definição de memória podem diferir.

## Testes e CI

```sh
cargo fmt --manifest-path desktop-rust/Cargo.toml --check
cargo clippy --locked --manifest-path desktop-rust/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path desktop-rust/Cargo.toml -- --include-ignored
```

Os testes de integração, ignorados por padrão por exigirem FFmpeg, exercitam as
quatro combinações de áudio, pausa, decodificação, presença dos dois tons na
mixagem, separação das fontes, recusa de sobrescrita e falha do encoder.
Não pedem acesso à tela/microfone.

O workflow `.github/workflows/rust-capture.yml` prepara build, lint e testes
sintéticos para Linux, Windows e macOS. A existência do workflow não significa
que já foi executado no GitHub, nem substitui testes nativos em hardware real.

Veja [VALIDATION.md](VALIDATION.md) para o que foi realmente executado.

## Integração com o desktop

O motor é exportado por `src/lib.rs`. `recording::run_controlled` aceita um canal
de comandos (`Pause`, `Resume`, `Stop`) e publica progresso para o desktop. Não
instala handlers globais de sinais ou stdin; a CLI faz isso em seu próprio caminho.
Volumetria de áudio é configurável antes de iniciar, por `--system-volume` e
`--microphone-volume` (0–100%). O relógio do microfone usa uma âncora do dispositivo
com continuidade por amostras, para não tratar rajadas de callbacks como retrocesso.

Próximas etapas: homologar captura real nos outros sistemas e portar vínculo,
fila persistente e upload com retomada. Credenciais precisarão de armazenamento
seguro específico de cada sistema em lugar de DPAPI exclusivo do Windows.

## Alvos de captura

`--capture-mode screen|window|area|camera` seleciona o alvo. O padrão permanece
screen. `--display` recebe o identificador da tela, janela ou câmera. Para área
pela CLI, use `--crop x,y,largura,altura`; a GUI oferece seleção interativa.
O retângulo é validado contra a fonte e aplicado antes da codificação. Câmera
usa FFmpeg V4L2/DirectShow/AVFoundation e preserva o motor de pausa, áudio e MP4.
Relógios independentes de câmera/áudio são alinhados ao host e exigem validação
perceptiva. Ver `vendor/pinray-platform-linux/README.tinrec.md` para o ajuste do
portal Wayland que restringe os tipos de fonte.
