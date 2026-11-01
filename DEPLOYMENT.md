# Publicação do Tinrec

- Site: https://tinrec.vercel.app
- Equipe Vercel: `vitor-tinelli`
- Projeto: `tinrec` (`prj_7HusAoYTutfyMAyavvhuHgTWWIFt`)
- Node.js: 22.x; framework Vite; API Express em `api/index.ts`.
- Banco: Neon `tinrec-db`, plano Free, região `iad1`, conectado ao projeto.
- Migração inicial executada em 2026-10-31.
- `APP_URL` e `TOKEN_ENCRYPTION_KEY` configurados em produção. A chave é Secret.

O acesso de produção é público, autorizado pelo usuário e validado sem autenticação em 2026-10-31. Os previews permanecem protegidos pela autenticação da Vercel (`deploymentType: preview`). A biblioteca continua exigindo login Google no aplicativo.

## Configurar Google OAuth

1. Crie um projeto no [Google Cloud Console](https://console.cloud.google.com/) e habilite a **Google Drive API**.
2. Em **Google Auth Platform**, configure a marca Tinrec e o público **Externo**. Durante os testes, adicione as contas que vão testar em **Usuários de teste**.
3. Configure os escopos `openid`, `email` e `https://www.googleapis.com/auth/drive.file`.
4. Em **Clientes**, crie um cliente do tipo **Aplicativo da Web** e cadastre este URI de redirecionamento exato:

   `https://tinrec.vercel.app/api/auth/callback`

5. Nas [variáveis do projeto Vercel](https://vercel.com/vitor-tinelli/tinrec/settings/environment-variables), cadastre em **Production** `GOOGLE_CLIENT_ID` e `GOOGLE_CLIENT_SECRET`. Cadastre o segredo como Secret, sem prefixo `VITE_`. Não envie o segredo no chat nem o versione.
6. Publique novamente com `vercel deploy --prod --scope vitor-tinelli`.
7. Valide login de duas contas, isolamento da biblioteca, aprovação do desktop, envio e abertura do link em uma janela anônima.

Cada usuário autoriza sua própria conta Google. As credenciais OAuth identificam o aplicativo e não escolhem o proprietário dos vídeos.

Até configurar o cliente OAuth, `/api/health` retorna `configured: false` e as operações autenticadas permanecem indisponíveis.

Referências: [credenciais Google](https://developers.google.com/workspace/guides/create-credentials), [OAuth para aplicações Web](https://developers.google.com/identity/protocols/oauth2/web-server).

## Reprodução temporária no R2

Implementada em 2026-10-31. Bucket privado `tinrec` na conta Cloudflare configurada,
com credenciais S3 apenas em Production. O desktop solicita URLs PUT assinadas
por parte (8 MiB, validade 15 minutos) e envia diretamente para o R2. A API lista
os blocos confirmados e verifica o tamanho final antes de publicar a URL GET.
O player usa URLs GET de uma hora e preserva a posição ao renová-las. O arquivo
H.264/AAC já usa `faststart`; o R2 serve o MP4, sem transcodificar.

Depois do R2, o desktop continua o envio para o Drive do próprio usuário. A cópia
temporária continua reproduzível mesmo se o envio ao Drive falhar. Para vídeos
antigos, use **Preparar reprodução rápida** no desktop atualizado; o MP4 precisa
continuar no caminho local salvo na fila.

O dono do vídeo, conectado ao site, pode clicar em **Testar player do Drive** e
**Assisti no Drive; confirmar reprodução**. A API revalida a cópia permanente,
passa o player ao Drive e agenda a remoção do objeto R2 para depois de 24 horas.
A API não usa tamanho, metadados ou carregamento do iframe como prova de reprodução.
Sem confirmação, o temporário é mantido. O MP4 local e o arquivo do Drive não são
apagados por essa rotina.

A limpeza ocorre no acesso ao link ou no cron diário das 04:00 UTC. Por isso,
a remoção por cron pode acontecer até um dia após o prazo. `CRON_SECRET` está
configurado como Secret; `/api/cron/temporary-cleanup` e o diagnóstico interno
`/api/cron/r2-check` exigem esse segredo. O diagnóstico cria um objeto pequeno
com nome aleatório em `diagnostics/`, testa leitura Range e o remove imediatamente.

Limites iniciais: 2 GiB por vídeo temporário e 8 GiB reservados no total. Ao atingir
um limite, novos envios seguem apenas para o Drive, com aviso no desktop. Isso
limita o armazenamento de vídeos gerenciado pelo app, não constitui teto de
cobrança da conta Cloudflare nem limita operações ou outros objetos do bucket.
Acompanhe o uso no painel R2. Recomenda-se configurar a regra do bucket para
abortar uploads multipart incompletos após um dia; não configure expiração geral
dos MP4, pois ela poderia remover cópias ainda necessárias.

Validação real de produção: upload multipart, HEAD, GET Range 206 e exclusão do
objeto de diagnóstico passaram. O teste não substitui reproduzir uma gravação
real enviada pelo desktop e confirmar a troca para o Drive.
