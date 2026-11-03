# Validação do desktop Tauri — 31 de outubro de 2026

## Entrega

Interface nativa Tauri 2.11 com React, integrada diretamente à biblioteca Rust.
Seleção de monitor/dispositivo, qualidade, fontes e volumes de áudio; início,
pausa, retomada, encerramento, escolha da pasta e lista dos vídeos locais.
O executável Linux inclui a interface e não precisa de servidor de desenvolvimento.

Não há vínculo de conta, fila de upload ou envio para Google Drive nesta etapa.

## Verificações concluídas

- `npm run desktop:web:build`: TypeScript e bundle da interface aprovados.
- `npm run desktop:build`: executável Tauri release Linux gerado pela CLI oficial.
- `npm run build`: site existente continua compilando.
- `npm test`: **15 testes passaram**, incluindo 11 da API existente e 4 da interface.
  Os testes de API precisaram executar fora do sandbox porque Supertest abre uma
  porta local; a primeira execução restrita falhou com `listen EPERM`.
- Motor Rust: **9 testes passaram** — 5 unitários, 1 de controle de duas sessões
  no mesmo processo e 3 de pipeline com FFmpeg real.
- Desktop Rust: teste de nomes de arquivo aprovado.
- Clippy com warnings como erro e formatação Rust verificados.
- Motor compartilhado: `cargo check --all-targets` para Windows GNU e macOS ARM64.
  Isso não equivale a build ou execução da interface nessas plataformas.

Os testes da interface verificam configurações enviadas ao Rust, bloqueio de
configuração durante captura, pausa/retomada/finalização, abertura pelo nome do
arquivo, ferramentas ausentes, erro de início e descarte de eventos antigos.

## Janela nativa real, fonte sintética

O teste WebDriver abriu uma janela GTK/WebKit Tauri, sem mocks de IPC, motor ou
encoder. Os cliques geraram comandos Rust e MP4s reais. A fonte de imagem/áudio
gerada foi habilitada exclusivamente pelo ambiente do binário debug.

O script reproduzível `scripts/test-desktop.py` concluiu **duas gravações
consecutivas na mesma janela**:

| Medida | Sessão 1 | Sessão 2 |
| --- | --- | --- |
| Quadros decodificados | 71 | 70 |
| Duração do vídeo | 2,367 s | 2,367 s |
| Maior diferença entre finais de áudio/vídeo | 33,334 ms | 53,105 ms |
| Trilhas de áudio | 3 | 3 |
| Checagens técnicas | Aprovadas | Aprovadas |

O contador ficou parado durante a pausa; o vídeo apareceu na biblioteca ao
concluir. Os screenshots foram inspecionados visualmente. O teste também exige
confirmação do modo sintético antes de clicar, para não capturar hardware por
engano. Esse modo não pode ser ativado em release.

Artefatos locais: `artifacts/desktop-tests/ready.png`, `paused-1.png`,
`completed-1.png`, `completed-2.png`, `recording-1.json`, `recording-2.json` e
`driver.log`.

O WebKitWebDriver não estava instalado no CachyOS. Para esta sessão, foram usados
o driver oficial Tauri e o WebKitWebDriver do repositório oficial Ubuntu, com suas
bibliotecas ICU extraídas em `.tools`, sem instalar/substituir bibliotecas globais.

## Captura real pela interface em Linux Wayland

A janela real iniciou captura nativa pelo diálogo do sistema, com áudio do
computador e entrada padrão como microfone. Os botões pausaram, retomaram e
encerraram a captura. O vídeo apareceu na lista de gravações.

| Medida | Resultado |
| --- | --- |
| Tempo ativo / de relógio | 2,690 / 4,091 s |
| Quadros decodificados | 82 |
| Duração do vídeo | 2,675 s |
| Maior diferença entre finais de áudio/vídeo | 8,521 ms |
| Trilhas de áudio | Mixagem + sistema + microfone |
| Checagens técnicas | Aprovadas |
| Pico do áudio do sistema / microfone | 0 / 0,00507 |

Não havia sinal de áudio do sistema nesse teste; sua trilha ficou silenciosa.
O sinal do microfone era baixo e não foi feita avaliação auditiva de voz. Este
teste comprova a integração dos botões com a captura real e a finalização,
sem homologar inteligibilidade ou sincronização perceptiva.

Uma primeira tentativa detectou timestamps sobrepostos do microfone e encerrou
com erro explícito e arquivo parcial. O relógio CPAL foi corrigido para ancorar
uma vez no relógio do dispositivo e manter os pacotes contínuos por amostras,
preservando descontinuidades positivas. O teste de regressão cobre callbacks
com jitter/retrocesso e gaps reais; a captura acima usa essa correção.

