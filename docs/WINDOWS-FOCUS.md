# Tinrec Windows — foco e arquitetura

Desde 5 de novembro de 2026, a linha ativa do desktop é Rust/Tauri para Windows
x64. Entregas macOS/Linux e o cliente WPF ficam suspensos. O código histórico
permanece disponível; os workflows automáticos validam Windows.

## Responsabilidades

- `recorder.rs`: preparação, comandos, sessão e finalização da gravação.
- `devices.rs`: descoberta assíncrona; consultas FFmpeg têm prazo de cinco segundos.
- `windows.rs`: identificação exata do monitor selecionado; resoluções iguais não
  são usadas como identificação no caminho Windows de seleção de área.
- `library.rs`: arquivos locais e miniaturas.
- `cloud.rs` / `cloud_commands.rs`: fila persistente e fronteira IPC dos uploads.
- `pages.rs`: espera limitada de 90 segundos, deduplicação de aberturas pendentes
  e notificação de falhas. Um envio pode continuar depois desse prazo.
- `diagnostics.rs`: mensagens visíveis e log local de tamanho limitado.
- `smoke.rs`: autoteste do executável instalado com WebView/IPC e mídia sintética.

O motor mantém a captura fora da interface. No Windows, a política padrão usa
WGC para incluir o cursor. DXGI continua disponível na CLI, explicitamente, para
diagnóstico. O salvamento usa rename Windows sem substituição do destino,
dispensando hard links. Falhas preservam o arquivo de origem.

Quadros sem composição compartilham o buffer com o transporte. Recortes são
aplicados antes do pipe para o FFmpeg. A codificação VFR não usa B-frames, evitando
inconsistência entre o último PTS visível e a duração indicada no MP4; isso pode
alterar o tamanho do arquivo na mesma qualidade. Áudio e arquivos não são
descartados silenciosamente para mascarar sobrecarga.

## Encoder experimental

O encoder ativo é libx264. O caminho experimental Media Foundation foi suspenso: um teste de capacidade não garante que uma captura real seja aceita pelo driver. Mesmo com `TINREC_ENCODER=mf`, a gravação usa software e registra fallback no relatório. A reativação depende de recuperação que preserve todos os frames e validação da configuração real.

## Build reproduzível e validação

```powershell
.\scripts\prepare-windows-ffmpeg.ps1
npm ci
npm run desktop:package -- --bundles nsis
```

FFmpeg 9.0.2 é obtido de URL versionada e verificado por SHA256. As ferramentas
NSIS usam cache local do projeto, evitando a dependência do cache global.
O instalador inclui FFmpeg/ffprobe e obtém WebView2 quando necessário.

Em runner descartável ou Windows Sandbox, com nenhuma outra instância aberta:

```powershell
.\scripts\test-windows-installed.ps1 -Installer '.\artifacts\Tinrec-Setup-0.3.0-x64.exe'
```

O script faz uma instalação real e registra o aplicativo no usuário do Windows.
O autoteste usa `TINREC_DATA_DIR` isolado e `--self-test`. Captura a tela real por IPC, pausa, retoma e finaliza; depois seleciona uma área e a descarta. Verifica também montagem React, estado registrado, ferramentas empacotadas e áudio/vídeo sintéticos. Não usa microfone, login ou upload reais. Exige sessão gráfica ativa com suporte a WGC; falhas nativas reprovam o teste. O script bloqueia instalações já registradas em HKCU/HKLM. Não substitui homologação de GPUs, cursor, múltiplos monitores ou permissões.

Para comparação controlada de dois motores, com a mesma mídia gerada:

```powershell
.\scripts\benchmark-windows.ps1 -Baseline '.\baseline.exe' -Current '.\current.exe'
```

O resultado inclui bytes brutos para o encoder, CPU, memória, tamanho do MP4,
quadros descartados e verificação de trilhas/duração. Bytes de transporte não
equivalem a ganho percentual de FPS ou CPU. Um único teste não comprova
desempenho sustentado nem cobre GPUs e drivers de outras máquinas.

## Evidências locais — 5 de novembro de 2026

- 66 testes da interface, 21 do backend e 26 do motor aprovados. Sob compilação
  concorrente, seis testes de interface excederam cinco segundos; a repetição
  com um worker passou, sem ampliar os prazos. A CI usa esse limite de workers.
- Capturas WGC reais de tela e área em 1080p, com áudio do sistema/microfone e
  pausa/retomada na área, passaram: erro de término de 0,033 s e diferenças
  áudio/vídeo de 0,043 s (tela) e 0,034 s (área).
- Cenário sintético de oito segundos, origem 640x360, recorte 320x180: dados
  brutos enviados ao encoder passaram de 222.105.600 para 55.526.400 bytes
  (75% menos); pico combinado de RSS de 128.114.688 para 95.723.520 bytes
  (aproximadamente 25% menos). Ambos passaram sem descarte de quadros.
