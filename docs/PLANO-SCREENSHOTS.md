# Plano: captura de screenshots no Tinrec

Status: implementação concluída em 02/11/2026; validação física dos seletores e do clipboard ainda necessária por sistema.
Base: estado atual do aplicativo Rust/Tauri, incluindo alterações locais pendentes.

## Resultado proposto

Adicionar a opção **Print** ao lado de **Vídeo**, reaproveitando Tela, Janela e
Área. Capturar em resolução original e abrir automaticamente um editor local,
com ferramentas equivalentes às de edição documentadas no FireShot Pro.

Fluxo solicitado: **capturar → editar → Copiar imagem / Salvar / Enviar ao site**.
Copiar e enviar são ações explícitas e sempre usam a composição final editada.
Não ligar microfone, áudio ou webcam para prints. Seleção de área termina ao
soltar o mouse; o editor posterior não é uma segunda seleção de captura.

O editor e o envio de prints ao site passam a fazer parte do escopo solicitado,
substituindo a proposta anterior de uma primeira entrega sem edição/upload.
As etapas abaixo são incrementos de implementação, não exclusões desse escopo.
OCR, captura com rolagem de páginas, captura de todas as abas, integrações com
serviços externos do FireShot e print durante gravação são recursos distintos,
fora deste planejamento inicial. Não copiar a identidade visual do FireShot;
manter os componentes e a aparência do Tinrec.

## Revisão inicial do código e implicações (histórico)

1. `desktop-rust/src/recording.rs`: já cria uma CaptureSession, recebe VideoFrame
   e converte o buffer com to_tight_bytes. Mas run_inner também configura áudio,
   relógio, encoder e relatório de vídeo. Não usar uma gravação curta para gerar
   print: criar um serviço de captura de um quadro sem FFmpeg ou áudio.
2. `desktop-app/src-tauri/src/main.rs`: concentra sessão, seleção, arquivos e
   comandos Tauri. Implementar screenshots em módulo próprio, evitando aumentar
   esse arquivo e compartilhar por acidente o estado de gravação/upload.
3. `desktop-app/src/AreaSelector.tsx`: geometria normalizada e conclusão ao
   soltar o mouse são reaproveitáveis. O componente está acoplado a Snapshot,
   recorder-state, get_preview e select_area. Separar a interação visual do
   transporte específico de vídeo antes de usá-lo para prints.
4. `desktop-app/src-tauri/src/main.rs`: recording_path e list_recordings aceitam
   somente MP4. `RecordingLibrary.tsx` pressupõe duração, reprodução e links de
   vídeo. Apenas salvar um PNG não o faria aparecer corretamente na biblioteca.
5. `desktop-app/src-tauri/Cargo.toml`: image está habilitado somente com jpeg;
   habilitar png para gravação e leitura de screenshots. Clipboard já está
   instalado e sua API Rust oferece write_image; não é necessário outro plugin.
6. `desktop-app/src-tauri/src/cloud.rs` e `server/drive.ts`: upload usa video/mp4,
   nomes .mp4 e validação de mídia de vídeo. `server/temporary.ts` e o player
   também são específicos para vídeo. PNG não deve entrar nessa fila sem um
   contrato próprio de imagem.

## Captura por plataforma

| Sistema | Caminho proposto | Seleção de área |
| --- | --- | --- |
| Linux Wayland | XDG Screenshot Portal via ashpd já presente | Usar a interação do compositor uma única vez; não abrir outro seletor do Tinrec |
| Linux X11 | Um quadro do backend nativo, sem encoder | Recortar o quadro original com a seleção do Tinrec |
| Windows | Um quadro da captura nativa existente | Seleção do Tinrec, com coordenadas físicas e DPI corretos |
| macOS | Um quadro do backend ScreenCaptureKit existente | Seleção do Tinrec, respeitando escala Retina e monitor escolhido |