Artefatos: `artifacts/rust/desktop-ui-native-report.json`,
`desktop-native-recording.png`, `desktop-native-paused.png` e
`desktop-native-completed.png`.

O ensaio anterior de 30 minutos documentado em `desktop-rust/VALIDATION.md`
antecede a interface, os volumes e esse ajuste do relógio. Não foi repetido para
o aplicativo completo, e os números antigos não representam o uso de memória
da GUI e seus processos WebKit.

## Windows e macOS

O workflow `.github/workflows/desktop.yml` foi preparado para builds nativos
Linux `.deb`, Windows NSIS e macOS `.app`, com testes de interface e motor.
No Linux, inclui teste da janela nativa em Xvfb com fonte gerada.
**O workflow não foi executado no GitHub nesta sessão.**

A tentativa de verificar a GUI macOS a partir do Linux parou em
`objc2-exception-helper`, que exige compilador/SDK Apple. O motor isolado passou
nas verificações de tipos para macOS e Windows, mas a interface precisa ser
compilada e executada nos sistemas correspondentes. Não há instalador Windows
ou app macOS homologado nesta entrega.

## Pendências de homologação

- Captura real em Windows e macOS, com permissões, dispositivos, pausas e arquivos.
- Escolha de pasta e abertura de arquivo/player por revisão manual em cada OS;
  chamadas da interface e restrição de caminhos foram implementadas.
- Homologação de ocultação, bandeja e saída nos demais sistemas e durante
  diálogos de permissão. O teste Linux de segundo plano está descrito abaixo.
- Sessões prolongadas do aplicativo inteiro, conteúdo em movimento e marcadores
  audiovisuais; avaliar volumes/ganho e todas as combinações de áudio no hardware.
- Assinatura/notarização e empacotamento de uma distribuição de FFmpeg compatível.

A compatibilidade com os três sistemas é o objetivo da arquitetura e do CI;
a execução real confirmada nesta sessão é no Linux Wayland.

## Integração Drive — 2026-10-31

Implementados vínculo por código de uso único, sessão no chaveiro nativo,
reserva antecipada do link, envio direto em blocos de 8 MiB, consulta do offset
confirmado, retomada após reinício e fila separada por conta. A conclusão chama
as verificações da API antes de disponibilizar o vídeo. URLs de sessão de upload
ficam apenas em memória no desktop; a API mantém sua cópia cifrada.

Validações desta alteração:

- Testes Rust do desktop: destino HTTPS restrito, Range limitado, fila persistente
  sem token, isolamento entre contas e servidor HTTP de teste simulando envio
  parcial seguido de reinício e conclusão.
- Testes React: conexão com código, link antecipado, copiar, repetir envio e seleção
  de arquivos locais concluídos. Os quatro testes anteriores da interface passaram.
- Janela Tauri real com fonte sintética: duas gravações consecutivas com pausa,
  retomada e parada; ambas passaram nas verificações do MP4 (aproximadamente 29,6 FPS; diferença
  máxima entre fim de áudio e vídeo de aproximadamente 55 ms no teste final).
- Build da interface, build Linux e Clippy sem avisos.
- Secret Service disponível neste Linux via KWallet/ksecretd. Isso não substitui
  a validação de gravação/leitura da sessão após autorização humana.

O teste de upload usa servidor simulado. A autorização humana do desktop e o
upload de um vídeo para a conta Google real continuam necessários para confirmar
o fluxo completo em produção. Os novos componentes nativos de chaveiro,
clipboard e instância única precisam de execução real no Windows e macOS.

## R2 temporário — 2026-10-31

Testes Rust adicionados para destino temporário restrito e reinício entre a parte
R2 confirmada e a fase Drive. Os 7 testes do desktop passaram. Os testes do site
cobrem isolamento, retomada multipart, publicação antes do Drive, arquivos antigos,
limites de capacidade, confirmação do dono, período de segurança, limpeza idempotente
e preservação da posição ao renovar a URL do player. Credenciais reais validadas
em produção com upload, HEAD, leitura parcial e exclusão de objeto de diagnóstico.

## Prévia, controles flutuantes e descarte — 2026-10-31

- Prévia obtida dos frames BGRA da fonte real, com conversão RGB e JPEG em fila
  limitada de um frame e até 2 atualizações por segundo. CLI sem observador não
  produz thumbnails. Testes cobrem dimensões, cores, limite e isolamento de sessões.
- Teste do motor com FFmpeg decodifica as trilhas isoladas e confirma silêncio
  quando ambas estão mutadas, retorno apenas do sistema e depois do microfone.
- Smoke test WebDriver com `--controls`: janela real, prévia carregada, mute das
  duas fontes no mini menu, pausa refletida na janela principal, duas gravações
  preservadas e terceira gravação descartada sem arquivos remanescentes.
