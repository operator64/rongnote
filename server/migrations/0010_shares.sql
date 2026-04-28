-- v1.2: read-only share links for notes.
--
-- Owner generates a random share_key client-side, decrypts the note,
-- re-encrypts with share_key, ships ciphertext + token to the server.
-- The share_key never reaches the server — it lives in the URL fragment
-- (#…) which browsers don't transmit. Recipients with the URL can decrypt
-- locally; anyone with just the token can fetch ciphertext but not read.

CREATE TABLE share_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- URL-safe random token. Server identifier.
    token TEXT UNIQUE NOT NULL,
    -- nonce(24) || secretbox(plaintext, share_key)
    encrypted_payload BYTEA NOT NULL,
    expires_at TIMESTAMPTZ,
    use_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX share_links_owner_idx ON share_links(owner_user_id);
CREATE INDEX share_links_item_idx ON share_links(item_id);
