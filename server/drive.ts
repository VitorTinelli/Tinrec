import type { Database } from "./db.js";
import { decrypt, encrypt, HttpError } from "./security.js";
export interface Drive {
  start(
    userId: string,
    recording: any,
    restart: boolean,
  ): Promise<{ uri: string; fileId: string }>;
  complete(userId: string, recording: any): Promise<void>;
  rename(userId: string, fileId: string, title: string): Promise<void>;
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
    if (!r.ok) {
      const e = await r.json().catch(() => ({}));
      const reason = e.error?.errors?.[0]?.reason;
      if (reason === "storageQuotaExceeded")
        throw new HttpError(
          409,
          "drive_full",
          "Seu Google Drive está sem espaço. Libere espaço e tente novamente.",
        );
      if (r.status === 401)
        throw new HttpError(
          409,
          "google_reconnect",
          "Reconecte sua conta Google.",
        );
      if (r.status === 404)
        throw new HttpError(
          409,
          "file_unavailable",
          "O arquivo não está disponível no Google Drive.",
        );
      throw new HttpError(
        409,
        "drive_error",
        "O Google Drive não concluiu a operação. Verifique as permissões e tente novamente.",
      );
    }
    return r;
  }
  return {
    async start(userId, r, restart) {
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
              name: r.title + ".mp4",
              parents: [folder],
              mimeType: "video/mp4",
              appProperties: { tinrecRecording: r.id },
            }),
            signal: AbortSignal.timeout(15000),
          },
        );
        if (!created.ok && created.status !== 409)
          throw new HttpError(
            409,
            "drive_error",
            "Não foi possível criar o arquivo no Drive.",
          );
      } else if (!exists.ok)
        throw new HttpError(
          409,
          "drive_error",
          "Não foi possível consultar o arquivo no Drive.",
        );
      const response = await request(
        `https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=resumable`,
        t,
        {
          method: "PATCH",
          headers: {
            "X-Upload-Content-Type": "video/mp4",
            "X-Upload-Content-Length": String(r.size_bytes),
          },
          body: JSON.stringify({ name: r.title + ".mp4" }),
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
        file.mimeType !== "video/mp4" ||
        Number(file.size) !== Number(r.size_bytes) ||
        file.appProperties?.tinrecRecording !== r.id
      )
        throw new HttpError(
          409,
          "upload_incomplete",
          "O envio ainda não foi concluído.",
        );
      try {
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
      } catch {
        throw new HttpError(
          409,
          "sharing_blocked",
          "O Drive bloqueou o compartilhamento por link. Verifique as políticas da sua conta e tente novamente.",
        );
      }
    },
    async rename(userId, fileId, title) {
      const { token: t } = await access(userId);
      await request(`https://www.googleapis.com/drive/v3/files/${fileId}`, t, {
        method: "PATCH",
        body: JSON.stringify({ name: title + ".mp4" }),
      });
    },
  };
}
