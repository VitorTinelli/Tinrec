CREATE TABLE IF NOT EXISTS users (
 id text PRIMARY KEY, email text NOT NULL UNIQUE, refresh_token text, drive_folder_id text,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS sessions (
 token_hash text PRIMARY KEY, user_id text NOT NULL REFERENCES users(id),
 kind text NOT NULL CHECK(kind IN ('web','device')), expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS oauth_states (
 state_hash text PRIMARY KEY, verifier text NOT NULL, expires_at timestamptz NOT NULL
);
CREATE TABLE IF NOT EXISTS device_codes (
 code_hash text PRIMARY KEY, secret_hash text NOT NULL, user_id text REFERENCES users(id),
 expires_at timestamptz NOT NULL, consumed_at timestamptz
);
CREATE TABLE IF NOT EXISTS recordings (
 id uuid PRIMARY KEY, owner_id text NOT NULL REFERENCES users(id), public_id text NOT NULL UNIQUE,
 title text NOT NULL CHECK(length(title) BETWEEN 1 AND 180), created_at timestamptz NOT NULL DEFAULT now(),
 duration_ms bigint NOT NULL DEFAULT 0 CHECK(duration_ms >= 0), resolution integer NOT NULL CHECK(resolution IN(720,1080)),
 status text NOT NULL DEFAULT 'recording' CHECK(status IN('recording','uploading','uploaded','error')),
 size_bytes bigint NOT NULL DEFAULT 0, uploaded_bytes bigint NOT NULL DEFAULT 0,
 drive_file_id text, upload_uri text, error_code text,
 UNIQUE(owner_id,id)
);
CREATE INDEX IF NOT EXISTS recordings_owner_created ON recordings(owner_id,created_at DESC);
CREATE INDEX IF NOT EXISTS sessions_expiry ON sessions(expires_at);
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_size_bytes bigint NOT NULL DEFAULT 0;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_upload_id text;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_ready boolean NOT NULL DEFAULT false;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_created_at timestamptz;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS drive_ready_at timestamptz;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_delete_after timestamptz;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS temp_deleted_at timestamptz;
CREATE TABLE IF NOT EXISTS temporary_budget (id integer PRIMARY KEY CHECK(id=1),used_bytes bigint NOT NULL DEFAULT 0 CHECK(used_bytes>=0));
INSERT INTO temporary_budget(id) VALUES(1) ON CONFLICT(id) DO NOTHING;
CREATE INDEX IF NOT EXISTS recordings_temp_cleanup ON recordings(temp_delete_after) WHERE temp_deleted_at IS NULL AND drive_ready_at IS NOT NULL;
ALTER TABLE recordings ADD COLUMN IF NOT EXISTS media_type text NOT NULL DEFAULT 'video' CHECK(media_type IN ('video','image'));
