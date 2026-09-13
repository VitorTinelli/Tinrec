import pg from "pg";
export interface Database {
  query<T = Record<string, any>>(
    sql: string,
    params?: unknown[],
  ): Promise<{ rows: T[] }>;
}
let pool: pg.Pool | undefined;
export const db: Database = {
  async query<T>(sql: string, params?: unknown[]) {
    pool ??= new pg.Pool({
      connectionString: process.env.DATABASE_URL,
      max: 3,
      connectionTimeoutMillis: 10_000,
      idleTimeoutMillis: 10_000,
    });
    return (await pool.query(sql, params)) as unknown as { rows: T[] };
  },
};
