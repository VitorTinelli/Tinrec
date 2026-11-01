# Tinrec desktop — Rust + Tauri

Gravador com interface React, motor Rust compartilhado com a CLI e upload direto para o Drive de cada usuário. O aplicativo WPF existente foi preservado.

## Abrir no Linux deste ambiente

O executável foi gerado em `artifacts/Tinrec-linux-x64` e já inclui a interface.
Não é necessário iniciar servidor web, Node.js ou a CLI de captura para usá-lo.

```sh
cd /home/vitortinelli/Documentos/Github/Tinrec
./artifacts/Tinrec-linux-x64
```

Esse binário foi compilado no CachyOS x86_64 atual. Ele depende das bibliotecas do
sistema e não é um pacote portátil para todas as distribuições. A configuração
de CI usa Ubuntu 24.04 para preparar um pacote `.deb` com uma base mais ampla.

Requisitos de execução Linux: GTK 3, WebKitGTK 4.1, ALSA, PipeWire e FFmpeg 6+
com `libx264`/AAC e `ffprobe` no PATH. No Wayland, é necessário o backend XDG
Desktop Portal correspondente ao compositor.

O aplicativo verifica as ferramentas ao abrir. Se faltarem, explica o que
instalar e mantém o botão de gravação desabilitado. No CachyOS deste ambiente,
essas dependências já estão disponíveis.

## Usar

1. Defina um nome opcional, a qualidade e as fontes de áudio. O microfone é mono
   para reproduzir a voz nos dois lados; o áudio do computador preserva seus canais.
2. Ajuste os volumes antes de iniciar; 80% é o padrão. Fontes e volumes ficam
   bloqueados durante a gravação para evitar mudanças de formato.
3. Clique em **Iniciar gravação**. No Wayland, escolha a tela no diálogo do sistema.
   A escolha do dropdown não substitui a seleção pelo portal. Nos outros backends,
   escolha o monitor listado.
4. Use **Pausar**, **Retomar** e **Encerrar**. O contador vem do motor Rust e
   exclui as pausas. Cada gravação tem limite de duas horas de tempo ativo.
5. Aguarde **Vídeo salvo**. O arquivo aparece em **Gravações recentes**, com
   duração, data e tamanho. **Abrir** usa o player padrão do sistema.

O caminho exibido no cartão permite escolher outra pasta pelo seletor nativo.
**Abrir pasta** usa o gerenciador de arquivos padrão. A pasta inicial é
`Tinrec` dentro de Vídeos, Documentos ou, na ausência desses, a pasta do usuário.
Até 100 arquivos recentes da pasta atual são exibidos.

O **X** oculta a janela e mantém o aplicativo na bandeja: gravação, finalização e
uploads continuam. Para reabrir, clique no ícone Tinrec ou use **Abrir Tinrec** no
menu da bandeja. Abrir novamente o executável também traz a janela existente.
**Sair do Tinrec**, na bandeja ou no rodapé, encerra o processo de verdade. Se
houver uma gravação, ele solicita Stop e aguarda o MP4 ser finalizado. Se ainda
houver um diálogo de permissão aberto, aprove ou cancele esse diálogo para liberar
a inicialização. Esta opção não configura início automático junto com o sistema.

Erros de captura ou de verificação aparecem na interface. Arquivos parciais e
relatórios são preservados para diagnóstico; arquivos `.partial.mp4` não aparecem
como vídeos concluídos na lista. Arquivos antigos nunca são sobrescritos.

Preferências de áudio/qualidade ficam no armazenamento local da interface; a
pasta escolhida fica em `desktop.json` na pasta de configuração do aplicativo.
Não há credenciais, acesso ao Google ou envio automático nesta versão.

## Desenvolvimento

Na raiz do repositório, com Node.js 22+ e Rust estável (mínimo 1.88):

```sh
npm ci
npm run desktop:dev
```

O comando inicia Vite somente para desenvolvimento e abre a janela Tauri.
A página web isolada não tem acesso a gravação/arquivos e explica que é preciso
abrir o aplicativo nativo.

Dependências de build no Ubuntu/Debian:

```sh
sudo apt-get install build-essential pkg-config clang libwebkit2gtk-4.1-dev \
  libpipewire-0.3-dev libasound2-dev librsvg2-dev ffmpeg
```

No Arch/CachyOS, os pacotes correspondentes incluem `base-devel`, `pkgconf`,
`clang`, `webkit2gtk-4.1`, `pipewire`, `alsa-lib` e `ffmpeg`.

