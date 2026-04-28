-- v1.3: snapshot every body-change so users can roll back. Server-side
-- the snapshot looks like another opaque ciphertext blob — same wrapping
-- as the current items row at the time of save.

CREATE TABLE item_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    encrypted_body BYTEA,
    wrapped_item_key BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (item_id, version)
);
CREATE INDEX item_versions_item_idx ON item_versions(item_id, version DESC);