- As duas gravações passaram nos checks do MP4; taxa próxima de 30 FPS e diferença
  máxima entre fim de áudio/vídeo de aproximadamente 52 ms no teste sintético.
- API de descarte testada com autenticação, isolamento, idempotência e recusa de
  remover registros com conteúdo já enviado. A rota está publicada na Vercel.

A proteção contra captura do menu continua dependente do sistema/compositor.
Não houve teste real dessa exclusão, de arraste ou de mute em hardware nesta etapa.


## Segundo plano e bandeja — 2026-10-31

O evento nativo CloseRequested agora apenas oculta a janela. Não envia Stop nem
marca saída pendente. Um ícone StatusNotifierItem/D-Bus no Linux fornece Abrir
Tinrec, Abrir pasta de vídeos e Sair do Tinrec. Windows/macOS usam o tray da Tauri.
Nova tentativa de abrir o executável restaura a instância existente.

Teste `scripts/test-desktop.py --background` com fonte sintética e D-Bus isolado:
fechamento de janela ociosa sem encerrar o processo, ativação pela interface real
da bandeja, captura continuando com a janela oculta e saída pelo item de menu
D-Bus com MP4 finalizado e serviço da bandeja encerrado. O relatório está em
`artifacts/desktop-background-tests/background-quit-report.json`.

O teste verifica a implementação do protocolo da bandeja; não automatiza cliques
no painel KDE/GNOME nem confirma a aparência em todos os ambientes gráficos.
Não habilita autostart no login do sistema. As rotinas de upload não dependem da
visibilidade da janela e seguem executando até Sair ser acionado.

## Tela, janela, área e câmera — 2026-11-01

- Quatro opções na interface e atalhos independentes para tela, janela e área.
- WindowCaptureTarget chega ao backend; o portal Wayland usa `types=2` para
  janela e `types=1` para tela, sem a opção de monitor no seletor de janelas.
- Seleção de área conferida no navegador com uma fonte FFmpeg gerada: arraste,
  dimensões em pixels da fonte original e confirmação antes de iniciar.
- Integração sintética gera MP4 com recorte de coordenada/largura ímpares,
  verifica dimensões finais e prova que o tempo de seleção fica fora do vídeo.
  Cancelamento não publica MP4 nem arquivo parcial.
- Pipeline de câmera testado com lavfi, sem acessar câmera física: leitura BGRA,
  preservação de aspecto/padding e encerramento do processo. Parsers de listas
  Windows/macOS testados para não confundir áudio, telas e câmeras.
- Captura de câmera real, áudio/câmera e permissões precisam ser validados no
  hardware. Windows/macOS ainda dependem de execução do CI e teste nativo.

## Webcam flutuante e controles ao vivo

- Interface: teste do botão de ativar/desativar no mini menu, prévia restrita à sessão ativa e limpeza ao desligar. Suíte desktop: 20 testes aprovados.
- Motor: MP4 sintético decodificado em cinco momentos comprova ativação, mudança de canto, desativação sem resíduo e reativação na última posição.
- Decoder: fonte FFmpeg gerada testa câmera quadrada e encerramento/reabertura do processo no mesmo arquivo de log.
- Nativo: 15 testes aprovados, incluindo persistência da posição e rejeição de valores inválidos. Clippy sem avisos.
- Executável Linux atualizado. Câmera física, arraste no compositor real e captura da janela flutuante no Linux ainda exigem teste manual; os testes automatizados não acessam a câmera pessoal. Windows e macOS não foram executados neste ambiente.


## Prints — 02/11/2026

Editor verificado no navegador com imagem sintética: arraste inicial, recorte,
redimensionamento, desfazer, conta-gotas e exportação real em canvas. Máscara opaca
verificada nos pixels exportados. Evidência: `/tmp/tinrec-editor-prints.png`.
Testes cobrem documento, efeitos, composição final nos três destinos, MIME PNG no
Drive, API e persistência da fila. Site publicado e migração de media_type aplicada.
Não foi realizado upload real na conta pessoal, nem teste físico de permissões,
seletores e clipboard em Windows/macOS/Linux. Esses testes permanecem manuais.

Verificação final: 83 testes web/API e 20 testes Rust passaram; builds web/desktop,
Clippy com warnings como erro e build release Linux passaram. Suíte desktop:
53 testes passaram. Executável atualizado em artifacts/Tinrec-linux-x64.

Ajuste do fluxo Wayland: prints usam um quadro PipeWire, sem o painel Screenshot
do KDE. Área usa o overlay Tinrec; o portal recebe apenas fontes de monitor.
Clippy e testes de screenshots verificados; interação física KDE pendente.

