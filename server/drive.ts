import type { Database } from "./db.js";
import { decrypt, encrypt, HttpError } from "./security.js";
export interface Drive {
  start(
    userId: string,
    recording: any,
    restart: boolean,
  ): Promise<{ uri: string; fileId: string }>;
  complete(userId: string, recording: any): Promise<void>;
  rename(
    userId: string,
    fileId: string,
    title: string,
    mediaType?: string,
  ): Promise<void>;
}
export function driveFailure(status: number, body: any): HttpError {
  const reasons = [
    body?.error?.errors?.[0]?.reason,
    ...(Array.isArray(body?.error?.details)
      ? body.error.details.map((d: any) => d?.reason)
      : []),
  ];
  if (
    reasons.some((r) =>
      ["accessNotConfigured", "SERVICE_DISABLED", "serviceDisabled"].includes(
        r,
      ),
    )
  )
    return new HttpError(
      409,
      "drive_api_disabled",
      "A Google Drive API está desativada no projeto Google Cloud do Tinrec. Ative a API no mesmo projeto do cliente OAuth e tente novamente.",
    );
  if (
    status === 401 ||
    reasons.some((r) =>
      ["insufficientPermissions", "ACCESS_TOKEN_SCOPE_INSUFFICIENT"].includes(
        r,
      ),
    )
  )
    return new HttpError(
      409,
      "google_reconnect",
      "Reconecte sua conta Google no site e autorize o acesso aos arquivos criados pelo Tinrec. Depois tente o envio novamente.",
    );
  if (reasons.includes("storageQuotaExceeded"))
    return new HttpError(
      409,
      "drive_full",
      "Seu Google Drive está sem espaço. Libere espaço e tente novamente.",
    );
  if (
    reasons.some((r) =>
      [
        "rateLimitExceeded",
        "userRateLimitExceeded",
        "dailyLimitExceeded",
      ].includes(r),
    ) ||
    status === 429
  )
    return new HttpError(
      503,
      "drive_rate_limit",
      "O Google Drive atingiu um limite temporário. O envio será tentado novamente.",
    );
  if (reasons.includes("domainPolicy"))
    return new HttpError(
      409,
      "drive_policy",
      "A organização da sua conta Google bloqueou o acesso ao Drive por aplicativos. Peça ao administrador para permitir o Tinrec.",
    );
  if (status === 404)
    return new HttpError(
      409,
      "file_unavailable",
      "O arquivo não está disponível no Google Drive.",
    );
  return new HttpError(
    409,
    "drive_error",
    "O Google Drive não concluiu a operação. Verifique as permissões e tente novamente.",
  );
}
async function verifyDriveResponse(r: Response, operation: string) {
  if (r.ok) return;
  const body = await r.json().catch(() => ({}));
  const error = driveFailure(r.status, body);
  const reason =
    body?.error?.errors?.[0]?.reason ??
    body?.error?.details?.find((d: any) => d?.reason)?.reason;
  // Never log tokens, request/response bodies, file IDs or resumable upload URLs.
  console.warn("Tinrec Drive failure", {
    operation,
    status: r.status,
    code: error.code,
    reason:
      typeof reason === "string" && /^[A-Za-z0-9_]{1,80}$/.test(reason)
        ? reason
        : "unknown",
  });
  throw error;
}
export function googleDrive(db: Database): Drive {
  async function access(userId: string) {
    const {
      rows: [u],
    } = await db.query("SELECT * FROM users WHERE id=$1", [userId]);
    if (!u?.refresh_token)
      throw new HttpError(
        409,
        "google_reconnect",
        "Reconecte sua conta Google para continuar.",
      );
    const r = await fetch("https://oauth2.googleapis.com/token", {
      method: "POST",
      body: new URLSearchParams({
        client_id: process.env.GOOGLE_CLIENT_ID!,
        client_secret: process.env.GOOGLE_CLIENT_SECRET!,
        refresh_token: decrypt(u.refresh_token),
        grant_type: "refresh_token",
      }),
      signal: AbortSignal.timeout(15000),
    });
    if (!r.ok)
      throw new HttpError(
        409,
        "google_reconnect",
        "Reconecte sua conta Google para continuar.",
      );
    return { user: u, token: (await r.json()).access_token as string };
  }
  async function request(url: string, t: string, init: RequestInit = {}) {
    const r = await fetch(url, {
      ...init,
      headers: {
        Authorization: `Bearer ${t}`,
        "Content-Type": "application/json",
        ...init.headers,
      },
      signal: AbortSignal.timeout(20000),
    });
    const path = new URL(url).pathname;
    const operation = path.endsWith("/generateIds")
      ? "generate-id"
      : path.endsWith("/permissions")
        ? "share-file"
        : path.startsWith("/upload/")
          ? "start-upload"
          : `${init.method ?? "GET"}-files`;
    await verifyDriveResponse(r, operation);
    return r;
  }
  return {
    async start(userId, r, restart) {
      const mime = r.media_type === "image" ? "image/png" : "video/mp4";
      const extension = r.media_type === "image" ? ".png" : ".mp4";
      if (r.upload_uri && !restart)
        return { uri: decrypt(r.upload_uri), fileId: r.drive_file_id };
      const { user, token: t } = await access(userId);
      let folder = user.drive_folder_id;
      if (!folder) {
        const found = await (
          await request(
            "https://www.googleapis.com/drive/v3/files?q=" +
              encodeURIComponent(
                "trashed=false and mimeType='application/vnd.google-apps.folder' and appProperties has { key='tinrec' and value='root' }",
              ) +
              "&fields=files(id)",
            t,
          )
        ).json();
        folder = found.files?.[0]?.id;
        if (!folder) {
          const f = await (
            await request("https://www.googleapis.com/drive/v3/files", t, {
              method: "POST",
              body: JSON.stringify({
                name: "Tinrec",
                mimeType: "application/vnd.google-apps.folder",
                appProperties: { tinrec: "root" },
              }),
            })
          ).json();
          folder = f.id;
        }
        await db.query("UPDATE users SET drive_folder_id=$1 WHERE id=$2", [
          folder,
          userId,
        ]);
      }
      let fileId = r.drive_file_id;
      if (!fileId) {
        const ids = await (
          await request(
            "https://www.googleapis.com/drive/v3/files/generateIds?count=1&space=drive&type=files",
            t,
          )
        ).json();
        // Persist the generated ID before creating the file. A retry always uses this same ID.
        const {
          rows: [saved],
        } = await db.query(
          "UPDATE recordings SET drive_file_id=COALESCE(drive_file_id,$1) WHERE id=$2 RETURNING drive_file_id",
          [ids.ids[0], r.id],
        );
        fileId = saved.drive_file_id;
      }
      const exists = await fetch(
        `https://www.googleapis.com/drive/v3/files/${fileId}?fields=id`,
        {
          headers: { Authorization: `Bearer ${t}` },
          signal: AbortSignal.timeout(15000),
        },
      );
      if (exists.status === 404) {
        const created = await fetch(
          "https://www.googleapis.com/drive/v3/files",
          {
            method: "POST",
            headers: {
              Authorization: `Bearer ${t}`,
              "Content-Type": "application/json",
            },
            body: JSON.stringify({
              id: fileId,
              name: r.title + extension,
              parents: [folder],
              mimeType: mime,
              appProperties: { tinrecRecording: r.id },
            }),
            signal: AbortSignal.timeout(15000),
          },
        );
        if (created.status !== 409)
          await verifyDriveResponse(created, "create-file");
      } else await verifyDriveResponse(exists, "lookup-file");
      const response = await request(
        `https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=resumable`,
        t,
        {
          method: "PATCH",
          headers: {
            "X-Upload-Content-Type": mime,
            "X-Upload-Content-Length": String(r.size_bytes),
          },
          body: JSON.stringify({ name: r.title + extension }),
        },
      );
      const uri = response.headers.get("location");
      if (!uri || new URL(uri).hostname !== "www.googleapis.com")
        throw new Error("Invalid upload session");
      await db.query(
        "UPDATE recordings SET upload_uri=$1,status='uploading',error_code=NULL WHERE id=$2",
        [encrypt(uri), r.id],
      );
      return { uri, fileId };
    },
    async complete(userId, r) {
      const { token: t } = await access(userId);
      const file = await (
        await request(
          `https://www.googleapis.com/drive/v3/files/${r.drive_file_id}?fields=id,size,mimeType,trashed,appProperties`,
          t,
        )
      ).json();
      if (
        file.trashed ||
        file.mimeType !==
          (r.media_type === "image" ? "image/png" : "video/mp4") ||
        Number(file.size) !== Number(r.size_bytes) ||
        file.appProperties?.tinrecRecording !== r.id
      )
        throw new HttpError(
          409,
          "upload_incomplete",
          "O envio ainda não foi concluído.",
        );
      try {
        const permissions = await (
          await request(
            `https://www.googleapis.com/drive/v3/files/${r.drive_file_id}/permissions?fields=permissions(type,role)`,
            t,
          )
        ).json();
        if (
          permissions.permissions?.some(
            (p: any) =>
              p.type === "anyone" && ["reader", "writer"].includes(p.role),
          )
        )
          return;
        await request(
          `https://www.googleapis.com/drive/v3/files/${r.drive_file_id}/permissions`,
          t,
          {
            method: "POST",
            body: JSON.stringify({
              type: "anyone",
              role: "reader",
              allowFileDiscovery: false,
            }),
          },
        );
      } catch (error) {
        if (error instanceof HttpError && error.code !== "drive_error")
          throw error;
        throw new HttpError(
          409,
          "sharing_blocked",
          "O Drive bloqueou o compartilhamento por link. Verifique as políticas da sua conta e tente novamente.",
        );
      }
    },
    async rename(userId, fileId, title, mediaType) {
      const { token: t } = await access(userId);
      await request(`https://www.googleapis.com/drive/v3/files/${fileId}`, t, {
        method: "PATCH",
        body: JSON.stringify({
          name: title + (mediaType === "image" ? ".png" : ".mp4"),
        }),
      });
    },
  };
}
