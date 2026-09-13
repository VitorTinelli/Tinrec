# Tinrec

Gravador para Windows 11 e biblioteca privada de vídeos, com compartilhamento por link e arquivos no Google Drive do proprietário.

**Estado: versão inicial para testes.** Aplicativo e site implementados; captura real e testes locais executados. A publicação foi recusada pela Vercel por bloqueio de uso justo da equipe. Google OAuth e Neon ainda precisam ser configurados. O teste prolongado revelou variação de memória e taxa efetiva de quadros: veja [VALIDATION.md](VALIDATION.md).

## Abrir o aplicativo

Execute `artifacts/Tinrec-Setup-0.1.0-x64.exe` para instalar ou extraia `artifacts/Tinrec-Windows-x64.zip` e abra `Tinrec.exe`.

O aplicativo inclui o runtime .NET. A captura requer Windows 11 x64, Visual C++ Redistributable x64 e Media Foundation. O instalador inclui o redistribuível Microsoft e só o executa quando não está instalado. Em edições Windows N, instale o Media Feature Pack nas configurações do Windows.

1. Em **Conta e preferências**, informe o endereço HTTPS do seu site.
2. Clique em **Conectar com Google**. No navegador, entre na conta proprietária e aprove o código exibido no aplicativo.
3. Escolha monitor, tela inteira ou área, 720p/1080p e dispositivos de áudio.
4. Grave, pause/retome e pare. O link é copiado quando disponível; deixe o aplicativo aberto para concluir o envio.

Sem conexão, a gravação fica no computador. O aplicativo retoma a fila ao abrir. Arquivos enviados são mantidos por sete dias; arquivos pendentes não são limpos automaticamente. **Arquivos locais** abre a pasta de gravações. Não exclua essa pasta enquanto houver envios pendentes.

O nome padrão usa prefixo e data/hora local. O título pode ser ajustado antes de gravar e renomeado na biblioteca depois. Alterar o nome não altera o link.

## Preparar Google, Neon e Vercel

### 1. Google Cloud

- Crie um projeto e habilite a **Google Drive API**.
- Configure a tela de consentimento OAuth para sua conta e os escopos `openid`, `email` e `https://www.googleapis.com/auth/drive.file`.
- Crie um cliente OAuth do tipo **Aplicativo da Web**.
- Cadastre exatamente `https://SEU-SITE/api/auth/callback` como URI de redirecionamento; para desenvolvimento, acrescente `http://localhost:5173/api/auth/callback`.
- Se o app estiver em modo de teste, inclua sua conta entre os usuários de teste. O Google pode expirar autorizações de aplicativos em teste; nesse caso, use **Reconectar Google**. Para uso duradouro, configure a publicação da tela de consentimento conforme o console Google.

O aplicativo solicita acesso somente aos arquivos criados/abertos pelo Tinrec. Ele cria a pasta `Tinrec` e compartilha cada vídeo como `anyone/reader`, sem tornar a pasta pública. Contas corporativas podem impedir essa permissão; o arquivo será preservado e o envio poderá ser tentado novamente após corrigir a política.

### 2. Neon

Crie um banco PostgreSQL Neon, preferencialmente pela integração do Marketplace da Vercel. Use a connection string com pooling e TLS como `DATABASE_URL`.

Na raiz do projeto, copie `.env.example` para `.env`, preencha as variáveis e execute:

```powershell
npm ci
npm run db:migrate
```

A migração cria as tabelas de proprietário, sessões, vínculos e gravações. É repetível e não remove dados existentes. Em produção, execute-a contra o banco de produção antes do primeiro login.

### 3. Variáveis de ambiente

| Variável | Valor |
| --- | --- |
| `APP_URL` | Origem exata do site, como `https://tinrec.exemplo.com` |
| `DATABASE_URL` | Connection string PostgreSQL Neon com TLS |
| `OWNER_EMAIL` | Único e-mail Google permitido nesta versão |
| `GOOGLE_CLIENT_ID` | ID do cliente OAuth Web |
| `GOOGLE_CLIENT_SECRET` | Segredo do cliente OAuth Web |
| `TOKEN_ENCRYPTION_KEY` | 32 bytes aleatórios em base64 |

Gere a chave uma vez e guarde-a com segurança:

```powershell
node -e "console.log(require('crypto').randomBytes(32).toString('base64'))"
```

Cadastre as seis variáveis na Vercel. **Não use prefixo `VITE_` para segredos.** Não versione `.env`. Trocar a chave invalida os refresh tokens e sessões de upload cifrados existentes; reconecte o Google após uma troca planejada.

### 4. Publicar

Após resolver o bloqueio da equipe Vercel:

```powershell
npx vercel login
npx vercel link
# Selecione/crie o projeto Tinrec na equipe correta.
npx vercel deploy --prod
```

