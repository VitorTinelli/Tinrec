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
