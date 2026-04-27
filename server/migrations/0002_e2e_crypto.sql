-- v0.2: client-side encryption.
--
-- Server stops seeing note bodies and master keys. The browser derives
-- master_key = Argon2id(passphrase, salt) locally and computes
-- auth_hash   = BLAKE2b(master_key, "rongnote-auth-v1") as the authenticator.
-- Server hashes auth_hash with its own Argon2id+salt and stores that.
--
-- This is a destructive migration: existing plaintext bodies are dropped,
-- and existing users (with the old password_hash) cannot authenticate
-- under the new flow. Any logged-in sessions are also invalidated.
--
-- We nuke users before adding the NOT NULL crypto columns. Items reference
-- users via items.created_by without ON DELETE CASCADE, so order matters.

TRUNCATE TABLE sessions, items, memberships, spaces, users CASCADE;

-- Old plaintext fields gone.
ALTER TABLE items DROP COLUMN body;
ALTER TABLE users DROP COLUMN password_hash;

-- New auth + per-user crypto state.
ALTER TABLE users ADD COLUMN salt BYTEA;
ALTER TABLE users ADD COLUMN auth_hash TEXT;          -- Argon2id PHC string of the client-supplied auth_hash
ALTER TABLE users ADD COLUMN public_key BYTEA;        -- X25519 public key (32 bytes)
ALTER TABLE users ADD COLUMN encrypted_private_key BYTEA;  -- nonce(24) || ciphertext(48), wrapped with master_key

ALTER TABLE users ALTER COLUMN salt SET NOT NULL;
ALTER TABLE users ALTER COLUMN auth_hash SET NOT NULL;
ALTER TABLE users ALTER COLUMN public_key SET NOT NULL;
ALTER TABLE users ALTER COLUMN encrypted_private_key SET NOT NULL;

-- Per-item encrypted body. nonce(24) || ciphertext(N+16) for both fields.
ALTER TABLE items ADD COLUMN encrypted_body BYTEA;       -- secretbox of the markdown
ALTER TABLE items ADD COLUMN wrapped_item_key BYTEA;     -- secretbox of the per-item XChaCha20 key, wrapped with master_key

-- For v0.2 we only have personal spaces, so wrapping is symmetric (master_key directly).
-- v0.8 (team spaces) will add a per-(item,member) wrap row in a separate table.
