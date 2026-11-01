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

## Webcam sobre a gravação

No Gravador, ative **Webcam**, selecione o dispositivo e clique em Tela, Janela ou Área. A webcam aparece em um círculo flutuante enquanto grava. Arraste o círculo para mudar de lugar; a posição é guardada no aplicativo e reutilizada nas próximas gravações. O botão de câmera do mini menu liga ou desliga a webcam durante a gravação, inclusive se ela estava desligada ao iniciar (usa a câmera selecionada ou a primeira disponível). Desligar libera o dispositivo. A câmera também é encerrada ao parar ou descartar. No modo Área, a posição inicial respeita o recorte. O modo Somente câmera continua disponível separadamente.

No macOS, autorize o acesso à câmera quando solicitado; no Windows, habilite o acesso à câmera para aplicativos da área de trabalho. No Linux, o dispositivo V4L2 precisa estar acessível ao usuário. Em sessões Wayland com XWayland disponível, a interface usa X11 para permitir arraste e restauração da posição; a captura continua pelo portal Wayland. A interface desativa o renderizador DMA-BUF do WebKit por padrão para evitar falha de alocação GBM e janela em branco. Isso não altera a captura PipeWire nem o encoder FFmpeg. Escolhas explícitas de GDK_BACKEND e WEBKIT_DISABLE_DMABUF_RENDERER são respeitadas. Em Wayland puro, posicionamento absoluto depende do compositor. Na captura de tela/área no Linux, a janela da webcam entra pela própria captura, evitando duplicação; em captura de janela e nos demais sistemas, a composição é feita pelo motor. Os testes automatizados usam imagens geradas; qualidade, latência e permissões precisam de validação com a câmera real em cada sistema.

## Usar

1. Defina o nome obrigatório e a qualidade em **Configurações**. No **Gravador**,
   escolha o modo, a fonte e o áudio. O microfone é mono
   para reproduzir a voz nos dois lados; o áudio do computador preserva seus canais.
2. Ajuste os volumes antes de iniciar; 80% é o padrão. Fontes e volumes ficam
   bloqueados durante a gravação para evitar mudanças de formato.
3. Clique em **Tela**, **Janela**, **Área** ou **Somente câmera** para iniciar.
   Se faltar uma janela ou câmera, selecione a fonte e a gravação começa.
   No Wayland, escolha a fonte no diálogo do sistema.
   A escolha do dropdown não substitui a seleção pelo portal. Nos outros backends,
   escolha o monitor listado.
4. Use **Pausar**, **Retomar** e **Encerrar**. O contador vem do motor Rust e
   exclui as pausas. Cada gravação tem limite de duas horas de tempo ativo.
5. Aguarde **Vídeo salvo**. Com a conta Google conectada, a página deste vídeo
   abre automaticamente no navegador. Enquanto o envio termina, a página mostra
   o progresso e libera a reprodução quando o arquivo estiver disponível.
   Gravações descartadas ou com falha não abrem páginas. Em **Meus vídeos**, o arquivo aparece em um cartão
   com miniatura real, duração, data e tamanho. Busque pelo título e clique no
   cartão para abrir no player padrão do sistema. Cada cartão também tem
   **Abrir no site**, que usa o link daquele arquivo na conta Google conectada. **Ver no site** abre a biblioteca web. A conexão Google fica em
   **Configurações**; os envios continuam em segundo plano.

A navegação entre as seções preserva a gravação e os envios ativos. As miniaturas
são geradas localmente pelo FFmpeg apenas quando os cartões entram na tela, uma
por vez, e ficam em memória durante o uso. Arquivos incompatíveis mantêm um
ícone de vídeo, sem impedir a abertura.

A qualidade e a pasta ficam em **Configurações → Qualidade e armazenamento**.
**Manter uma cópia no computador** vem ativado. Ao desativar, é necessário
conectar o Google: o arquivo fica em `pending-recordings` nos dados do aplicativo
e só é removido após o servidor confirmar o envio completo ao Drive. A política
fica salva na fila e sobrevive a reinícios. Falhas preservam o temporário para
nova tentativa; vídeos anteriores na pasta escolhida não são apagados.

O caminho exibido em Configurações permite escolher outra pasta pelo seletor nativo.
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

Na raiz do repositório, com Node.js 22+ e Rust estável (mínimo 1.90):

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

## Modelo de nome

