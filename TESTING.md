# Testes

| Camada                        | Comando                                                                                                                                             | Contrato verificado                                                                                                                                                         |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Site, API e interface desktop | `npm test`                                                                                                                                          | Autorização, compartilhamento, multipart, retenção, reprodução e comandos da interface. Cada teste de API usa um PostgreSQL PGlite próprio.                                 |
| Motor de captura              | `cargo test --locked --manifest-path desktop-rust/Cargo.toml -- --include-ignored --nocapture --test-threads=1`                                     | MP4, trilhas de áudio, pausas, mute, recorte, webcam, falha do encoder e proteção contra sobrescrita. Requer FFmpeg/libx264 e ffprobe.                                      |
| Backend Tauri                 | `cargo test --locked --manifest-path desktop-app/src-tauri/Cargo.toml --features custom-protocol -- --include-ignored --nocapture --test-threads=1` | Fila persistente, isolamento de contas, retomada, descarte, atalhos e miniaturas. Requer dependências nativas, FFmpeg e frontend compilado com `npm run desktop:web:build`. |
| Interface nativa Linux        | `python3 scripts/test-desktop.py --controls` e `python3 scripts/test-desktop.py --background`                                                       | Fluxo WebDriver → IPC → motor → arquivo; janela flutuante, descarte e ciclo de vida na bandeja. Requer binário debug, tauri-driver, WebKitWebDriver e sessão gráfica/D-Bus. |
| Fila Windows WPF              | `dotnet run --project desktop/Tinrec.Tests/Tinrec.Tests.csproj -p:Platform=x64`                                                                     | DPAPI, retomada, sessão expirada, backoff e retenção da implementação WPF. Requer Windows e SDK .NET 8.                                                                     |

A CI ativa do desktop é `Windows desktop`: valida interface, motor Rust e backend Tauri, gera o instalador e executa `scripts/test-windows-installed.ps1` no runner Windows. O teste instalado verifica montagem React e os comandos IPC de captura nativa, pausa, retomada, parada, seleção de área e descarte, além do encoder empacotado com áudio/vídeo sintéticos e perfil isolado. Exige sessão gráfica com WGC e bloqueia instalações existentes. GPUs, múltiplos monitores, webcam e suspensão exigem homologação real. FFmpeg tem versão e SHA256 fixos em `scripts/prepare-windows-ffmpeg.ps1`.

Linux/macOS e o WPF estão suspensos. Seus testes e fontes permanecem como histórico; os workflows legados têm apenas disparo manual. Consulte [o foco Windows](docs/WINDOWS-FOCUS.md) para comandos e limites da validação.

## Suíte mantida após a revisão

A revisão considera o desktop Tauri/Rust para Windows e o site/API em uso.
A contagem é de casos executados, incluindo tabelas parametrizadas; consolidar
um caso não significa retirar suas verificações.

| Camada | Antes | Após consolidação | Após correções de captura |
| --- | ---: | ---: | ---: |
| Interface desktop | 71 | 62 | 75 |
| Site, API e player | 30 | 25 | 25 |
| Backend Tauri | 22 | 20 | 25 |
| Motor de captura | 27 | 23 | 25 |
| Total ativo | 150 | 130 | 150 |

Os quatro cenários de proteção do instalador permanecem separados dessa
contagem. O autoteste instalado também permanece: é a verificação do caminho
WebView → IPC → captura nativa → arquivo e exige sessão gráfica com WGC.
Testes com mocks verificam o fluxo do autoteste, sem comprovar captura física.

### O que foi removido ou consolidado

- Removidas as renderizações específicas de Linux/macOS e a repetição da mesma
  migração de preferência por sistema. A migração continua testada no Windows.
  Scripts nativos Linux e testes WPF ficam congelados como histórico, fora da CI
  automática e da contagem ativa; não validam a versão Tauri para Windows.
- Removido o teste de layout que verificava principalmente ícone e separação de
  telas. Navegação, comandos de captura e erros na biblioteca continuam cobertos.
- Os três atalhos de print compartilham uma montagem da interface, com comando e
  encerramento verificados a cada captura. O cenário principal de gravação carrega
  o nome salvo, deixando a edição e persistência para o teste específico de nomes.
