import express from "express";
import {
  r2Storage,
  temporaryVideos,
  type TemporaryStorage,
} from "./temporary.js";
import { createHash } from "node:crypto";
import { z } from "zod";
import type { Database } from "./db.js";
import { db as realDb } from "./db.js";
import { googleDrive, type Drive } from "./drive.js";
import {
  baseUrl,
  configReady,
  encrypt,
  hash,
  HttpError,
  same,
  token,
} from "./security.js";

function cookies(req: express.Request) {
  return Object.fromEntries(
    (req.headers.cookie ?? "")
      .split(";")
      .filter((x) => x.includes("="))
      .map((x) => {
        const i = x.indexOf("=");
        return [x.slice(0, i).trim(), x.slice(i + 1)];
      }),
  );
}
const title = z.string().trim().min(1).max(180);
const uuid = z.string().uuid();
const view = (r: any) => ({
  id: r.id,
  publicId: r.public_id,
  title: r.title,
  createdAt: r.created_at,
  durationMs: Number(r.duration_ms),
  resolution: r.resolution,
  mediaType: r.media_type ?? "video",
  status: r.status,
  sizeBytes: Number(r.size_bytes),
  uploadedBytes: Number(r.uploaded_bytes),
  errorCode: r.error_code,
  url: `${baseUrl()}/v/${r.public_id}`,
});