Fluxo revisado por solicitação: editor removido da interface de captura; PNG salvo
e copiado diretamente, envio explícito pela biblioteca. Testes verificam os dois
destinos e preservação do arquivo quando clipboard falha. Área usa imagem congelada.
Portal Screenshot não interativo só com um monitor; outros casos usam PipeWire.
Validação física de autorização e monitor no KDE permanece pendente.

Seleção de área com edição sobre o monitor: barra flutuante com desenho, seta,
formas, texto, destaque, máscara e desfoque; copiar/enviar exportam apenas o
recorte composto. Testes cobrem não concluir ao selecionar, composição enviada
aos dois destinos e preservação das anotações em falha de envio. Browser com
imagem sintética confirmou seleção e seta; evidência /tmp/tinrec-print-overlay.png.
Teste físico KDE/clipboard/Drive continua pendente.

Correção da barra do print: posicionamento medido perto da seleção, limites laterais
e margem inferior de 80px; fundo nativo opaco no print e transparente ao voltar
a selecionar área de vídeo. Teste de geometria, build e Clippy passaram.
A falha de repintura ao hover reportada no KDE exige confirmação nesse ambiente.

Após regressão de tela branca reportada no KDE: revertida a troca do fundo nativo
no print e removidas promoções de camada CSS. A janela area usa WebKitSettings
hardware-acceleration-policy Never no Linux, antes de seu primeiro uso.
Build/Clippy validam integração; não há confirmação visual nativa neste ambiente.

Após persistência da falha: print-area passa a ser janela própria opaca desde
a criação, com rota e capability dedicadas. A janela transparente area fica só
para vídeo. Posição física reaplicada após tamanho; máscara por quatro retângulos
substitui sombra de 99999px. 12 testes de seleção/overlay, builds e Clippy passam.
A renderização nativa no KDE do usuário ainda não foi verificada.

Visual do overlay de print alinhado ao mini menu de vídeo: paleta escura,
botões de 36px, bordas, separadores e destaque azul para envio. Apenas CSS
e classes de apresentação; janela opaca e posicionamento preservados.

Prints configuram explicitamente CursorMode::Hidden. Removido o caminho Screenshot
não interativo, que não permite exigir cursor oculto pela API usada. Vídeos
preservam sua configuração. Compilação e testes de screenshots verificados;
a ausência do ponteiro na captura física ainda exige teste por plataforma.

Restauração de permissões Wayland compartilhada pelo motor de vídeo e screenshot:
SessionBuilder recebe restore_token persistido e renova o token retornado. Arquivo
local com modo 0600 e gravação atômica; falha ao persistir não bloqueia captura.
Rejeição invalida token para próxima tentativa. Build/Clippy verificados; restauração
real do KDE precisa ser confirmada com autorização concedida pelo usuário.

Correção do modo Área: removida conversão de area para screen no Wayland.
Gravação agora chama o seletor de área Tinrec em cada sessão. Tokens de monitor
usam namespace novo para descartar escolhas antigas que podiam representar regiões.
Botão de troca esquece as escolhas locais para a próxima captura nos dois modos.
Portal ainda pode oferecer região; para seleção Tinrec deve-se autorizar monitor.
Validação física do recorte e restauração KDE permanece necessária.

Fluxo de fonte alinhado ao Cap: removido cache/restauração de tokens e botão de
esquecer fonte; portal persist_mode=0. Wayland lista monitores via Tauri e codifica
seus limites lógicos no alvo; backend valida posição/tamanho de stream antes de
iniciar PipeWire. Rejeita região errada e monitores iguais em posições distintas.
Campo Tela disponível nas duas plataformas; removidos overrides para auto.
Área nova por padrão; fixar área de vídeo é opt-in e limitado ao monitor escolhido.
31 testes React e 2 testes do backend de monitores passaram. Integração física
KDE/Windows ainda não executada; não equivale a teste ponta a ponta nesses sistemas.

Rollback solicitado: fluxo de captura anterior à mudança de cursor restaurado.
Removidos validação de monitor no portal, fonte customizada, cache de autorização
e área fixa. Screenshot não interativo com um monitor voltou; cursor usa padrão
anterior. Vídeo Wayland usa seleção nativa do portal, sem segundo recorte.
Mantidos identidade, biblioteca de prints e minimização da janela principal.

Preferências de cópia local separadas: prints persistem decisão no backend para
compartilhar com overlay; copiar não produz salvamento duplicado. Upload sem cópia
local usa pending-prints e só remove após status uploaded. Caminhos de limpeza
continuam restritos aos diretórios temporários. Nova gravação local usa Videos;
lookup/listagem preservam arquivos antigos da raiz. Testes cobrem independência
das opções, encaminhamento dos três atalhos e limpeza protegida após upload.
