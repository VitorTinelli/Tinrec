import { describe, it, expect } from "vitest";
import { driveFailure } from "../server/drive";
describe("Drive error diagnosis", () => {
  it("distinguishes a disabled API from missing user authorization", () => {
    expect(
      driveFailure(403, {
        error: { errors: [{ reason: "accessNotConfigured" }] },
      }).code,
    ).toBe("drive_api_disabled");
    expect(
      driveFailure(403, {
        error: { details: [{ reason: "SERVICE_DISABLED" }] },
      }).code,
    ).toBe("drive_api_disabled");
    expect(
      driveFailure(403, {
        error: { errors: [{ reason: "insufficientPermissions" }] },
      }).code,
    ).toBe("google_reconnect");
    expect(driveFailure(401, {}).code).toBe("google_reconnect");
  });
  it("distinguishes quota, policy and temporary rate limits", () => {
    expect(
      driveFailure(403, {
        error: { errors: [{ reason: "storageQuotaExceeded" }] },
      }).code,
    ).toBe("drive_full");
    expect(
      driveFailure(403, { error: { errors: [{ reason: "domainPolicy" }] } })
        .code,
    ).toBe("drive_policy");
    expect(driveFailure(429, {}).status).toBe(503);
    expect(
      driveFailure(403, {
        error: { errors: [{ reason: "userRateLimitExceeded" }] },
      }).code,
    ).toBe("drive_rate_limit");
  });
  it("handles unknown responses without exposing Google response contents", () => {
    const error = driveFailure(500, {
      error: { message: "private data", errors: [{ reason: "unknown" }] },
    });
    expect(error.code).toBe("drive_error");
    expect(error.message).not.toContain("private data");
    expect(driveFailure(404, null).code).toBe("file_unavailable");
  });
});

// Exercise real Drive request construction without transmitting user data.
import { afterEach, vi } from "vitest";
import { googleDrive } from "../server/drive";
import { encrypt } from "../server/security";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});
it("creates, uploads, verifies and renames PNG with the correct MIME type", async () => {
  vi.stubEnv("TOKEN_ENCRYPTION_KEY", Buffer.alloc(32, 7).toString("base64"));
  const requests: { url: string; init?: RequestInit }[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      requests.push({ url, init });
      if (url.includes("oauth2.googleapis.com"))
        return Response.json({ access_token: "test-token" });
      if (url.includes("fields=id,size"))
        return Response.json({
          id: "image-file",
          mimeType: "image/png",
          size: "128",
          appProperties: { tinrecRecording: "print-id" },
        });
      if (url.endsWith("?fields=id"))
        return new Response(null, { status: 404 });
      if (url.includes("uploadType=resumable"))
        return new Response(null, {
          status: 200,
          headers: {
            location:
              "https://www.googleapis.com/upload/drive/v3/files/image-file?upload_id=test",
          },
        });
      return Response.json({ id: "image-file" });
    }),
  );
  const db = {
    query: vi.fn(async (sql: string) => ({
      rows: sql.startsWith("SELECT * FROM users")
        ? [
            {
              refresh_token: encrypt("test-refresh"),
              drive_folder_id: "folder",
            },
          ]
        : [],
    })),
  };
  const service = googleDrive(db as unknown as import("../server/db").Database);
  const recording = {
    id: "print-id",
    title: "Print editado",
    media_type: "image",
    size_bytes: 128,
    drive_file_id: "image-file",
  };
  await service.start("owner", recording, false);
  expect(
    requests.some(
      (r) =>
        r.init?.body ===
        JSON.stringify({
          id: "image-file",
          name: "Print editado.png",
          parents: ["folder"],
          mimeType: "image/png",
          appProperties: { tinrecRecording: "print-id" },
        }),
    ),
  ).toBe(true);
  expect(
    requests.find((r) => r.url.includes("uploadType=resumable"))?.init?.headers,
  ).toMatchObject({ "X-Upload-Content-Type": "image/png" });
  await service.complete("owner", recording);
  await service.rename("owner", "image-file", "Novo print", "image");
  expect(requests.at(-1)?.init?.body).toBe(
    JSON.stringify({ name: "Novo print.png" }),
  );
});