export function createApp(
  db: Database = realDb,
  drive: Drive = googleDrive(db),
  storage: TemporaryStorage = r2Storage(),
) {
  const app = express();
  const temporary = temporaryVideos(db, storage);
  app.disable("x-powered-by");
  app.use(express.json({ limit: "16kb" }));
  app.use("/api", (_req, res, next) => {
    res.set({
      "Cache-Control": "no-store",
      "X-Robots-Tag": "noindex, nofollow",
    });
    next();
  });
  app.get("/api/health", (_req, res) =>
    res.json({
      configured: configReady(),
      name: "Tinrec",
      r2Configured: storage.ready(),
    }),
  );
  app.use("/api", (_req, _res, next) => {
    if (!configReady())
      throw new HttpError(
        503,
        "setup_required",
        "O Tinrec está aguardando a configuração da conta Google e do banco de dados.",
      );
    next();
  });
  async function auth(req: express.Request, webOnly = false) {
    const bearer = req.headers.authorization?.match(
      /^Bearer ([A-Za-z0-9_-]+)$/,
    )?.[1];
    const raw = bearer ?? cookies(req).tinrec_session;
    if (!raw)
      throw new HttpError(401, "unauthorized", "Entre com sua conta Google.");
    const {
      rows: [s],
    } = await db.query(
      "SELECT s.*,u.email FROM sessions s JOIN users u ON u.id=s.user_id WHERE token_hash=$1 AND expires_at>now()",
      [hash(raw)],
    );
    if (
      !s ||
      (webOnly && s.kind !== "web") ||
      (bearer && s.kind !== "device") ||
      (!bearer && s.kind !== "web")
    )
      throw new HttpError(
        401,
        "unauthorized",
        "Sua sessão expirou. Entre novamente.",
      );
    if (
      !bearer &&
      !["GET", "HEAD"].includes(req.method) &&
      req.headers.origin !== baseUrl()
    )
      throw new HttpError(403, "origin_rejected", "Origem inválida.");
    return s;
  }
  async function owned(req: express.Request) {
    const s = await auth(req);
    const id = uuid.parse(req.params.id);
    const {
      rows: [r],
    } = await db.query("SELECT * FROM recordings WHERE id=$1 AND owner_id=$2", [
      id,
      s.user_id,
    ]);
    if (!r) throw new HttpError(404, "not_found", "Vídeo não encontrado.");
    return { s, r };
  }
  app.get("/api/auth/google", async (req, res) => {
    const state = token(),
      verifier = token();
    await db.query(
      "INSERT INTO oauth_states VALUES($1,$2,now()+interval '10 minutes')",
      [hash(state), verifier],
    );
    res.cookie("tinrec_oauth", state, {
      httpOnly: true,
      secure: baseUrl().startsWith("https:"),
      sameSite: "lax",
      maxAge: 600000,
      path: "/api/auth",
    });
    const params = new URLSearchParams({
      client_id: process.env.GOOGLE_CLIENT_ID!,
      redirect_uri: baseUrl() + "/api/auth/callback",
      response_type: "code",
      scope: "openid email https://www.googleapis.com/auth/drive.file",
      access_type: "offline",
      prompt: "consent",
      state,
      code_challenge: createHash("sha256").update(verifier).digest("base64url"),
      code_challenge_method: "S256",
    });
    res.redirect("https://accounts.google.com/o/oauth2/v2/auth?" + params);
  });
  app.get("/api/auth/callback", async (req, res) => {
    const state = z.string().parse(req.query.state);
    if (!same(state, cookies(req).tinrec_oauth ?? ""))
      throw new HttpError(
        400,
        "oauth_state",
        "Login inválido. Tente novamente.",
      );
    const {
      rows: [saved],
    } = await db.query(
      "DELETE FROM oauth_states WHERE state_hash=$1 AND expires_at>now() RETURNING verifier",
      [hash(state)],
    );
    if (!saved)
      throw new HttpError(
        400,
        "oauth_state",
        "Login expirado. Tente novamente.",
      );
    res.clearCookie("tinrec_oauth", { path: "/api/auth" });
    const code = z.string().parse(req.query.code);
    const response = await fetch("https://oauth2.googleapis.com/token", {
      method: "POST",
      body: new URLSearchParams({
        code,
        client_id: process.env.GOOGLE_CLIENT_ID!,
        client_secret: process.env.GOOGLE_CLIENT_SECRET!,
        redirect_uri: baseUrl() + "/api/auth/callback",
        grant_type: "authorization_code",
        code_verifier: saved.verifier,
      }),
      signal: AbortSignal.timeout(15000),
    });
    if (!response.ok)
      throw new HttpError(
        400,
        "google_auth",
        "Não foi possível entrar no Google. Tente novamente.",
      );
    const grant = await response.json();
    if (
      !String(grant.scope ?? "")
        .split(" ")
        .includes("https://www.googleapis.com/auth/drive.file")
    )
      throw new HttpError(
        403,
        "drive_scope",
        "Autorize o acesso aos arquivos criados pelo Tinrec para continuar.",
      );
    const profile = await fetch(
      "https://openidconnect.googleapis.com/v1/userinfo",
      {
        headers: { Authorization: `Bearer ${grant.access_token}` },
        signal: AbortSignal.timeout(15000),
      },
    );
    if (!profile.ok)
      throw new HttpError(
        400,
        "google_auth",
        "Não foi possível verificar sua conta.",
      );
    const user = await profile.json();
    if (
      !user.email_verified ||
      typeof user.email !== "string" ||
      typeof user.sub !== "string" ||
      !user.sub
    )
      throw new HttpError(
        403,
        "unverified_account",
        "Conecte uma conta Google com e-mail verificado.",
      );
    await db.query(
      "INSERT INTO users(id,email,refresh_token) VALUES($1,$2,$3) ON CONFLICT(id) DO UPDATE SET email=EXCLUDED.email,refresh_token=COALESCE(EXCLUDED.refresh_token,users.refresh_token)",
      [
        user.sub,
        user.email,
        grant.refresh_token ? encrypt(grant.refresh_token) : null,
      ],
    );
    const session = token();
    await db.query(
      "INSERT INTO sessions VALUES($1,$2,'web',now()+interval '7 days')",
      [hash(session), user.sub],
    );
    res.cookie("tinrec_session", session, {
      httpOnly: true,
      secure: baseUrl().startsWith("https:"),
      sameSite: "lax",
      maxAge: 604800000,
      path: "/",
    });
    res.redirect("/?signedin=1");
  });
  app.get("/api/me", async (req, res) => {
    const s = await auth(req);
    res.json({ email: s.email, id: s.user_id });
  });
  app.post("/api/devices/revoke", async (req, res) => {
    await auth(req);
    const bearer = req.headers.authorization?.match(
      /^Bearer ([A-Za-z0-9_-]+)$/,
    )?.[1];
    if (!bearer)
      throw new HttpError(401, "unauthorized", "Use a sessão do aplicativo.");
    await db.query(
      "DELETE FROM sessions WHERE token_hash=$1 AND kind='device'",
      [hash(bearer)],
    );
    res.json({ ok: true });
  });
  app.post("/api/logout", async (req, res) => {
    await auth(req);
    await db.query("DELETE FROM sessions WHERE token_hash=$1", [
      hash(cookies(req).tinrec_session ?? ""),
    ]);
    res.clearCookie("tinrec_session", { path: "/" });
    res.json({ ok: true });
  });
  app.post("/api/devices/start", async (_req, res) => {
    await db.query("DELETE FROM device_codes WHERE expires_at<now()");
    const {
      rows: [count],
    } = await db.query("SELECT count(*) AS n FROM device_codes");
    if (Number(count.n) >= 50)
      throw new HttpError(
        429,
        "rate_limit",
        "Aguarde alguns minutos e tente novamente.",
      );
    const code = token(12),
      secret = token();
    await db.query(
      "INSERT INTO device_codes(code_hash,secret_hash,expires_at) VALUES($1,$2,now()+interval '10 minutes')",
      [hash(code), hash(secret)],
    );
    res.json({
      code,
      secret,
      url: `${baseUrl()}/connect?code=${code}`,
      expiresIn: 600,
    });
  });
  app.post("/api/devices/approve", async (req, res) => {
    const s = await auth(req, true);
    const code = z.string().min(16).max(64).parse(req.body.code);
    const { rows } = await db.query(
      "UPDATE device_codes SET user_id=$1 WHERE code_hash=$2 AND expires_at>now() AND consumed_at IS NULL AND user_id IS NULL RETURNING code_hash",
      [s.user_id, hash(code)],
    );
    if (!rows.length)
      throw new HttpError(
        410,
        "code_expired",
        "Código expirado ou já utilizado. Inicie uma nova conexão no aplicativo.",
      );
    res.json({ ok: true });
  });
  app.post("/api/devices/poll", async (req, res) => {
    const { code, secret } = z
      .object({ code: z.string().max(64), secret: z.string().max(64) })
      .parse(req.body);
    const deviceToken = token();
    const { rows } = await db.query(
      "WITH consumed AS (DELETE FROM device_codes WHERE code_hash=$1 AND secret_hash=$2 AND expires_at>now() AND user_id IS NOT NULL RETURNING user_id) INSERT INTO sessions(token_hash,user_id,kind,expires_at) SELECT $3,user_id,'device',now()+interval '90 days' FROM consumed RETURNING user_id",
      [hash(code), hash(secret), hash(deviceToken)],
    );
    if (rows.length) {
      res.json({ token: deviceToken });
      return;
    }
    const { rows: pending } = await db.query(
      "SELECT code_hash FROM device_codes WHERE code_hash=$1 AND secret_hash=$2 AND expires_at>now()",
      [hash(code), hash(secret)],
    );
    if (!pending.length)
      throw new HttpError(
        410,
        "code_expired",
        "Código expirado ou já utilizado.",
      );
    res.status(202).json({ pending: true });
  });
  app.post("/api/recordings", async (req, res) => {
    const s = await auth(req);
    const b = z
      .object({
        id: uuid,
        title,
        resolution: z.union([z.literal(720), z.literal(1080)]),
        mediaType: z.enum(["video", "image"]).default("video"),
      })
      .parse(req.body);
    await db.query(
      "INSERT INTO recordings(id,owner_id,public_id,title,resolution,media_type) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",
      [b.id, s.user_id, token(24), b.title, b.resolution, b.mediaType],
    );
    const {
      rows: [r],
    } = await db.query("SELECT * FROM recordings WHERE id=$1 AND owner_id=$2", [
      b.id,
      s.user_id,
    ]);
    if (!r || r.media_type !== b.mediaType)
      throw new HttpError(409, "conflict", "Identificador já utilizado.");
    res.json(view(r));
  });
  app.get("/api/recordings", async (req, res) => {
    const s = await auth(req);
    const q = z.string().max(180).default("").parse(req.query.q);
    const { rows } = await db.query(
      "SELECT * FROM recordings WHERE owner_id=$1 AND title ILIKE $2 ORDER BY created_at DESC",
      [s.user_id, "%" + q + "%"],
    );
    res.json(rows.map(view));
  });
  app.patch("/api/recordings/:id", async (req, res) => {
    const { s, r } = await owned(req);
    const b = z.object({ title }).parse(req.body);
    if (r.drive_file_id)
      await drive.rename(s.user_id, r.drive_file_id, b.title, r.media_type);
    await db.query("UPDATE recordings SET title=$1 WHERE id=$2", [
      b.title,
      r.id,
    ]);
    res.json({ ok: true });
  });
  app.post("/api/recordings/:id/discard", async (req, res) => {
    const s = await auth(req),
      id = uuid.parse(req.params.id);
    const {
      rows: [r],
    } = await db.query("SELECT * FROM recordings WHERE id=$1 AND owner_id=$2", [
      id,
      s.user_id,
    ]);
    if (!r) {
      res.json({ ok: true });
      return;
    }
    const { rows } = await db.query(
      "DELETE FROM recordings WHERE id=$1 AND owner_id=$2 AND drive_file_id IS NULL AND temp_size_bytes=0 AND status IN ('recording','error') RETURNING id",
      [id, s.user_id],
    );
    if (!rows.length)
      throw new HttpError(
        409,
        "already_uploading",
        "Este vídeo já foi enviado ou entrou no envio. Descarte apenas a gravação em andamento.",
      );
    res.json({ ok: true });
  });
  app.post("/api/recordings/:id/temporary", async (req, res) => {
    const { r } = await owned(req);
    if (r.media_type === "image") {
      res.json({ available: false, reason: "image_direct_drive" });
      return;
    }
    const b = z
      .object({
        sizeBytes: z
          .number()
          .int()
          .positive()
          .max(5 * 1024 ** 4),
        durationMs: z.number().int().nonnegative(),
      })
      .parse(req.body);
    if (Number(r.size_bytes) > 0 && Number(r.size_bytes) !== b.sizeBytes)
      throw new HttpError(
        409,
        "size_changed",
        "O arquivo local mudou desde o início do envio.",
      );
    try {
      res.json(await temporary.upload(r, b.sizeBytes, b.durationMs));
    } catch (e: any) {
      if (e instanceof HttpError) throw e;
      console.warn("Tinrec R2 failure", {
        operation: "temporary-upload",
        name: typeof e?.name === "string" ? e.name : "unknown",
        status: e?.$metadata?.httpStatusCode,
      });
      throw new HttpError(
        503,
        "r2_error",
        "Não foi possível preparar o envio temporário no R2. Verifique as credenciais e o acesso ao bucket.",
      );
    }
  });
  app.post("/api/public/:publicId/drive-ready", async (req, res) => {
    const s = await auth(req, true);
    const id = z
      .string()
      .regex(/^[A-Za-z0-9_-]{32}$/)
      .parse(req.params.publicId);
    const {
      rows: [r],
    } = await db.query(
      "SELECT * FROM recordings WHERE public_id=$1 AND owner_id=$2",
      [id, s.user_id],
    );
    if (!r) throw new HttpError(404, "not_found", "Vídeo não encontrado.");
    if (r.status !== "uploaded")
      throw new HttpError(
        409,
        "upload_incomplete",
        "Aguarde o término do envio ao Drive antes de confirmar a reprodução.",
      );
    // Recheck the durable copy before scheduling removal of the temporary object.
    await drive.complete(s.user_id, r);
    await db.query(
      "UPDATE recordings SET drive_ready_at=COALESCE(drive_ready_at,now()),temp_delete_after=COALESCE(temp_delete_after,now()+interval '24 hours') WHERE id=$1",
      [r.id],
    );
    res.json({ ok: true });
  });
  app.get("/api/cron/r2-check", async (req, res) => {
    const secret = process.env.CRON_SECRET;
    if (!secret || !same(req.headers.authorization ?? "", `Bearer ${secret}`))
      throw new HttpError(401, "unauthorized", "Não autorizado.");
    if (!storage.ready())
      throw new HttpError(503, "r2_setup", "R2 não configurado.");
    const key = `diagnostics/${token(24)}.mp4`,
      body = Buffer.from("Tinrec R2 connectivity check");
    let uploadId: string | undefined;
    try {
      uploadId = await storage.start(key);
      const put = await fetch(
        await storage.signPart(key, uploadId, 1, body.length),
        { method: "PUT", body, signal: AbortSignal.timeout(20000) },
      );
      if (!put.ok) throw Error("R2 test upload failed");
      const parts = await storage.parts(key, uploadId);
      if (parts.length !== 1 || parts[0].size !== body.length)
        throw Error("R2 test parts failed");
      await storage.complete(key, uploadId, parts);
      uploadId = undefined;
      if ((await storage.size(key)) !== body.length)
        throw Error("R2 test HEAD failed");
      const read = await fetch(await storage.playback(key), {
        headers: { Range: "bytes=0-5" },
        signal: AbortSignal.timeout(20000),
      });
      if (read.status !== 206 || (await read.text()) !== "Tinrec")
        throw Error("R2 test Range failed");
      await storage.remove(key);
      if ((await storage.size(key)) !== null)
        throw Error("R2 test deletion failed");
      res.json({
        ready: true,
        upload: true,
        read: true,
        range: true,
        deleted: true,
      });
    } finally {
      if (uploadId) await storage.abort(key, uploadId);
      await storage.remove(key);
    }
  });
  app.get("/api/cron/temporary-cleanup", async (req, res) => {
    const secret = process.env.CRON_SECRET;
    if (!secret || !same(req.headers.authorization ?? "", `Bearer ${secret}`))
      throw new HttpError(401, "unauthorized", "Não autorizado.");
    const { rows } = await db.query(
      "SELECT * FROM recordings WHERE temp_delete_after<=now() AND drive_ready_at IS NOT NULL AND temp_deleted_at IS NULL AND temp_size_bytes>0 ORDER BY temp_delete_after LIMIT 25",
    );
    let removed = 0;
    for (const r of rows) {
      try {
        if (await temporary.cleanup(r)) removed++;
      } catch {
        console.warn("Tinrec R2 cleanup failed");
      }
    }
    res.json({ removed });
  });
  app.post("/api/recordings/:id/upload", async (req, res) => {
    const { s, r } = await owned(req);
    const b = z
      .object({
        sizeBytes: z
          .number()
          .int()
          .positive()
          .max(5 * 1024 ** 4),
        durationMs: z.number().int().nonnegative(),
        restart: z.boolean().default(false),
      })
      .parse(req.body);
    if (r.status === "uploaded") {
      res.json({ completed: true });
      return;
    }
    if (Number(r.size_bytes) > 0 && Number(r.size_bytes) !== b.sizeBytes)
      throw new HttpError(
        409,
        "size_changed",
        "O arquivo local mudou desde o início do envio.",
      );
    await db.query(
      "UPDATE recordings SET size_bytes=$1,duration_ms=$2,status='uploading',error_code=NULL WHERE id=$3",
      [b.sizeBytes, b.durationMs, r.id],
    );
    res.json(
      await drive.start(
        s.user_id,
        { ...r, size_bytes: b.sizeBytes },
        b.restart,
      ),
    );
  });
  app.post("/api/recordings/:id/progress", async (req, res) => {
    const { r } = await owned(req);
    const b = z
      .object({ uploadedBytes: z.number().int().nonnegative() })
      .parse(req.body);
    if (b.uploadedBytes > Number(r.size_bytes))
      throw new HttpError(400, "invalid_progress", "Progresso inválido.");
    await db.query(
      "UPDATE recordings SET uploaded_bytes=$1 WHERE id=$2 AND status<>'uploaded'",
      [b.uploadedBytes, r.id],
    );
    res.json({ ok: true });
  });
  app.post("/api/recordings/:id/complete", async (req, res) => {
    const { s, r } = await owned(req);
    if (r.status !== "uploaded") {
      if (!r.drive_file_id || Number(r.size_bytes) <= 0)
        throw new HttpError(
          409,
          "upload_incomplete",
          "O envio ainda não foi iniciado.",
        );
      await drive.complete(s.user_id, r);
      await db.query(
        "UPDATE recordings SET status='uploaded',uploaded_bytes=size_bytes,upload_uri=NULL,error_code=NULL WHERE id=$1",
        [r.id],
      );
    }
    res.json({ url: `${baseUrl()}/v/${r.public_id}` });
  });
  app.post("/api/recordings/:id/error", async (req, res) => {
    const { r } = await owned(req);
    const code = z
      .enum([
        "upload_failed",
        "recording_failed",
        "google_reconnect",
        "drive_full",
        "sharing_blocked",
        "file_unavailable",
      ])
      .parse(req.body.code);
    await db.query(
      "UPDATE recordings SET status='error',error_code=$1 WHERE id=$2 AND status<>'uploaded'",
      [code, r.id],
    );
    res.json({ ok: true });
  });
  app.get("/api/public/:publicId", async (req, res) => {
    const id = z
      .string()
      .regex(/^[A-Za-z0-9_-]{32}$/)
      .parse(req.params.publicId);
    const {
      rows: [r],
    } = await db.query("SELECT * FROM recordings WHERE public_id=$1", [id]);
    if (!r)
      throw new HttpError(404, "not_found", "Este vídeo não foi encontrado.");
    let canConfirmDrive = false;
    if (cookies(req).tinrec_session) {
      try {
        canConfirmDrive = (await auth(req, true)).user_id === r.owner_id;
      } catch {
        /* Public playback is independent of library authentication. */
      }
    }
    // Cleanup also runs on access, so already-confirmed copies do not depend on cron alone.
    if (
      r.temp_delete_after &&
      new Date(r.temp_delete_after).getTime() <= Date.now() &&
      r.drive_ready_at &&
      !r.temp_deleted_at
    ) {
      try {
        if (await temporary.cleanup(r)) {
          r.temp_ready = false;
          r.temp_deleted_at = new Date();
        }
      } catch {
        console.warn("Tinrec R2 cleanup failed");
      }
    }
    const playbackUrl =
      r.temp_ready && !r.temp_deleted_at && !r.drive_ready_at && storage.ready()
        ? await storage.playback(`temporary/${r.public_id}.mp4`)
        : null;
    res.json({
      mediaType: r.media_type ?? "video",
      playbackUrl,
      playbackExpiresAt: playbackUrl ? Date.now() + 3600_000 : null,
      driveReady: !!r.drive_ready_at,
      canConfirmDrive,
      title: r.title,
      status: r.status,
      progress:
        Number(r.size_bytes) > 0
          ? Math.min(
              100,
              Math.floor(
                (Number(r.uploaded_bytes) / Number(r.size_bytes)) * 100,
              ),
            )
          : 0,
      embedUrl:
        r.status === "uploaded"
          ? `https://drive.google.com/file/d/${encodeURIComponent(r.drive_file_id)}/preview`
          : null,
    });
  });
  app.use("/api", (_req, _res) => {
    throw new HttpError(404, "not_found", "Página não encontrada.");
  });
  app.use(
    (
      err: any,
      _req: express.Request,
      res: express.Response,
      _next: express.NextFunction,
    ) => {
      if (err instanceof z.ZodError) {
        res.status(400).json({
          code: "validation",
          message: "Verifique os dados enviados.",
        });
        return;
      }
      if (err instanceof HttpError) {
        res.status(err.status).json({ code: err.code, message: err.message });
        return;
      }
      console.error(
        "Tinrec request failed:",
        err instanceof Error ? err.name : "UnknownError",
      );
      res.status(500).json({
        code: "internal",
        message: "Não foi possível concluir. Tente novamente em instantes.",
      });
    },
  );
  return app;
}