A raiz do projeto é esta pasta. Framework: Vite; build: `npm run build`; saída: `dist`. O arquivo `api/index.ts` serve a API Node.js. `vercel.json` encaminha `/api/*` para a função e as páginas para a SPA. Não configure a pasta `desktop` como raiz do projeto.

O site abre em estado de configuração quando faltam variáveis. Isso não é uma biblioteca de demonstração: nenhum vídeo fictício é criado. Antes de compartilhar com clientes, teste uma gravação curta e abra seu link em janela anônima.

## Desenvolvimento e validação

Requisitos: Node.js 22+ e SDK .NET 8 para Windows. O SDK baixado durante esta implementação está em `.tools/dotnet`, ignorado pelo Git.

```powershell
npm ci
npm run dev            # http://localhost:5173, UI e API juntas
npm test               # testes de API com PostgreSQL PGlite isolado
npm run build
dotnet build desktop/Tinrec/Tinrec.csproj -p:Platform=x64
dotnet run --project desktop/Tinrec.Tests/Tinrec.Tests.csproj -p:Platform=x64
dotnet run --project desktop/Tinrec.Diagnostics/Tinrec.Diagnostics.csproj -p:Platform=x64 -- 8 artifacts/test.mp4 --pause
```

O diagnóstico abre uma janela própria e grava a área dela. Opções: `--720`, `--full`, `--mic`, `--system`, `--pause`. O primeiro argumento é a duração em segundos; o segundo é o MP4 de saída. Ele salva um relatório JSON com CPU, memória e contagem de callbacks de quadros. A contagem efetiva de quadros do arquivo deve ser medida com `ffprobe`; callbacks não equivalem necessariamente a quadros codificados.

Para gerar pacotes:

```powershell
./scripts/publish-windows.ps1 -Dotnet dotnet -Iscc 'C:/Program Files (x86)/Inno Setup 6/ISCC.exe'
```

O script gera o pacote portátil e, com Inno Setup disponível, o instalador. Esta entrega usa Inno Setup 6.4.3, cuja licença permite uso comercial. Obtenha o redistribuível oficial Microsoft em `https://aka.ms/vs/17/release/vc_redist.x64.exe` e salve-o em `.tools/vc_redist.x64.exe` antes de compilar o instalador. Os binários desta versão não têm assinatura digital de editor.

## Estrutura e comportamento

- `desktop/Tinrec`: WPF, Windows Graphics Capture, Media Foundation H.264/AAC via ScreenRecorderLib 7.0.1, seleção em pixels físicos, fila cifrada com DPAPI e upload em blocos de 8 MiB.
- `src`: biblioteca privada, busca, renomeação, vínculo do aplicativo e página pública com iframe do Drive.
- `server` e `api`: Express, PostgreSQL, OAuth com PKCE e cookie de estado, sessões hashadas, refresh tokens e URLs de upload cifrados por AES-256-GCM.
- `tests` e `desktop/Tinrec.Tests`: testes de acesso, compartilhamento, retomada e retenção de arquivos.

O MP4 nunca passa pela Vercel. A API reserva um identificador do Drive antes de criar o arquivo; retries usam o mesmo ID. O desktop consulta o offset confirmado pelo Drive e retoma o upload. A conclusão valida tamanho, MIME e vínculo do arquivo ao registro antes de torná-lo acessível por link. Uma página pública não retorna credenciais, identidade do proprietário ou a URL de upload.

O site é privado na biblioteca, mas o vídeo compartilhado é acessível a qualquer pessoa com o link, inclusive pelo Drive. `noindex` evita indexação voluntária, não funciona como controle de acesso. O player é o do Google Drive e o processamento pode levar minutos.

## Limites desta entrega

- Um proprietário por instalação do backend; Windows x64. Sem webcam, editor, transcrição, comentários ou cobrança.
- Não houve validação real Google OAuth → Drive → Vercel por falta das credenciais e bloqueio de publicação.
- O teste prolongado concluiu, mas não aprovou desempenho sustentado de 30 FPS nem estabilidade de memória privada. Não há promessa de reprodução instantânea.
- A gravação em múltiplos monitores com DPI diferentes exige validação no hardware correspondente. A seleção converte para pixels físicos e limita a área ao monitor escolhido.
- Uma interrupção abrupta durante a gravação pode deixar MP4 não finalizado; o arquivo é preservado e marcado para atenção, sem envio automático de um arquivo possivelmente corrompido.

Dependência de captura: [ScreenRecorderLib](https://github.com/sskodje/ScreenRecorderLib), licença MIT, incluída nos pacotes. APIs de upload: [Google Drive resumable uploads](https://developers.google.com/workspace/drive/api/guides/manage-uploads).
