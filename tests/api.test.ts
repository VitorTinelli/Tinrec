import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { PGlite } from "@electric-sql/pglite";
import { randomUUID } from "node:crypto";
import request from "supertest";
import { createApp } from "../server/app";
import { createTestDatabase, createDriveMock } from "./support/server";
import type { Database } from "../server/db";
import type { Drive } from "../server/drive";
let sql: PGlite, app: ReturnType<typeof createApp>, drive: Drive;
const device = "device-session",
  web = "web-session";
beforeEach(async () => {
  sql = await createTestDatabase(device, web, "other-session");
  drive = createDriveMock();
  app = createApp(sql as unknown as Database, drive);
});
afterEach(async () => {
  await sql.close();
  vi.unstubAllEnvs();
});
const create = () =>
  request(app)
    .post("/api/recordings")
    .auth(device, { type: "bearer" })
    .send({ id: randomUUID(), title: "Atendimento", resolution: 1080 });
describe("owner boundary and public sharing", () => {
  it("fails closed when setup is missing", async () => {
    vi.stubEnv("GOOGLE_CLIENT_ID", "");
    expect((await request(app).get("/api/health")).body.configured).toBe(false);
    expect((await request(app).get("/api/recordings")).status).toBe(503);
  });
  it("rejects anonymous access and isolates authenticated accounts", async () => {
    expect((await request(app).get("/api/recordings")).status).toBe(401);
    const owner = await create();
    const other = await request(app)
      .get("/api/recordings")
      .auth("other-session", { type: "bearer" });
    expect(other.status).toBe(200);
    expect(other.body).toEqual([]);
    for (const path of ["upload", "complete", "progress", "error"]) {
      const response = await request(app)
        .post(`/api/recordings/${owner.body.id}/${path}`)
        .auth("other-session", { type: "bearer" })
        .send({});
      expect(response.status).toBe(404);
    }
    const renamed = await request(app)
      .patch(`/api/recordings/${owner.body.id}`)
      .auth("other-session", { type: "bearer" })
      .send({ title: "Outra pessoa" });
    expect(renamed.status).toBe(404);
    const collision = await request(app)
      .post("/api/recordings")
      .auth("other-session", { type: "bearer" })
      .send({ id: owner.body.id, title: "Outro", resolution: 1080 });
    expect(collision.status).toBe(409);
  });
  it("revokes only the current desktop session", async () => {
    expect(
      (
        await request(app)
          .post("/api/devices/revoke")
          .auth(device, { type: "bearer" })
          .send({})
      ).status,
    ).toBe(200);
    expect(
      (await request(app).get("/api/me").auth(device, { type: "bearer" }))
        .status,
    ).toBe(401);
    expect(
      (
        await request(app)
          .get("/api/me")
          .auth("other-session", { type: "bearer" })
      ).status,
    ).toBe(200);
  });
  it("creates one recording when a reservation is retried", async () => {
    const first = await create();
    expect(first.status).toBe(200);
    const second = await request(app)
      .post("/api/recordings")
      .auth(device, { type: "bearer" })
      .send({ id: first.body.id, title: "Atendimento", resolution: 1080 });
    expect(second.status).toBe(200);
    expect(second.body.publicId).toBe(first.body.publicId);
    expect((await sql.query("SELECT * FROM recordings")).rows).toHaveLength(1);
  });
  it("public endpoint does not expose owner, session, upload URI or file before completion", async () => {
    const r = await create();
    const publicResult = await request(app).get(
      "/api/public/" + r.body.publicId,
    );
    expect(publicResult.status).toBe(200);
    expect(Object.keys(publicResult.body).sort()).toEqual([
      "canConfirmDrive",
      "driveReady",
      "embedUrl",
      "playbackExpiresAt",
      "playbackUrl",
      "progress",
      "status",
      "title",
    ]);
    expect(publicResult.body.embedUrl).toBeNull();
  });
  it("cookie writes require same origin; device tokens cannot approve another device", async () => {
    const r = await create();
    expect(
      (
        await request(app)
          .patch("/api/recordings/" + r.body.id)
          .set("Cookie", "tinrec_session=" + web)
          .send({ title: "Other" })
      ).status,
    ).toBe(403);
    expect(
      (
        await request(app)
          .post("/api/devices/approve")
          .auth(device, { type: "bearer" })
          .send({ code: "1234567890123456" })
      ).status,
    ).toBe(401);
  });
  it("does not mark an upload without a Drive file as available", async () => {
    const r = await create();
    expect(
      (
        await request(app)
          .post(`/api/recordings/${r.body.id}/complete`)
          .auth(device, { type: "bearer" })
          .send({})
      ).status,
    ).toBe(409);
    expect(drive.complete).not.toHaveBeenCalled();
  });
  it("bounds progress and prevents file replacement during retry", async () => {
    const r = await create();
    const upload = () =>
      request(app)
        .post(`/api/recordings/${r.body.id}/upload`)
        .auth(device, { type: "bearer" });
    expect(
      (await upload().send({ sizeBytes: 100, durationMs: 1000 })).status,
    ).toBe(200);
    expect(
      (await upload().send({ sizeBytes: 101, durationMs: 1000 })).status,
    ).toBe(409);
    expect(
      (
        await request(app)
          .post(`/api/recordings/${r.body.id}/progress`)
          .auth(device, { type: "bearer" })
          .send({ uploadedBytes: 101 })
      ).status,
    ).toBe(400);
  });
  it("returns an embed only after server-side completion succeeds", async () => {
    const r = await create();
    await sql.query(
      "UPDATE recordings SET size_bytes=100,drive_file_id='123' WHERE id=$1",
      [r.body.id],
    );
    const finished = await request(app)
      .post(`/api/recordings/${r.body.id}/complete`)
      .auth(device, { type: "bearer" })
      .send({});
    expect(finished.status).toBe(200);
    expect(drive.complete).toHaveBeenCalledOnce();
    const pub = await request(app).get("/api/public/" + r.body.publicId);
    expect(pub.body.embedUrl).toBe(
      "https://drive.google.com/file/d/123/preview",
    );
    expect(pub.body.progress).toBe(100);
  });
});
describe("one-time device codes", () => {
  it("requires approval and correct secret, then atomically consumes the code", async () => {
    const start = await request(app).post("/api/devices/start").send({});
    expect(start.status).toBe(200);
    const { code, secret } = start.body;
    const poll = (s = secret) =>
      request(app).post("/api/devices/poll").send({ code, secret: s });
    expect((await poll()).status).toBe(202);
    expect((await poll("wrong")).status).toBe(410);
    expect(
      (
        await request(app)
          .post("/api/devices/approve")
          .set("Cookie", "tinrec_session=" + web)
          .set("Origin", "https://tinrec.example")
          .send({ code })
      ).status,
    ).toBe(200);
    const results = await Promise.all([poll(), poll()]);
    expect(results.map((result) => result.status).sort()).toEqual([200, 410]);
    const result = results.find((result) => result.status === 200)!;
    expect(result.body.token).toBeTypeOf("string");
    expect((await poll()).status).toBe(410);
    expect(
      (
        await request(app)
          .get("/api/me")
          .auth(result.body.token, { type: "bearer" })
      ).status,
    ).toBe(200);
  });
  it("rejects an expired device code", async () => {
    const { body } = await request(app).post("/api/devices/start").send({});
    await sql.query(
      "UPDATE device_codes SET expires_at=now()-interval '1 second'",
    );
    expect(
      (await request(app).post("/api/devices/poll").send(body)).status,
    ).toBe(410);
  });
});
describe("discarding a live capture", () => {
  it("removes only the owner's unuploaded reservation and is idempotent", async () => {
    const r = await create(),
      url = `/api/recordings/${r.body.id}/discard`;
    expect((await request(app).post(url).send({})).status).toBe(401);
    expect(
      (
        await request(app)
          .post(url)
          .auth("other-session", { type: "bearer" })
          .send({})
      ).status,
    ).toBe(200);
    expect(
      (await request(app).get(`/api/public/${r.body.publicId}`)).status,
    ).toBe(200);
    expect(
      (await request(app).post(url).auth(device, { type: "bearer" }).send({}))
        .status,
    ).toBe(200);
    expect(
      (await request(app).post(url).auth(device, { type: "bearer" }).send({}))
        .status,
    ).toBe(200);
    expect(
      (await request(app).get(`/api/public/${r.body.publicId}`)).status,
    ).toBe(404);
  });
  it("refuses to discard an object that already entered storage", async () => {
    const r = await create();
    await sql.query("UPDATE recordings SET temp_size_bytes=100 WHERE id=$1", [
      r.body.id,
    ]);
    expect(
      (
        await request(app)
          .post(`/api/recordings/${r.body.id}/discard`)
          .auth(device, { type: "bearer" })
          .send({})
      ).status,
    ).toBe(409);
  });
});
