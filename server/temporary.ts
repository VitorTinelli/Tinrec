import {
  S3Client,
  CreateMultipartUploadCommand,
  ListPartsCommand,
  UploadPartCommand,
  CompleteMultipartUploadCommand,
  HeadObjectCommand,
  GetObjectCommand,
  DeleteObjectCommand,
  AbortMultipartUploadCommand,
} from "@aws-sdk/client-s3";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
import type { Database } from "./db.js";
import { HttpError } from "./security.js";
export const PART_BYTES = 8 * 1024 * 1024;
const MAX_FILE = 2 * 1024 ** 3;
const MAX_STORAGE = 8 * 1024 ** 3;
export interface TemporaryStorage {
  ready(): boolean;
  start(key: string): Promise<string>;
  parts(
    key: string,
    id: string,
  ): Promise<{ number: number; size: number; etag: string }[]>;
  signPart(
    key: string,
    id: string,
    number: number,
    length: number,
  ): Promise<string>;
  complete(
    key: string,
    id: string,
    parts: { number: number; etag: string }[],
  ): Promise<void>;
  size(key: string): Promise<number | null>;
  playback(key: string): Promise<string>;
  remove(key: string): Promise<void>;
  abort(key: string, id: string): Promise<void>;
}
export function r2Storage(): TemporaryStorage {
  let client: S3Client;
  const bucket = () => process.env.R2_BUCKET!;
  const s3 = () =>
    (client ??= new S3Client({
      region: "auto",
      endpoint: process.env.R2_ENDPOINT,
      forcePathStyle: true,
      maxAttempts: 2,
      requestChecksumCalculation: "WHEN_REQUIRED",
      responseChecksumValidation: "WHEN_REQUIRED",
      credentials: {
        accessKeyId: process.env.R2_ACCESS_KEY_ID!,
        secretAccessKey: process.env.R2_SECRET_ACCESS_KEY!,
      },
    }));
  return {
    ready: () =>
      [
        "R2_ENDPOINT",
        "R2_BUCKET",
        "R2_ACCESS_KEY_ID",
        "R2_SECRET_ACCESS_KEY",
      ].every((k) => !!process.env[k]),
    async start(key) {
      const r = await s3().send(
        new CreateMultipartUploadCommand({
          Bucket: bucket(),
          Key: key,
          ContentType: "video/mp4",
          CacheControl: "private, max-age=900",
        }),
      );
      if (!r.UploadId) throw Error("Missing upload ID");
      return r.UploadId;
    },
    async parts(key, id) {
      const r = await s3().send(
        new ListPartsCommand({
          Bucket: bucket(),
          Key: key,
          UploadId: id,
          MaxParts: 1000,
        }),
      );
      return (r.Parts ?? []).map((p) => ({
        number: p.PartNumber!,
        size: p.Size!,
        etag: p.ETag!,
      }));
    },
    async signPart(key, id, number, length) {
      return getSignedUrl(
        s3(),
        new UploadPartCommand({
          Bucket: bucket(),
          Key: key,
          UploadId: id,
          PartNumber: number,
          ContentLength: length,
        }),
        { expiresIn: 900 },
      );
    },
    async complete(key, id, parts) {
      await s3().send(
        new CompleteMultipartUploadCommand({
          Bucket: bucket(),
          Key: key,
          UploadId: id,
          MultipartUpload: {
            Parts: parts.map((p) => ({ PartNumber: p.number, ETag: p.etag })),
          },
        }),
      );
    },
    async size(key) {
      try {
        return Number(
          (
            await s3().send(
              new HeadObjectCommand({ Bucket: bucket(), Key: key }),
            )
          ).ContentLength,
        );
      } catch (e: any) {
        if (e.$metadata?.httpStatusCode === 404) return null;
        throw e;
      }
    },
    async playback(key) {
      return getSignedUrl(
        s3(),
        new GetObjectCommand({ Bucket: bucket(), Key: key }),
        { expiresIn: 3600 },
      );
    },
    async remove(key) {
      await s3().send(new DeleteObjectCommand({ Bucket: bucket(), Key: key }));
    },
    async abort(key, id) {
      try {
        await s3().send(
          new AbortMultipartUploadCommand({
            Bucket: bucket(),
            Key: key,
            UploadId: id,
          }),
        );
      } catch (e: any) {
        if (e.name !== "NoSuchUpload") throw e;
      }
    },
  };
}
export function temporaryVideos(db: Database, storage: TemporaryStorage) {
  async function upload(r: any, size: number, duration: number): Promise<any> {
    if (!storage.ready()) return { available: false };
    if (r.temp_deleted_at || r.drive_ready_at) return { available: false };
    if (r.temp_ready) return { completed: true };
    if (size > MAX_FILE) return { available: false, reason: "size_limit" };
    if (Number(r.temp_size_bytes) > 0 && Number(r.temp_size_bytes) !== size)
      throw new HttpError(
        409,
        "size_changed",
        "O arquivo local mudou desde o início do envio.",
      );
    if (!Number(r.temp_size_bytes)) {
      // One atomic budget reservation also serializes concurrent reservations for the same video.
      const { rows } = await db.query(
        `WITH budget AS (
        UPDATE temporary_budget SET used_bytes=used_bytes+$1 WHERE id=1 AND used_bytes+$1<=$2
        AND NOT EXISTS(SELECT 1 FROM recordings WHERE id=$3 AND temp_size_bytes>0) RETURNING id)
        UPDATE recordings SET temp_size_bytes=$1,size_bytes=$1,duration_ms=$4,status=CASE WHEN status='recording' THEN 'uploading' ELSE status END,temp_created_at=now()
        WHERE id=$3 AND EXISTS(SELECT 1 FROM budget) RETURNING *`,
        [size, MAX_STORAGE, r.id, duration],
      );
      if (!rows.length) {
        const {
          rows: [current],
        } = await db.query("SELECT * FROM recordings WHERE id=$1", [r.id]);
        if (!Number(current?.temp_size_bytes))
          return { available: false, reason: "capacity" };
        r = current;
      } else r = rows[0];
    }
    const key = `temporary/${r.public_id}.mp4`;
    // HEAD recovers a completion whose response or database update was interrupted.
    const actual = await storage.size(key);
    if (actual !== null) {
      if (actual !== size)
        throw new HttpError(
          409,
          "temporary_size",
          "A cópia temporária está incompleta.",
        );
      await db.query(
        "UPDATE recordings SET temp_ready=true,temp_upload_id=NULL WHERE id=$1",
        [r.id],
      );
      return { completed: true };
    }
    let id = r.temp_upload_id;
    if (!id) {
      id = await storage.start(key);
      const {
        rows: [saved],
      } = await db.query(
        "UPDATE recordings SET temp_upload_id=COALESCE(temp_upload_id,$1) WHERE id=$2 RETURNING temp_upload_id",
        [id, r.id],
      );
      if (saved.temp_upload_id !== id) {
        await storage.abort(key, id);
        id = saved.temp_upload_id;
      }
    }
    let parts;
    try {
      parts = await storage.parts(key, id);
    } catch (e: any) {
      if (e.name !== "NoSuchUpload") throw e;
      await db.query(
        "UPDATE recordings SET temp_upload_id=NULL WHERE id=$1 AND temp_upload_id=$2",
        [r.id, id],
      );
      return { retry: true };
    }
    parts.sort((a, b) => a.number - b.number);
    let offset = 0;
    for (const [index, part] of parts.entries()) {
      if (
        part.number !== index + 1 ||
        part.size !== Math.min(PART_BYTES, size - offset) ||
        !part.etag
      )
        throw new HttpError(
          409,
          "temporary_parts",
          "O R2 retornou blocos inconsistentes. O vídeo local foi preservado.",
        );
      offset += part.size;
    }
    if (offset === size) {
      await storage.complete(key, id, parts);
      if ((await storage.size(key)) !== size)
        throw new HttpError(
          409,
          "temporary_size",
          "Não foi possível confirmar o tamanho da cópia temporária.",
        );
      await db.query(
        "UPDATE recordings SET temp_ready=true,temp_upload_id=NULL WHERE id=$1",
        [r.id],
      );
      return { completed: true };
    }
    const length = Math.min(PART_BYTES, size - offset),
      partNumber = parts.length + 1;
    return {
      url: await storage.signPart(key, id, partNumber, length),
      offset,
      length,
      partNumber,
    };
  }
  async function cleanup(r: any) {
    if (
      !storage.ready() ||
      !r.drive_ready_at ||
      !r.temp_delete_after ||
      new Date(r.temp_delete_after).getTime() > Date.now() ||
      r.temp_deleted_at ||
      !Number(r.temp_size_bytes)
    )
      return false;
    const key = `temporary/${r.public_id}.mp4`;
    await storage.remove(key);
    if (r.temp_upload_id) await storage.abort(key, r.temp_upload_id);
    await db.query(
      `WITH removed AS (UPDATE recordings SET temp_deleted_at=now(),temp_ready=false,temp_upload_id=NULL
      WHERE id=$1 AND temp_deleted_at IS NULL RETURNING temp_size_bytes)
      UPDATE temporary_budget SET used_bytes=GREATEST(0,used_bytes-COALESCE((SELECT temp_size_bytes FROM removed),0)) WHERE id=1`,
      [r.id],
    );
    return true;
  }
  return { upload, cleanup, storage };
}
