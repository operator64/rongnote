-- v0.7: encrypted file blobs.
--
-- Each file lives as an item (type='file') referencing one blob via sha256.
-- Blob bytes are stored on disk under $DATA_DIR/blobs/ab/cdef..., not in the
-- database. The hash is over the ciphertext, so dedup happens whenever two
-- uploads produce identical encrypted bytes (same plaintext + same per-file
-- key + same nonce — vanishingly rare across users, common when re-uploading).
--
-- Server can never decrypt blobs: per-file key is wrapped with master_key
-- and lives in items.wrapped_item_key alongside the encrypted metadata in
-- items.encrypted_body (JSON: filename, mime, size).

CREATE TABLE files_blobs (
    sha256 BYTEA PRIMARY KEY,
    size BIGINT NOT NULL,
    refcount INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE items ADD COLUMN blob_sha256 BYTEA REFERENCES files_blobs(sha256);
CREATE INDEX items_blob_sha256_idx ON items(blob_sha256) WHERE blob_sha256 IS NOT NULL;
