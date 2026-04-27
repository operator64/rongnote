-- v0.6: WebAuthn / Passkeys.
--
-- One row per registered authenticator. Multiple passkeys per user are
-- supported by the schema; the v0.6 UI only supports one (extending
-- later is just UI work).
--
-- master_wrap_passkey is master_key encrypted with a kek derived from the
-- passkey's PRF output. Server can't unwrap it — only the authenticator,
-- when present, can.

CREATE TABLE passkeys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id BYTEA UNIQUE NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    -- Serialized webauthn-rs Passkey state (CBOR-ish, opaque to us).
    credential JSONB NOT NULL,
    master_wrap_passkey BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ
);
CREATE INDEX passkeys_user_id_idx ON passkeys(user_id);