No Wayland, screenshot e screencast são interfaces distintas. A API Screenshot
retorna uma URI de imagem e admite interação do sistema; o aplicativo precisa
ler a imagem resultante, não tratar a URI como um caminho qualquer. A ashpd
instalada oferece interactive e consulta de available_targets, mas a versão e
as capacidades do portal em execução precisam ser verificadas. Se o compositor
não permitir impor Tela/Janela/Área separadamente, apresentar o seletor nativo
sem prometer que o cartão força aquele tipo. Não repetir um recorte já feito
pelo sistema nem alegar identificar o tipo quando o portal não o informar.

Referência primária: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html

## Etapas de implementação

### 1. Contrato e estados

- Definir ScreenshotRequest, resultado com identificador, dimensões e arquivo,
  e estados próprios: idle, selecting, capturing, editing, exporting, uploading,
  completed, cancelled, failed. Cancelamento normal não é erro.
- Introduzir serviço screenshot no Rust, com captura de quadro e codificação
  PNG. Compartilhar apenas auxiliares de fonte/buffer que realmente coincidam.
- Na primeira versão, impedir screenshot enquanto há gravação ou outra captura
  em andamento; não interromper silenciosamente uma gravação. Captura simultânea
  exige validação adicional e pode ser adicionada depois.
- Usar dimensões reais da fonte, sem aplicar a resolução 720p/1080p do vídeo.

### 2. Backends e seleção

- Implementar primeiro o portal de screenshot Wayland e um backend de quadro
  único para Windows/macOS/X11. Definir timeout, liberação da CaptureSession em
  qualquer saída e tratamento de permissão negada.
- Separar o seletor visual do estado/comando de vídeo; preservar arraste inverso,
  Esc, início ao soltar e proteção contra eventos duplicados.
- Ocultar as janelas relevantes do Tinrec antes da captura e restaurar somente
  as que estavam abertas. Não confiar apenas em contentProtected no Linux.
- Evitar usar a prévia JPEG reduzida como imagem final. Aplicar recorte no buffer
  original, respeitando stride, ordem BGRA/RGBA e escala física/lógica.
- Tratar a URI retornada pelo portal com conversão de URI e acesso permitido;
  só remover temporários que pertençam à operação e possam ser removidos com
  segurança. Não apagar um arquivo de origem arbitrário.

### 3. Persistência e clipboard

- Captura original fica como rascunho local privado, fora da biblioteca de
  arquivos concluídos. Salvar explicitamente a composição final em subpasta
  Prints, com nome como `Print - 02-11-2026 às 18h30m00s.png` e identificador
  contra colisões. Cancelar antes de exportar elimina os temporários da sessão.
- Criar arquivo temporário exclusivo e publicar por rename após PNG completo;
  não sobrescrever arquivos nem deixar imagem parcial na biblioteca.
- Oferecer Copiar imagem no editor; clipboard recebe os pixels finais,
  não apenas o caminho. Se copiar falhar, preservar o PNG e informar a falha.
- Definir política específica de rascunhos e cópias locais de prints. Não apagar
  rascunho com alterações não exportadas por causa da política de vídeos.
  Após envio confirmado, respeitar a preferência de manter cópia final local;
  falha no upload preserva a composição para nova tentativa.

### 4. Interface e biblioteca

- Seletor Vídeo/Print no gravador. Em Print, esconder controles de áudio/webcam
  e campos exclusivos de vídeo sem alterar as preferências do usuário.
- Adicionar atalho configurável Tirar print, integrado aos mecanismos existentes
  de Windows/macOS/X11 e ao portal de atalhos do Wayland.
- Evoluir o contrato de arquivo para discriminar video/image e incluir dimensões.
  Usar filtros Vídeos/Prints na biblioteca; imagem abre visualizador e não player.
- Gerar miniaturas de imagem em Rust com tamanho limitado e cache; não enviar
  PNG 4K inteiro em base64 para montar todos os cartões.
- Generalizar validação de caminhos com extensões permitidas, mantendo a
  restrição à pasta configurada e a rejeição de caminhos externos/symlinks.

### 5. Compartilhamento da imagem editada

