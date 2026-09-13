# Validação — 7 de setembro de 2026

## Executado

- Build React/TypeScript/Vite concluído.
- 11 testes de API com PostgreSQL PGlite passaram: configuração ausente, autenticação, proprietário permitido, reserva idempotente, dados públicos mínimos, origem de mutações via cookie, upload incompleto, tamanho e progresso, conclusão, códigos de vínculo de uso único/expirados e autenticação da criptografia.
- Site aberto em navegador automatizado em desktop e viewport 390×844. Biblioteca vazia, configurações e diálogo de instruções renderizados; sem erros JavaScript reportados.
- Aplicativo Windows compilado em x64 sem erros.
- Janela WPF real aberta com armazenamento de teste isolado; monitor e dispositivos de áudio carregados. Captura da própria interface salva em `artifacts/desktop-ui.png`. O ambiente de teste tem um monitor.
- Testes da fila desktop passaram: proteção DPAPI, persistência após reinício, consulta de offset, envio apenas de bytes restantes, recriação de sessão expirada, falha de rede, bloqueio de compartilhamento, backoff, nova tentativa e retenção segura dos arquivos.
- MP4 de área 1080p: H.264, 1920×1080, 30 FPS declarados, 8,009 s. Pausa de dois segundos excluída da duração final.
- MP4 de tela inteira 720p com áudio de sistema: H.264, 1280×720, vídeo 5,994 s; AAC estéreo 48 kHz, áudio 6,016 s.
- Captura curta de microfone executada e MP4 produzido. Presença de faixa não comprova, sozinha, audibilidade ou sincronização perceptiva.
- Captura curta com microfone e som do computador juntos: H.264 1920×1080, vídeo 6,000 s; AAC estéreo 48 kHz, áudio 5,995 s. As quatro combinações de áudio (sem áudio, sistema, microfone e ambos) produziram arquivos; seleção de dispositivos alternativos não foi homologada.

## Teste prolongado: resultado parcial, desempenho não aprovado

Gravação de 30 minutos da área de diagnóstico, com áudio do sistema, sem upload simultâneo. Artefatos em `artifacts/benchmark-1080-30min.mp4` e JSON correspondente.

| Medida | Resultado |
| --- | --- |
| Duração do vídeo | 1.800,009 s |
| Duração do áudio | 1.800,021 s |
| Resolução e codecs | 1920×1080, H.264 + AAC 48 kHz estéreo |
| Tamanho | 42.500.305 bytes |
| CPU média do processo, normalizada por núcleos lógicos | 0,188% |
| Working set na primeira amostra / última | 198 / 299 MiB |
| Pico de working set | 318 MiB |
| Memória privada na primeira amostra / última | 345 / 723 MiB |
| Pico de memória privada | 1.313 MiB |
| Quadros no arquivo, segundo ffprobe | 24.887 (~13,83 FPS médios) |
| Cadência nominal informada pelo arquivo | 30 FPS |

O arquivo foi finalizado e suas trilhas têm durações próximas. Entretanto, há intervalos com poucos quadros e oscilações expressivas de memória privada. O ensaio não permite atribuir a causa a captura, driver, sessão de desktop ou codificação. Portanto, **não comprova desempenho sustentado a 30 FPS nem memória estável**. A taxa nominal do cabeçalho não deve ser apresentada como taxa real observada.

Para homologação, repetir em uma sessão Windows ativa e controlada, com padrão em movimento contínuo, telemetria de perdas da captura e do encoder, GPU identificada, microfone/loopback audíveis e comparação hardware/software. O aplicativo usa buffers de upload limitados, mas isso não garante a memória interna da biblioteca nativa.

## Pendências externas

- Publicação Vercel recusada com HTTP 402, `resource_creation_blocked`: equipe bloqueada por exceder limites de uso justo. Nenhuma URL de produção entregue.
- Credenciais Google OAuth e banco Neon não fornecidas/configuradas; teste real de upload e player anônimo não realizado.
- Seleção entre monitores com DPI diferentes, matriz completa de dispositivos de áudio e sincronização perceptiva ainda precisam de validação em hardware apropriado.
- Não foi feita instalação em uma máquina Windows limpa. O pacote inclui runtime .NET e instalador do Visual C++ Redistributable.
