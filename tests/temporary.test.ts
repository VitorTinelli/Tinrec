import { beforeEach, afterEach, it, expect, vi } from "vitest";
import { PGlite } from "@electric-sql/pglite";
import { readFile } from "node:fs/promises";
import request from "supertest";
import { createApp } from "../server/app";
import { hash } from "../server/security";
import { PART_BYTES, type TemporaryStorage } from "../server/temporary";
let db: PGlite,
  app: ReturnType<typeof createApp>,
  storage: TemporaryStorage,
  parts: { number: number; size: number; etag: string }[],
  objectSize: number | null;
const id = "11111111-1111-4111-8111-111111111111",
  publicId = "p".repeat(32),
  device = "device",
  web = "web";
const post = (suffix: string) =>
  request(app)
    .post(`/api/recordings/${id}${suffix}`)
    .auth(device, { type: "bearer" });
beforeEach(async () => {
  vi.stubEnv("APP_URL", "https://tinrec.example");
  vi.stubEnv("DATABASE_URL", "unused");
  vi.stubEnv("GOOGLE_CLIENT_ID", "client");
  vi.stubEnv("GOOGLE_CLIENT_SECRET", "secret");
  vi.stubEnv("TOKEN_ENCRYPTION_KEY", Buffer.alloc(32).toString("base64"));
  vi.stubEnv("CRON_SECRET", "cron-test-secret");
  db = new PGlite();
  await db.exec(
    await readFile(new URL("../server/schema.sql", import.meta.url), "utf8"),
  );
  await db.query(
    "INSERT INTO users(id,email) VALUES('owner','owner@example.test'),('other','other@example.test')",
  );
  await db.query(
    "INSERT INTO sessions VALUES($1,'owner','device',now()+interval '1 day'),($2,'owner','web',now()+interval '1 day'),($3,'other','device',now()+interval '1 day')",
    [hash(device), hash(web), hash("other")],
  );
  await db.query(
    "INSERT INTO recordings(id,owner_id,public_id,title,resolution) VALUES($1,'owner',$2,'Video',720)",
    [id, publicId],
  );
  parts = [];
  objectSize = null;
  storage = {
    ready: () => true,
    start: vi.fn(async () => "multipart"),
    parts: vi.fn(async () => parts),
    signPart: vi.fn(
      async () => "https://r2.example/temporary/part?signature=secret",
    ),
    complete: vi.fn(async () => {
      objectSize = parts.reduce((sum, p) => sum + p.size, 0);
    }),
    size: vi.fn(async () => objectSize),
    playback: vi.fn(
      async () => "https://r2.example/temporary/video?signature=read",
    ),
    remove: vi.fn(async () => {}),
    abort: vi.fn(async () => {}),
  };
  app = createApp(
    db as any,
    { start: vi.fn(), complete: vi.fn(async () => {}), rename: vi.fn() },
    storage,
  );
});
afterEach(async () => {
  await db.close();
  vi.unstubAllEnvs();
});
it("restricts uploads to the owner and never gives anonymous users a write URL", async () => {
  expect(
    (
      await request(app)
        .post(`/api/recordings/${id}/temporary`)
        .send({ sizeBytes: 100, durationMs: 1000 })
    ).status,
  ).toBe(401);
  expect(
    (
      await request(app)
        .post(`/api/recordings/${id}/temporary`)
        .auth("other", { type: "bearer" })
        .send({ sizeBytes: 100, durationMs: 1000 })
    ).status,
  ).toBe(404);
  expect(storage.start).not.toHaveBeenCalled();
});
it("resumes from acknowledged parts and publishes R2 before Drive completion", async () => {
  const first = await post("/temporary").send({
    sizeBytes: PART_BYTES + 3,
    durationMs: 1000,
  });
  expect(first.status).toBe(200);
  expect(first.body.offset).toBe(0);
  expect(first.body.length).toBe(PART_BYTES);
  parts = [{ number: 1, size: PART_BYTES, etag: "part-one" }];
  const next = await post("/temporary").send({
    sizeBytes: PART_BYTES + 3,
    durationMs: 1000,
  });
  expect(next.body.offset).toBe(PART_BYTES);
  expect(next.body.length).toBe(3);
  expect(storage.start).toHaveBeenCalledTimes(1);
  parts.push({ number: 2, size: 3, etag: "part-two" });
  expect(
    (
      await post("/temporary").send({
        sizeBytes: PART_BYTES + 3,
        durationMs: 1000,
      })
    ).body.completed,
  ).toBe(true);
  const shared = (await request(app).get(`/api/public/${publicId}`)).body;
  expect(shared.playbackUrl).toContain("signature=read");
  expect(shared.embedUrl).toBeNull();
  expect(shared.canConfirmDrive).toBe(false);
  expect(JSON.stringify(shared)).not.toMatch(
    /multipart|part-one|owner@example|signature=secret/,
  );
  expect(
    (
      await post("/temporary").send({
        sizeBytes: PART_BYTES + 3,
        durationMs: 1000,
      })
    ).body.completed,
  ).toBe(true);
  expect(storage.complete).toHaveBeenCalledTimes(1);
});
it("does not remove a temporary video until its owner confirms Drive and the grace period passes", async () => {
  await db.query(
    "UPDATE recordings SET status='uploaded',size_bytes=100,temp_size_bytes=100,temp_ready=true,drive_file_id='drive-file' WHERE id=$1",
    [id],
  );
  await db.query("UPDATE temporary_budget SET used_bytes=100");
  const cron = () =>
    request(app)
      .get("/api/cron/temporary-cleanup")
      .set("Authorization", "Bearer cron-test-secret");
  expect((await cron()).body.removed).toBe(0);
  expect(storage.remove).not.toHaveBeenCalled();
  expect(
    (
      await request(app)
        .post(`/api/public/${publicId}/drive-ready`)
        .auth(device, { type: "bearer" })
        .send({})
    ).status,
  ).toBe(401);
  expect(
    (
      await request(app)
        .post(`/api/public/${publicId}/drive-ready`)
        .set("Cookie", `tinrec_session=${web}`)
        .send({})
    ).status,
  ).toBe(403);
  expect(
    (
      await request(app)
        .post(`/api/public/${publicId}/drive-ready`)
        .set("Cookie", `tinrec_session=${web}`)
        .set("Origin", "https://tinrec.example")
        .send({})
    ).status,
  ).toBe(200);
  expect((await cron()).body.removed).toBe(0);
  await db.query(
    "UPDATE recordings SET temp_delete_after=now()-interval '1 minute' WHERE id=$1",
    [id],
  );
  expect((await cron()).body.removed).toBe(1);
  expect(storage.remove).toHaveBeenCalledTimes(1);
  expect((await cron()).body.removed).toBe(0);
  const {
    rows: [budget],
  } = await db.query<{ used_bytes: string }>(
    "SELECT used_bytes FROM temporary_budget",
  );
  expect(Number(budget.used_bytes)).toBe(0);
});
it("rejects changed files and inconsistent parts", async () => {
  await post("/temporary").send({ sizeBytes: 100, durationMs: 1000 });
  expect(
    (await post("/temporary").send({ sizeBytes: 101, durationMs: 1000 }))
      .status,
  ).toBe(409);
  parts = [{ number: 1, size: 99, etag: "wrong" }];
  expect(
    (await post("/temporary").send({ sizeBytes: 100, durationMs: 1000 }))
      .status,
  ).toBe(409);
  expect(storage.complete).not.toHaveBeenCalled();
});
it("falls back to Drive when temporary storage is unavailable or its budget is full", async () => {
  storage.ready = () => false;
  expect(
    (await post("/temporary").send({ sizeBytes: 100, durationMs: 1000 })).body
      .available,
  ).toBe(false);
  storage.ready = () => true;
  await db.query("UPDATE temporary_budget SET used_bytes=$1", [8 * 1024 ** 3]);
  expect(
    (await post("/temporary").send({ sizeBytes: 100, durationMs: 1000 })).body
      .reason,
  ).toBe("capacity");
  expect(storage.start).not.toHaveBeenCalled();
});
it("protects the cleanup job", async () => {
  expect((await request(app).get("/api/cron/temporary-cleanup")).status).toBe(
    401,
  );
});
it("can prepare an older video without resetting its completed Drive upload", async () => {
  await db.query(
    "UPDATE recordings SET status='uploaded',size_bytes=100,drive_file_id='drive-file' WHERE id=$1",
    [id],
  );
  expect(
    (await post("/temporary").send({ sizeBytes: 100, durationMs: 1000 }))
      .status,
  ).toBe(200);
  const {
    rows: [row],
  } = await db.query<{ status: string }>(
    "SELECT status FROM recordings WHERE id=$1",
    [id],
  );
  expect(row.status).toBe("uploaded");
});