- Seleção em 4K e arraste inverso compartilham um caso. Limites, arredondamento e
  clique sem arraste permanecem distintos. O nome vazio continua no fluxo de
  configurações; o teste unitário mantém espaços em branco e limites de caracteres
  e bytes.
- Removido o teste que apenas repetia o retorno constante da seleção de hardware.
  O pipeline verifica o encoder efetivamente reportado. O caso de timestamps
  irregulares, com tolerância mais estrita, cobre também o fim de vídeo esparso.
- Decodificação, pixels, letterbox, webcam quadrada e reabertura compartilham a
  preparação. Composição da webcam no recorte e controles ao vivo usam o mesmo
  vídeo, com pixels inspecionados antes e depois de cada mudança.
- Links por ID e caminho compartilham reserva, deduplicação, remoção do arquivo e
  troca de conta. A proteção contra arquivos homônimos continua independente.
  Validação de PNG inclui roundtrip e rejeição de dados inválidos no mesmo caso.
- Reserva idempotente e exposição pública compartilham o mesmo registro de API.
  Finalização rejeitada e publicação após verificação do Drive compartilham outro.
  Autorização da limpeza é verificada junto da retenção e do período de carência.
- Reprodução temporária e renovação de URL compartilham o player. A confirmação
  pelo proprietário inclui a transição para reprodução no Drive.
- A CI do site executa somente `tests/`; a CI Windows executa somente a interface
  desktop e os componentes nativos. Isso elimina a repetição da suíte desktop em
  Linux e da compilação do frontend desktop no workflow do site. `npm test` local
  continua executando os dois conjuntos JavaScript.

### Critério para manter testes

Mantidos os contratos de autorização, isolamento de contas, criptografia,
retomada e integridade de uploads, retenção, proteção contra sobrescrita,
duração e trilhas, recorte, controles, falhas e confirmação de descarte.
Testes em camadas diferentes permanecem quando verificam fronteiras diferentes
(por exemplo, autorização da API versus isolamento da fila local, ou botão IPC
versus conteúdo real do MP4).

Não criar casos apenas para aumentar a contagem, repetir detalhes internos ou
revalidar a mesma preparação com nomes diferentes. Consolidar quando as mesmas
entradas e recursos exercitam um único fluxo; manter casos independentes quando
as precondições ou o comportamento de falha forem diferentes. Cada teste de API
continua usando seu próprio PGlite, sem compartilhar estado entre casos.

## Regressões de captura corrigidas em 06/11/2026

Os quatro casos adicionais verificam contratos novos identificados na auditoria:
MP4 reproduzível após falha de captura, ffprobe travado encerrado no prazo,
persistência de fila sem job fantasma e recuperação do print na biblioteca sem
sobrescrever um arquivo existente. Não são repetições da preparação anterior.
O fluxo existente do autoteste inclui agora fechamento nativo dos seletores,
print de tela salvo e listado e acompanhamento de uma janela de teste movida.
Esse fluxo só executa com --self-test e perfil isolado; não usa login ou upload.

## Seleção de área em múltiplos monitores

O backend testa a associação entre seletor, sessão e monitor, rejeitando áreas
fora dos limites e confirmações repetidas. A interface testa que a área escolhida
chega pronta ao editor de print, sem exigir outro arraste. O autoteste nativo
percorre todos os monitores detectados, seleciona uma área e grava um MP4 em cada
um; o print usa o último monitor e verifica o fechamento do editor.

Em 06/11/2026, esse fluxo passou com dois monitores no perfil isolado
`.tools/native-multimonitor-fc84ea756c0041248c3f9acc73685f2c`. Ambos os MP4 de
área têm 960×720 e duração de 1,333333 s, com checagens aprovadas. O teste escolhe
a região por IPC; a interação física de arraste entre telas com DPI diferente
ainda requer conferência manual. Não há upload nesse autoteste.

## Contorno da gravação

O teste geométrico cobre recortes em pixels físicos, origens negativas e janelas
que atravessam o limite de um monitor. O autoteste nativo verifica o alinhamento
do contorno após mover a janela e nas áreas dos monitores disponíveis, além de
visibilidade durante pausa/gravação e ocultação ao terminar. Também consulta o
Windows para confirmar exclusão da captura e estilos que deixam os cliques
passarem e impedem ativação da janela de contorno.

Validação nativa do contorno aprovada em 06/11/2026 no perfil
`.tools/native-border-cc7f1cf15c9d4ef9998faeea43aa7e7b`, com saída 0.

