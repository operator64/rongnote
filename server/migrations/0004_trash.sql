-- v0.5.2: soft delete.
--
-- An item with deleted_at IS NOT NULL is in the trash. The list endpoint
-- filters on this by default. Items in trash can be restored or hard-deleted.
-- Retention (e.g. cron-cleanup of items older than 30 days in trash) lives
-- on the host, not in the app — see deploy.md.

ALTER TABLE items ADD COLUMN deleted_at TIMESTAMPTZ;
CREATE INDEX items_deleted_at_idx ON items(deleted_at) WHERE deleted_at IS NOT NULL;
