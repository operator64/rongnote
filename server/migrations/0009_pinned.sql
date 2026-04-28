-- v1.1: pinned items rise to the top of the list. Spec §3 caps at 5
-- per space; we enforce in the UI, not in the DB.

ALTER TABLE items ADD COLUMN pinned BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX items_pinned_idx ON items(space_id) WHERE pinned = TRUE AND deleted_at IS NULL;
