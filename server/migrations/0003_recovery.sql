-- v0.3: master-key refactor + recovery code.
--
-- The master_key is no longer derived from the passphrase. It's a random
-- 32-byte key, wrapped twice on the server:
--   master_wrap_passphrase = secretbox(master_key, kek_passphrase)
--   master_wrap_recovery   = secretbox(master_key, kek_recovery)
-- where kek_passphrase = Argon2id(passphrase,    passphrase_salt)
-- and   kek_recovery   = Argon2id(recovery_code, recovery_salt).
--
-- The recovery_code is a 24-char base32 string the user sees exactly once at
-- registration. It lets them reset their passphrase without losing notes.
-- Server can't see plaintext at any point.
--
-- Destructive: any v0.2 user has incompatible columns (no master wrap, no
-- recovery_salt). Easier to nuke and re-register than retrofit.

TRUNCATE TABLE sessions, items, memberships, spaces, users CASCADE;

ALTER TABLE users RENAME COLUMN salt TO passphrase_salt;

ALTER TABLE users ADD COLUMN recovery_salt BYTEA NOT NULL;
ALTER TABLE users ADD COLUMN master_wrap_passphrase BYTEA NOT NULL;
ALTER TABLE users ADD COLUMN master_wrap_recovery BYTEA NOT NULL;