- Acrescentar tipo de mídia, image/png, dimensões e extensão aos contratos de
  cliente, API, banco e Drive; criar visualização de imagem no site.
- Definir se haverá cópia temporária R2 para imagens. Prints não precisam esperar
  o processamento de vídeo do Drive, mas autorização, disponibilidade e cleanup
  ainda precisam de regras próprias.
- Preservar compatibilidade dos vídeos existentes e nunca montar um player MP4
  para um print. Só oferecer Abrir no site quando existir upload de imagem válido.

## Validação necessária

Automatizada:
- Buffer conhecido com padding/stride, BGRA/RGBA, recorte nas bordas, arraste
  inverso, DPI fracionado, PNG decodificado com dimensões/cores esperadas.
- Primeira seleção já captura; não exigir segundo arraste/confirmação.
- Cancelamento/timeout/erro liberam a sessão e restauram as janelas corretamente.
- PNG incompleto não aparece; nomes não colidem; acesso por caminho externo é
  rejeitado; erro do clipboard não remove arquivo salvo.
- Biblioteca distingue imagem/vídeo e preserva filtros, busca e abertura de MP4.
- Testes Linux garantem que o portal de screenshot não abre seletor adicional.

Manual, em cada plataforma:
- KDE/GNOME Wayland, X11, Windows e macOS; monitor principal/secundário e
  múltiplas escalas; permissões, cancelamento, cores e cursor.
- Confirmar que janela principal, mini menu e webcam do Tinrec não contaminam o
  print. Verificar conteúdo da imagem real, não apenas sucesso do comando.
- Copiar e colar em outro aplicativo e abrir PNG salvo.

## Critério de conclusão da funcionalidade

Uma ação ou atalho captura sem seleção repetida e abre o editor. As ferramentas
listadas abaixo funcionam e o usuário consegue copiar, salvar e enviar ao site
a composição final. A imagem final aparece na biblioteca e abre corretamente
no site; copiar e enviar preservam o mesmo conteúdo. Cancelar antes de exportar
não cria arquivo final ou upload. Vídeo, áudio, webcam e compartilhamento já existentes
continuam funcionando. O recurso é verificado nos sistemas alvo; compilação
cruzada ou teste sintético isolado não substitui essa validação manual.

## Editor solicitado: referência e cobertura