Windows: Visual Studio Build Tools C++/SDK, Rust MSVC e WebView2, além de FFmpeg
e ffprobe no PATH ou ao lado do executável. macOS: Xcode Command Line Tools,
Rust, macOS 13+ e `brew install ffmpeg`; autorize Tela/Áudio do Sistema e Microfone.
A configuração do bundle inclui a descrição de uso do microfone no Info.plist.

Para usar outro build de FFmpeg, as variáveis locais `TINREC_FFMPEG` e
`TINREC_FFPROBE` aceitam caminhos completos. O app também procura ferramentas ao
lado do próprio executável e em `bin` nos recursos do bundle. No macOS, procura
adicionalmente `/opt/homebrew/bin` e `/usr/local/bin`, pois apps abertos pelo
Finder podem não receber o PATH do shell. FFmpeg ainda não é incluído nos pacotes.

## Compilar

```sh
npm run desktop:build          # executável release, interface embutida
npm run desktop:package        # pacotes nativos do sistema atual
```

O executável fica em `desktop-app/src-tauri/target/release/tinrec-desktop`
(Windows: `.exe`). Pacotes ficam em `target/release/bundle`.

Para gerar um formato específico:

```sh
npm run desktop:package -- --bundles deb    # Linux
npm run desktop:package -- --bundles nsis   # Windows
npm run desktop:package -- --bundles app    # macOS
```

Os pacotes devem ser construídos no sistema correspondente. O workflow
`.github/workflows/desktop.yml` prepara esses builds e os testes nas três
plataformas. Ele não foi executado no GitHub nesta sessão. Os pacotes são de teste;
assinatura de editor/notarização e distribuição de FFmpeg ainda estão pendentes.

## Verificar

```sh
npx vitest run desktop-app/src/App.test.tsx
cargo test --manifest-path desktop-rust/Cargo.toml -- --include-ignored
cargo test --manifest-path desktop-app/src-tauri/Cargo.toml --features custom-protocol
cargo clippy --manifest-path desktop-app/src-tauri/Cargo.toml --all-targets --features custom-protocol -- -D warnings
```

Existe também um teste da **janela nativa**, sem mock de IPC nem do encoder.
No Linux, instale `tauri-driver` e `WebKitWebDriver` (Ubuntu: `webkit2gtk-driver`):

```sh
cargo install tauri-driver --locked
npm run desktop:web:build
cargo build --manifest-path desktop-app/src-tauri/Cargo.toml --features custom-protocol
python3 scripts/test-desktop.py
```

O script usa um binário debug, uma fonte sintética habilitada somente pelo
ambiente do processo de teste, e uma pasta temporária. Ele verifica duas sessões
seguidas: clique em iniciar → pausa → retomada → fim → item na lista → MP4
decodificado. Salva screenshots e relatórios em `artifacts/desktop-tests`.
O modo sintético não pode ser ativado pela interface nem em um build release.

Para testar hardware real explicitamente, use `--native` e aprove o diálogo do
sistema. Durante esse teste curto, fale e reproduza áudio se quiser verificar
as duas fontes. `--driver`, `--native-driver`, `--port` e `--artifacts` permitem
configurar as ferramentas locais. Os vídeos temporários desse teste são removidos
ao sair; relatórios e screenshots ficam na pasta de artefatos.

No CI Linux, esse teste usa Xvfb e fonte gerada. Ele não homologa hardware,
Wayland, Windows ou macOS. Veja [VALIDATION.md](VALIDATION.md) para a evidência
realmente coletada nesta sessão.

## Arquitetura

- `src`: interface e bridge tipada de comandos/eventos Tauri. Revisões monotônicas
  impedem que uma resposta atrasada reverta um estado mais recente.
- `src-tauri`: estado de uma sessão, descoberta de dispositivos/ferramentas,
  seleção de pasta, persistência da pasta e abertura restrita aos vídeos locais.
- `../desktop-rust`: biblioteca de captura compartilhada. A interface usa canal
  de comandos e callbacks; stdin e Ctrl+C permanecem exclusivos da CLI.
- Captura, encoder e inspeção rodam fora da thread da interface.