## Atalhos de teclas isoladas e mouse

Os dois casos existentes de backend foram atualizados para aceitar teclas
isoladas e modificadores opcionais, normalizar botões e rejeitar duplicatas e
formatos inválidos. O fluxo existente da interface agora configura F5, Q, uma
combinação de teclado e Ctrl+botão lateral, salva e remove um atalho. Não foram
criadas cópias do mesmo teste para cada botão. O autoteste nativo verifica as
entradas reais do Windows na janela de teste e a ausência de repetição por
tecla mantida; não altera os atalhos do perfil instalado.

## Transição do print sem troca de janela

O seletor do monitor escolhido é mantido enquanto a captura é feita, somente
quando o Windows confirma exclusão real dessa janela da captura. A interface
testa que a seleção permanece desenhada até a imagem ser decodificada e o
editor entrar no mesmo lugar. O teste nativo compara o identificador do seletor
com o editor visível e verifica que a janela principal não foi reaberta ao tirar
um print com o app oculto. O fechamento nativo do editor deve cancelar e limpar
a captura normalmente. O teste estrutural não mede cintilação percebida em
cada combinação de driver, monitor e frequência de atualização.

Autoteste da transição aprovado em 06/11/2026, saída 0, no perfil isolado
`.tools/native-print-transition-79209b0d65cf47509c9bd903ced4a657`.

## Seta numerada

O editor sobre a área e o editor de projetos compartilham a operação
`numbered-arrow`: seta preenchida com círculo numerado na origem do arraste.
O teste do overlay cobre sequência, desfazer e exportação com arraste inverso.
O teste de documento cobre reabertura de projeto e limites da seleção incluindo
o círculo. Uma prévia usando o renderizador real do canvas foi conferida em
`.tools/numbered-arrow-preview/preview.png`, com um número de dois dígitos.

## Liberação após cancelar print

O teste mantém a leitura das miniaturas pendente após o cancelamento e verifica
que os controles e a navegação são liberados e uma segunda captura pode iniciar
sem aguardar a biblioteca. A atualização continua em segundo plano para mostrar
arquivos preservados, inclusive em falhas. Respostas antigas de atualização
não substituem as mais recentes. A captura nativa consulta o cancelamento entre
as leituras de quadros, sem aguardar o prazo completo de obtenção da imagem.

## Comandos locais do editor de print

Ctrl+C copia a composição e Ctrl+S envia ao site. Esc primeiro desativa a
ferramenta; sem ferramenta ativa, cancela. Repetição automática da tecla Esc
não executa a segunda etapa. Ctrl+rolagem altera a espessura dentro dos limites
do controle e impede o zoom do navegador. Os comandos só existem nos editores
de print; não criam entradas na configuração de atalhos globais.

Um fluxo em cada editor cobre exportação pelas teclas, mudança de espessura e
Esc em duas etapas. O seletor que hospeda o editor delega o Esc ao editor, para
evitar que o mesmo evento também cancele a captura pelo seletor externo.

## Preferência de espessura

A espessura é persistida no armazenamento local do aplicativo ao ser alterada,
por Ctrl+rolagem ou pelo controle. Os editores usam a mesma preferência, com
os limites próprios de cada controle. Os testes remontam os editores para
verificar a restauração do valor e simulam armazenamento inválido ou
indisponível sem impedir a edição.

## Preparação da seleção de área

A instrução do seletor não tem botão de cancelar nem indicação de Esc, e
desaparece após confirmar o arraste. O teste mantém a garantia de que o recorte
continua desenhado até a imagem ser decodificada. A prévia usa um filtro PNG
fixo sem perdas; o caso de roundtrip compara todos os pixels RGBA, inclusive
alfa, tanto na codificação normal quanto na prévia rápida. O caminho que mantém
o seletor dispensa a nova enumeração e o reposicionamento dos monitores.

Autoteste nativo aprovado em 06/11/2026, saída 0, no perfil isolado
`.tools/native-area-preparation-1015d5b13cb3408db1ec95d7dc90857a`.

## Exportação de print sem recodificação redundante

O PNG composto pelo editor é validado e preservado byte a byte ao salvar,
sem nova compressão. A validação continua rejeitando arquivos de outro formato
disfarçados com o prefixo PNG. O teste existente de pixels cobre ambos.

