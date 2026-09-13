import {
  randomBytes,
  createHash,
  createCipheriv,
  createDecipheriv,
  timingSafeEqual,
} from "node:crypto";
export const token = (bytes = 32) => randomBytes(bytes).toString("base64url");
export const hash = (value: string) =>
  createHash("sha256").update(value).digest("hex");
export function same(a: string, b: string) {
  const x = Buffer.from(a),
    y = Buffer.from(b);
  return x.length === y.length && timingSafeEqual(x, y);
}
function key() {
  const k = Buffer.from(process.env.TOKEN_ENCRYPTION_KEY ?? "", "base64");
  if (k.length !== 32) throw new Error("Invalid encryption key");
  return k;
}
export function encrypt(value: string) {
  const iv = randomBytes(12),
    c = createCipheriv("aes-256-gcm", key(), iv);
  const data = Buffer.concat([c.update(value, "utf8"), c.final()]);
  return Buffer.concat([iv, c.getAuthTag(), data]).toString("base64");
}
export function decrypt(value: string) {
  const b = Buffer.from(value, "base64"),
    d = createDecipheriv("aes-256-gcm", key(), b.subarray(0, 12));
  d.setAuthTag(b.subarray(12, 28));
  return Buffer.concat([d.update(b.subarray(28)), d.final()]).toString("utf8");
}
export class HttpError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}
export const configReady = () =>
  [
    "APP_URL",
    "DATABASE_URL",
    "OWNER_EMAIL",
    "GOOGLE_CLIENT_ID",
    "GOOGLE_CLIENT_SECRET",
    "TOKEN_ENCRYPTION_KEY",
  ].every((k) => !!process.env[k]);
export const baseUrl = () =>
  new URL(process.env.APP_URL ?? "http://localhost:5173").origin;