Referências de implementação: [comandos Tauri](https://v2.tauri.app/develop/calling-rust/),
[estado](https://v2.tauri.app/develop/state-management/),
[WebDriver](https://tauri.app/develop/tests/webdriver/manual-setup/).


## Drive e compartilhamento

O painel **Seu Drive, seu link** conecta o desktop a `https://tinrec.vercel.app`.
Clique em **Conectar Google**, confira o código no navegador e aprove a conexão
com a conta desejada. A sessão do desktop dura até 90 dias e fica no chaveiro
nativo (Secret Service no Linux, Credential Manager no Windows e Keychain no macOS).
Não há fallback para guardar tokens em texto puro. No Linux, mantenha um serviço
compatível com Secret Service disponível e desbloqueado.

Com a conta conectada, novas gravações entram automaticamente na fila. O site
reserva o link durante a captura; **Copiar link** funciona assim que essa reserva
é concluída. O vídeo será reproduzível após finalizar, enviar e o Google processar
o MP4. Qualquer pessoa com o link pode assistir. O arquivo local é preservado.

O envio usa blocos de 8 MiB e retoma a partir do offset confirmado pelo Drive.
Usar **Sair do Tinrec** pausa os envios; reabrir retoma a fila persistida em
`uploads.json` na pasta de configuração. Essa fila guarda metadados, não tokens
nem URLs secretas de upload. Trocar de conta pausa os trabalhos da conta anterior.
Use **Enviar ao Drive** para gravações anteriores concluídas ou **Tentar novamente**
para antecipar uma nova tentativa após corrigir um erro. A gravação funciona sem
conectar uma conta e nenhum vídeo antigo é enviado sem ação explícita.

O desktop confere metadados do MP4 ao finalizar, sem decodificar a gravação inteira.
Para contagem de quadros por decodificação, use a CLI sem `--fast-finalize`.

### Reprodução imediata com R2

Com o servidor configurado para R2, o desktop envia primeiro a cópia temporária
e depois a cópia do Drive. O player do site fica disponível ao concluir o primeiro
envio. A fila retoma cada estágio consultando os blocos confirmados no servidor.
Para uma gravação anterior já enviada, clique em **Preparar reprodução rápida**.

O dono confirma no site que assistiu ao player do Drive antes de agendar a limpeza
do R2 (margem de 24 horas). Não se considera o upload concluído como prova de que
o player do Drive está pronto. Limites iniciais: 2 GiB por temporário e 8 GiB no
servidor; ao atingir um limite, o app avisa e envia somente ao Drive.

## Prévia e mini menu durante a captura

A área **Tela capturada** mostra frames da própria sessão escolhida no sistema,
com dimensões da fonte e atualização de até 2 vezes por segundo. A prévia é reduzida
para no máximo 480 × 270 pixels e não altera a resolução do vídeo salvo. Os frames
ficam somente em memória; não são enviados ao site.

O mini menu aparece ao começar a gravação e fica acima das outras janelas. Arraste
pela alça à esquerda. Ele permite pausar/retomar, parar e salvar, silenciar/reativar
microfone e som do computador separadamente, voltar à janela principal e descartar.
Os botões de áudio só ficam disponíveis para as fontes ligadas no início.
O mute mantém a trilha e o relógio, gravando silêncio até reativar.

**Descartar gravação** pede confirmação, encerra o motor, remove somente o MP4,
o parcial e os relatórios da sessão atual e cancela sua reserva de link. Nenhum
upload é iniciado para essa sessão; vídeos anteriores não são removidos. Se o
site estiver offline, o cancelamento do link é repetido pela fila ao reconectar.

No Windows/macOS, o menu solicita exclusão da captura pelo sistema. No Linux
Wayland, essa exclusão não é garantida pelo compositor; o mini menu pode aparecer
na gravação de um monitor inteiro. A prévia também congela quando a sessão é pausada.

Teste nativo reproduzível da prévia, controles entre janelas e descarte:

```sh
python3 scripts/test-desktop.py --controls
```

Ele usa uma fonte sintética e não captura a tela real sem `--native`. O modo de
controle/descarte acima deve ser usado apenas com a fonte sintética.


### Bandeja e segundo plano

No Linux, o ícone usa StatusNotifierItem/D-Bus diretamente (KDE e desktops com
suporte à bandeja), sem exigir libappindicator. Nos demais sistemas, usa a bandeja
nativa da Tauri. O menu oferece abrir a janela, abrir a pasta de vídeos e sair.
Se o ambiente gráfico não exibir a bandeja, reabra o executável para restaurar a
janela existente. Nenhuma instância duplicada é criada.

Teste Linux da ocultação pelo evento nativo de fechamento, ativação via bandeja,
captura contínua com janela oculta e saída com MP4 finalizado:

```sh
python3 scripts/test-desktop.py --background
```