Referências oficiais consultadas em 02/11/2026:
- [FireShot: editor e ferramentas](https://getfireshot.com/using.php#editing)
- [FireShot: visão geral](https://getfireshot.com/)

A documentação do FireShot serve como referência funcional do editor. Há
variações de edição/versão do produto; a matriz abaixo define a cobertura do
Tinrec, sem prometer equivalência com funcionalidades não documentadas.

| Grupo | Ferramentas a implementar |
| --- | --- |
| Seleção | Selecionar região e objetos; mover, redimensionar e excluir anotações |
| Anotação | Retângulo, elipse, linha, seta, desenho livre, texto e chamadas com texto |
| Estilo | Cor, preenchimento, contorno, espessura, fonte e tamanho de texto |
| Composição | Inserir imagem, destacar trechos, apagar/preencher uma região |
| Imagem | Recortar, redimensionar e selecionar cor com conta-gotas |
| Efeitos | Desfoque gaussiano, escala de cinza, inversão de cores, bordas luminosas e entrelaçamento |
| Histórico | Desfazer/refazer alterações e continuar editando os objetos |
| Personalização | Marca-d’água, cabeçalho e rodapé configuráveis |
| Projeto | Salvar e reabrir rascunho editável local |

Zoom, ajuste à janela, navegação da imagem e atalhos de seleção/exclusão são
requisitos de usabilidade do Tinrec. Cobertura adicional de ocultação sólida
é obrigatória: desfoque é um efeito visual e não deve ser apresentado como
remoção garantida de informação sensível. Não incluir numeração, OCR ou outras
ferramentas como se fossem equivalência documentada com o FireShot.

## Arquitetura e interação do editor

- Abrir uma superfície própria de edição no Tauri, sem misturar o editor com a
  janela transparente de seleção nem com o mini menu da gravação.
- Barra compacta de ferramentas, propriedades da ferramenta/objeto selecionado,
  canvas central e ações **Copiar imagem**, **Salvar** e **Enviar ao site**.
  Nenhum upload ou cópia automática ao terminar de selecionar a área.
- Modelo de documento: imagem base + objetos de anotação + operações de imagem
  + histórico. Coordenadas em pixels da imagem, independentes do zoom/DPI.
- Separar estado do documento, interação e renderização em módulos próprios.
  Fazer um protótipo técnico para escolher canvas de objetos ou biblioteca,
  comparando compatibilidade WebKitGTK/WebView2/WKWebView, licença, texto,
  seleção, memória e fidelidade da exportação. Nenhuma dependência escolhida
  apenas por facilidade de montar uma demonstração.
- Preservar anotações editáveis enquanto o documento estiver aberto; não
  regravar a imagem base a cada movimento. Histórico baseado em comandos ou
  patches, com checkpoints limitados para operações raster; não duplicar um
  bitmap 4K completo para cada movimento do mouse.
- Recorte, redimensionamento, efeitos e objetos precisam de ordem definida.
  Transformar coordenadas e limites de forma consistente; texto/objetos fora
  do recorte não podem reaparecer na exportação.
- Efeitos aplicados à seleção precisam alterar os pixels exportados, e não só
  a aparência CSS do preview. Preenchimento de ocultação deve ser opaco.
- Exportar uma versão congelada do documento, sem alças de seleção, cursores ou
  barras do editor. Fonte e imagem devem estar carregadas antes de exportar.
  Copiar, salvar e enviar usam a mesma composição PNG final.
- Se o documento mudar durante upload, não alterar silenciosamente a imagem já
  enviada. Associar a operação a uma revisão e oferecer novo envio explícito.
- Rascunho editável é um formato local próprio com versão do schema. Pode
  conter a imagem original; portanto, não o enviar no lugar do PNG final nem
  disponibilizar originais no site. Validar imagens e limites na importação.
- Falha ao copiar ou enviar mantém a edição aberta. Fechar um documento com
  alterações não exportadas permite salvar rascunho ou descartar. Depois de
  Copiar, o editor não deve obrigar a enviar/salvar uma segunda vez.

## Ordem de execução atualizada

1. Captura e contrato de imagens, com prova do fluxo Wayland sem seleção dupla.
2. Documento de edição, canvas, seleção de objetos e desfazer/refazer.
3. Anotações e estilos; recorte e redimensionamento.
4. Efeitos raster, imagens inseridas e personalização; rascunhos locais.
5. Renderização final compartilhada, PNG e clipboard.
6. Biblioteca de imagens e upload/visualizador de PNG no site/Drive.
7. Validação integrada nos três sistemas e empacotamento.

Os passos são verificáveis individualmente; a funcionalidade solicitada não
está concluída apenas com a captura ou com um editor que não exporta efeitos.

## Testes adicionais do editor

- Confirmar cada ferramenta da matriz sobre imagens pequenas e 4K, incluindo
  seleções sobrepostas, zoom, DPI fracionado, bordas e texto multilinha.
- Após desenhar, editar propriedades, recortar e desfazer/refazer, comparar o
  PNG com o resultado esperado. Incluir ordem de camadas e transformações.
- Verificar a igualdade da imagem produzida para clipboard, arquivo e upload.
  Não basta comparar thumbnails ou o DOM do editor.
- Testar ocultação sólida, desfoque e recorte no arquivo decodificado e no PNG
  baixado pelo link do site. Originais, histórico e rascunhos não acompanham
  o upload nem as miniaturas públicas.
- Recuperação de rascunho e cancelamento; upload que falha e é retomado;
  exportação enquanto o documento muda; clipboard ocupado/indisponível.
- Medir memória/latência de pincel e histórico, com limite definido antes da
  aprovação do protótipo técnico; não assumir que todo canvas funciona igual
  nas três implementações de WebView.