O padrão em pt-BR é `{fonte} ({tipo}) - {data} às {hora}`. Exemplo:
`Tela selecionada (Tela) - 01-11-2026 às 14h05m09s`.
O modelo é salvo para as próximas gravações e resolvido ao iniciar, no horário
local. Variáveis: `{fonte}`, `{tipo}`, `{data}`, `{hora}`, `{dia}`, `{mes}`,
`{ano}` e `{aleatorio}` (código de 6 caracteres). Nomes fixos também são aceitos.
O nome é obrigatório; campos vazios, variáveis desconhecidas e caracteres
reservados impedem o início. No Wayland, a fonte usa “Tela selecionada”, pois a
escolha ocorre depois, no diálogo do sistema. O sufixo exclusivo do arquivo é
preservado para evitar colisões; Drive e biblioteca usam o título resolvido.

## Mini menu durante a captura

A janela do gravador não exibe nem gera prévia ao vivo. As miniaturas da biblioteca
são obtidas dos vídeos salvos, apenas quando necessário.

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
na gravação de um monitor inteiro.

Teste nativo reproduzível dos controles entre janelas e descarte:

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

## Atalhos de teclado

Em **Configurações → Atalhos de teclado**, selecione o campo da ação e pressione
uma combinação com Ctrl, Alt ou Cmd. Também é possível digitá-la, por exemplo
`Ctrl+Shift+R`. Clique em **Salvar atalhos**. Nenhum atalho global vem atribuído,
seguindo o modelo do Cap. Combinações repetidas são recusadas; falhas de registro
aparecem no painel. As combinações são salvas e restauradas ao abrir o aplicativo.

Ações: gravar tela, janela ou área; abrir gravador, iniciar gravação no modo
selecionado, parar e salvar, pausar/retomar, alternar
microfone, alternar som do computador, mostrar mini menu e abrir biblioteca.
As ações de controle exigem uma gravação no estado apropriado; o início utiliza
as configurações atuais. Não há atalho para descartar sem confirmação.

No Windows/macOS e X11, o registro usa o plugin oficial `global-shortcut` do
Tauri. No Linux Wayland, usa o portal `org.freedesktop.portal.GlobalShortcuts`;
confirme a janela do sistema ao salvar ou restaurar os atalhos. A disponibilidade
depende do compositor e do portal instalado. O painel mostra as combinações
ativas retornadas pelo sistema. O portal foi detectado no ambiente KDE atual;
a autorização e o acionamento físico precisam ser conferidos ao escolher os
atalhos. Nenhuma combinação foi registrada na sessão do usuário durante o teste.

Com uma janela do Tinrec em foco, `Ctrl/Cmd + ,` abre Configurações e
`Ctrl/Cmd + W` oculta a janela sem encerrar a gravação. Esses dois atalhos são
locais e não precisam de registro global.

## Modos de captura

O gravador oferece **Tela**, **Janela**, **Área** e **Somente câmera**.
No Wayland, Tela e Área abrem o seletor de monitores do sistema; Janela abre o
seletor restrito a janelas. Nos outros sistemas, escolha o monitor ou janela na
lista. Não há substituição silenciosa de uma janela por um monitor inteiro.

Em Área, selecione a fonte e arraste sobre a imagem estática apresentada pelo
Tinrec. Confirme **Gravar área** para iniciar. O tempo de seleção fica fora da
gravação e só o recorte é codificado. Cancelar descarta a sessão ainda não iniciada.
A imagem de seleção permanece somente em memória; não é enviada ao site.
O gravador é ocultado antes de obter essa imagem e ao confirmar o recorte, para
não aparecer sobre a região gravada. Os controles flutuantes continuam disponíveis.

Somente câmera usa o dispositivo escolhido por FFmpeg: V4L2 no Linux, DirectShow
no Windows e AVFoundation no macOS. Não abre captura de tela. Microfone e som do
computador continuam opcionais. Câmeras precisam de permissão do sistema e de
um FFmpeg com o dispositivo correspondente; a resolução efetiva depende da
câmera. A captura usa fila limitada e é encerrada ao parar ou falhar. A listagem
Linux pode incluir mais de um nó do mesmo dispositivo; escolha o nó de vídeo.

Em Atalhos, **Gravar tela**, **Gravar janela** e **Gravar área** são ações
independentes. No Wayland abrem a seleção nativa do modo e prosseguem; em X11,
Windows e macOS, Janela sem uma seleção abre o gravador para escolher a fonte.
As combinações continuam configuráveis e sem atribuições por padrão.

A dependência Pinray Linux 0.2.7 recebe um pequeno ajuste local, documentado em
`desktop-rust/vendor/pinray-platform-linux/README.tinrec.md`, para respeitar o tipo
Tela/Janela no portal. Captura com câmera física, permissões e sincronização
perceptiva ainda devem ser conferidas no dispositivo em cada sistema operacional.

A janela principal inicia em 820 × 700, com mínimo de 700 × 560. O gravador tem
uma única coluna; durante a captura, uma faixa compacta mantém o estado e os
controles acessíveis, além do mini menu flutuante. Não há um botão separado de
início: os quatro cartões iniciam o fluxo de captura.