- Nesse ensaio, CPU média não melhorou: 36,81% para 42,57% de um núcleo;
  MP4 de 562.044 para 589.104 bytes. São medições curtas numa máquina de trabalho,
  com carga concorrente, e não uma comparação controlada de qualidade visual.
- O protótipo MF codificou nesta máquina. Em comparação separada, CPU caiu
  de 33,30% para 20,98% de um núcleo, mas houve quatro quadros descartados,
  maior tempo total (8,53 para 9,63 s) e MP4 de 562.044 para 2.249.250 bytes.
  São perfis de qualidade/bitrate diferentes; isso não comprova superioridade
  geral e justifica manter software como padrão.
- Os relatórios completos ficam em `artifacts/benchmark-*/comparison.json` e
  `artifacts/windows03-verified-*.mp4.capture.json` (não versionados).
- Instalador NSIS 0.3.0 gerado e cópia conferida por SHA256. O autoteste instalado
  foi implementado e conectado à CI, mas não foi executado localmente: o controle
  do computador foi interrompido pelo usuário com Esc antes da atualização da
  instalação 0.2.0. A instalação aberta não foi modificada. CI remota não executada
  nesta sessão.

## Matriz de homologação física

Antes de ampliar a distribuição, executar em Windows 10/11 e GPUs Intel/AMD/NVIDIA:
30 minutos de gravação, DPI 100/125/150%, dois monitores iguais, monitor secundário,
webcam, cursor, troca de headset, bloqueio/suspensão, mudança de resolução, disco
exFAT e retomada de upload. Mudanças de formato/resolução ainda encerram a sessão
com diagnóstico e preservação dos arquivos; recuperação transparente não foi
implementada. Essa matriz precisa de máquinas reais e não é comprovada pela CI.


## Recuperação de falhas de captura — 06/11/2026

- Falhas do dispositivo ou mudança de dimensões continuam encerrando a sessão,
  mas o motor tenta finalizar o vídeo já recebido. O relatório permanece failed,
  impedindo envio automático de gravação incompleta. Se o próprio encoder falhar,
  o parcial e o log continuam disponíveis; recuperação não é garantida nesse caso.
- A inspeção rápida de metadados usada pelo desktop tem limite de 30 segundos.
  A contagem completa de quadros da CLI tem limite de dez minutos, pois decodifica
  todo o arquivo. Ambos são separados do limite de finalização do encoder.
- Fechar os seletores pela janela nativa cancela a operação. O bloqueio de captura
  só é liberado após a tarefa de print encerrar sua limpeza.
- Falha ao enfileirar PNG com cópia local desativada preserva excepcionalmente o
  print na biblioteca e informa o caminho. Se a biblioteca estiver indisponível,
  informa o caminho temporário preservado. A fila reverte inclusão não persistida.
- A geometria da janela selecionada é acompanhada a cada 200 ms enquanto a sessão
  estiver ativa. Atualizações antigas não podem modificar uma sessão nova.
- O autoteste nativo agora salva um print, cancela seletores pelo evento real de
  fechamento e grava/move uma janela de teste. Clipboard, webcam física, áudio
  físico e múltiplos monitores ainda exigem homologação em máquinas reais.


## Autoteste nativo concluído — 06/11/2026

A compilação de desenvolvimento passou no autoteste gráfico com perfil isolado,
sem login nem upload. Resultado local: .tools/native-audit-4d9f083754744e5e8ce71bd8006a96a1/self-test-result.json,
status passed e código de saída 0.

Foram exercitados WebView/IPC, captura WGC de tela e janela, pausa/retomada,
movimentação da janela e atualização da geometria, cancelamento dos seletores pelo
evento nativo de fechamento, seleção/descarte de área e salvamento/listagem de PNG.
Os dois MP4s nativos terminaram completed, passaram nas checagens e não tiveram
quadros descartados pelo encoder nesta execução curta (2,529 s e 1,574 s).
Áudio/trilhas foram verificados com fontes sintéticas; isso não homologa áudio
físico, webcam física, clipboard, múltiplos monitores ou sessões prolongadas.

A execução revelou um bloqueio na inicialização: as consultas de monitor estavam
sob o mutex de sessão, que também é usado pelos comandos síncronos da interface.
As consultas agora ocorrem antes do bloqueio. O comando exclusivo do autoteste que
cria a janela de teste também passou a ser assíncrono para não bloquear a thread
que inicializa o WebView no Windows. O fluxo nativo reproduz e protege esses casos.

Essa validação não equivale à instalação de um novo NSIS: o instalador anterior
ainda precisa ser regenerado com os ajustes atuais.

## Área em qualquer monitor

