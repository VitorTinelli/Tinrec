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
