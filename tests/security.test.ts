import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { decrypt, encrypt } from "../server/security";

beforeEach(() =>
  vi.stubEnv("TOKEN_ENCRYPTION_KEY", Buffer.alloc(32, 7).toString("base64")),
);
afterEach(() => vi.unstubAllEnvs());

it("encrypts the same token with independent nonces and authenticates the result", () => {
  const first = encrypt("refresh-token"),
    second = encrypt("refresh-token");
  expect(first).not.toBe(second);
  for (const ciphertext of [first, second]) {
    expect(ciphertext).not.toContain("refresh-token");
    expect(decrypt(ciphertext)).toBe("refresh-token");
  }
  // Changing the nonce, tag or payload must invalidate authentication.
  for (const offset of [0, 12, 28]) {
    const bytes = Buffer.from(first, "base64");
    bytes[offset] ^= 1;
    expect(() => decrypt(bytes.toString("base64"))).toThrow();
  }
  vi.stubEnv("TOKEN_ENCRYPTION_KEY", Buffer.alloc(32, 8).toString("base64"));
  expect(() => decrypt(first)).toThrow();
});
