import { readFile } from "node:fs/promises";
import { PGlite } from "@electric-sql/pglite";
import { vi } from "vitest";
import { hash } from "../../server/security";
import type { Drive } from "../../server/drive";

// Each test owns its database; shared setup must never share account state.
export async function createTestDatabase(
  device: string,
  web: string,
  other: string,
) {
  for (const [name, value] of Object.entries({
    APP_URL: "https://tinrec.example",
    DATABASE_URL: "postgresql://unused",
    GOOGLE_CLIENT_ID: "client",
    GOOGLE_CLIENT_SECRET: "secret",
    TOKEN_ENCRYPTION_KEY: Buffer.alloc(32, 7).toString("base64"),
    CRON_SECRET: "cron-test-secret",
  }))
    vi.stubEnv(name, value);
  const db = new PGlite();
  await db.exec(
    await readFile(new URL("../../server/schema.sql", import.meta.url), "utf8"),
  );
  await db.query(
    "INSERT INTO users(id,email) VALUES('owner','owner@example.com'),('other','other@example.com')",
  );
  await db.query(
    "INSERT INTO sessions VALUES($1,'owner','device',now()+interval '1 day'),($2,'owner','web',now()+interval '1 day'),($3,'other','device',now()+interval '1 day')",
    [hash(device), hash(web), hash(other)],
  );
  return db;
}

export function createDriveMock(): Drive {
  return {
    start: vi.fn(async () => ({
      uri: "https://www.googleapis.com/upload/drive/v3/files/123?upload_id=secret",
      fileId: "123",
    })),
    complete: vi.fn(async () => {}),
    rename: vi.fn(async () => {}),
  };
}
