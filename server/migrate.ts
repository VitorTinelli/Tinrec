import "dotenv/config";
import { readFile } from "node:fs/promises";
import pg from "pg";
const client = new pg.Client({ connectionString: process.env.DATABASE_URL });
await client.connect();
try {
  await client.query(
    await readFile(new URL("./schema.sql", import.meta.url), "utf8"),
  );
  console.log("Banco Tinrec atualizado.");
} finally {
  await client.end();
}