Vídeo e print no Windows abrem um seletor transparente por monitor. O usuário
inicia pelo botão ou atalho e arrasta na tela desejada; o monitor escolhido vale
para aquela captura, sem alterar a preferência salva. Cada seletor converte o
arraste relativo à sua janela em pixels físicos locais. O backend associa a
região ao identificador do monitor e rejeita resolução alterada ou limites
inválidos. As janelas de seleção são fechadas antes da captura. O print captura
somente o monitor escolhido e abre o editor com a região pronta.

O modelo foi comparado ao seletor por monitor do Cap. Para Tela e Janela, a
proposta de UX é escolher sobre a área de trabalho: clicar na tela inteira ou
na janela destacada sob o cursor. Esses dois modos continuam com o fluxo atual
enquanto essa proposta é avaliada. A alternativa nativa é GraphicsCapturePicker,
que seleciona monitor/janela pelo diálogo do Windows; a área ainda precisaria
do seletor próprio. Não foi implementado um mosaico de todos os monitores nem
um recorte que atravesse duas telas.

## Contorno durante a gravação

Área e Janela mostram uma borda verde; na pausa, ela fica amarela e tracejada.
O contorno acompanha a posição e o tamanho da janela, some quando ela é
minimizada e reaparece ao restaurar. Ao parar, descartar ou falhar, o contorno
é ocultado. Ele usa uma janela transparente, sem foco e que deixa os cliques
passarem; a proteção de conteúdo solicita sua exclusão da captura do Windows.
A posição é medida em pixels físicos, incluindo monitores à esquerda/acima
da tela principal. O monitor inteiro mantém o comportamento anterior.

Se a versão do Windows não oferecer exclusão real do contorno, ele permanece
oculto para evitar um retângulo preto sobre o vídeo. A exclusão usa
WDA_EXCLUDEFROMCAPTURE, disponível a partir do Windows 10 versão 2004.

## Atalhos livres de teclado e mouse

Atalhos de teclado aceitam uma tecla isolada (F5, Q etc.) ou combinações com
modificadores. No Windows, os cinco botões padrão do mouse podem ser usados
sozinhos ou com Ctrl, Alt, Shift e Windows. O primeiro clique dá foco ao campo;
a próxima tecla ou botão define o atalho e encerra a captura do campo. Os
cliques do mouse continuam chegando ao aplicativo sob o cursor.

Durante a edição, os atalhos registrados são temporariamente removidos e
reativados ao terminar. Falhas de registro continuam retornando erro, com
tentativa de restaurar a configuração anterior. Botões são monitorados em
thread própria, sem executar comandos dentro do callback do hook. A edição
de atalhos só registra o que o usuário configura; nenhum padrão é imposto.

O autoteste usa uma janela própria, confirma F5, Q sem repetição, botão lateral
e Ctrl+botão central por SendInput, e restaura configuração e cursor ao sair.
Validação aprovada em 06/11/2026 no perfil isolado
`.tools/native-shortcuts-143d3e9673a9402b947d42c6aa8ec9f5`, saída 0.

A execução final também verificou que teclado e mouse deixam de disparar
durante a captura do campo e voltam a funcionar após a edição.

## Transição do print

No Windows com exclusão de captura disponível, o seletor de área do monitor
escolhido vira o editor. Ele permanece visível durante a captura, e o recorte
continua desenhado até a imagem congelada estar decodificada. Não há destruição
do seletor seguida pela abertura de outra janela, nem a espera fixa de 200 ms
entre as duas etapas. O fallback que oculta o seletor permanece quando o Windows
não pode excluí-lo da captura, para evitar que o overlay entre na imagem.

A janela principal só é restaurada quando já estava visível antes da captura.
Os prints iniciados com o app oculto terminam sem trazê-lo para frente. Se ele
estava aberto, é restaurado antes de retirar o editor de cima, evitando expor a
área de trabalho entre as janelas. Cancelar durante a preparação também cancela
o processamento e remove o seletor retido.

## Refatoração do caminho crítico de print

As otimizações anteriores não retiravam o custo de criar WebViews e iniciar
uma sessão de vídeo a cada print, nem a compressão da tela inteira antes do
editor. O fluxo residente passa a pré-carregar os seletores, reutilizá-los com
novas sessões e entregar os pixels por um protocolo de memória autorizado ao
editor atual. Fechar libera o frame e limpa a UI, mas preserva o seletor oculto.

No Windows, tela/área usam GDI para capturar uma imagem. O compositor é
sincronizado sem espera fixa de 200 ms. WGC permanece nas gravações e nas
capturas de janela, além do fallback de tela caso GDI falhe. Mudanças de
identidade, posição ou resolução do monitor são rejeitadas antes da captura.

A cópia simples de tela/janela e a exportação de área sem anotações têm caminho
nativo direto. Imagens anotadas usam canvas apenas do tamanho do recorte e
codificação assíncrona. PNG só é gerado na exportação; o envio pela internet
continua em segundo plano e depende da rede. A distribuição de teste é release
otimizada, para não confundir desempenho de debug com o uso normal.