Após exportar a área, o editor confirma a seleção com `returnImage: false`;
o resultado dessa captura contém dimensões e imagem vazia, dispensando recorte,
compressão e retorno de uma segunda imagem que não seria usada. Chamadores que
omitem a opção continuam recebendo PNG. O autoteste nativo exporta uma área
editada no canvas, salva e confirma esse retorno sem imagem redundante.

Autoteste nativo aprovado em 06/11/2026, saída 0, no perfil
`.tools/native-print-export-a2847195de194beda315e5c8ef40ba01`. A etapa nativa
de exportação para arquivo mediu 241 ms para a tela e 14 ms para a área; esses
tempos não incluem renderização do canvas, clipboard nem upload e não são uma
comparação antes/depois.

## Novo caminho residente dos prints no Windows

Seletores por monitor são carregados em segundo plano e reutilizados. Cada
ativação recebe uma nova sessão; o frontend limpa ferramentas, imagem e
promessas antigas, impedindo que uma decodificação atrasada reapareça na próxima
captura. O teste de transição percorre duas ativações no mesmo componente.

Tela e área usam captura GDI de uma imagem, com WGC como fallback em falhas.
Janela mantém WGC para preservar a captura de janelas ocluídas. A prévia de área
é um BMP em memória, servido por protocolo interno apenas ao editor ativo;
não envolve arquivo temporário nem compressão PNG/Base64. O caso de pixels
verifica orientação, canais e alfa desse BMP.

O canvas começa diretamente no recorte, evita criar uma tela inteira por traço,
e limita o desenho da prévia ao ciclo de pintura. A exportação PNG passa por
toBlob assíncrono. Área sem anotações pode exportar o recorte nativo; tela/janela
podem capturar e copiar/salvar no backend sem ida e volta da imagem pela UI.
O teste da interface garante que esse retorno não provoca uma segunda cópia.

O autoteste aguarda a pintura real do editor, mede o intervalo desde a seleção
e verifica um pixel conhecido da janela de teste através do transporte real.
Compara o fechamento com e sem imagem de retorno e também exporta o recorte
diretamente. Métricas usam compilação release; tempo de rede não é incluído.

Validação nativa de 06/11/2026, release com `custom-protocol`: relatório
`.tools/native-fast-print-ae3fe48a650a4101bab2d7d6800d74e2/self-test-result.json`
com status `passed`. Compara o pixel transportado com uma amostra nativa do
monitor, pois a cor exibida pelo Windows pode diferir do valor CSS original.
Tela/área, cancelamento, reutilização, canvas e recorte nativo passaram.
Exportação local para arquivo: 1–16 ms. Seleção até editor pintado: 457–617 ms;
captura e preparação do bitmap: 103–120 ms (incluídos no intervalo anterior).
São amostras locais, sem rede, e ainda não comprovam paridade com Flameshot.
Os 72 testes da interface passaram com um worker. A execução concorrente com
compilação release excedeu o timeout de cinco casos; a repetição isolada passou.


Correção de eventos entre monitores (06/11/2026): os listeners de area-pick,
area-reset e print-area-selection agora usam o alvo WebviewWindow da própria
janela. Listeners globais recebiam eventos destinados ao outro monitor e
sobrescreviam sua sessão. O caso existente foi parametrizado para os dois
rótulos e verifica todos os alvos, cancelamento e reutilização (2 casos passam).
Autoteste nativo da release: .tools/native-monitor-events-875139f40e4943e7ba95d0b86d4bdd90/self-test-result.json, status passed.


Barra compacta de print (06/11/2026): os 9 casos de PrintOverlay passaram,
incluindo arraste pela alça, cancelamento do arraste, movimento pelo teclado,
limites do viewport e preservação da posição ao trocar ferramenta. Desfoque
tem nome visível e ícone próprio; ocultação usa preenchimento e seta numerada
mostra número e seta. Compilação TypeScript/Vite aprovada.


Texto na imagem (06/11/2026): ferramenta Texto abre uma edição inline no
ponto clicado. Enter confirma; Shift+Enter cria uma linha; Esc descarta a
edição atual. Copiar/enviar inclui a edição ainda ativa. Os 10 casos do
PrintOverlay passaram, incluindo foco e exportação do texto na posição
correta. Compilação TypeScript/Vite aprovada.
