-- v1.5: groundwork for team spaces.
--
-- Personal-space items keep wrapping the per-item key with master_key
-- (items.wrapped_item_key). Team-space items are sealed-box-encrypted to
-- each member's public key and stored here, one row per (item, member).
-- Phase A only adds the table — items continue to use master_key wraps
-- for now. Phase B switches the create/update path for team-space items.

CREATE TABLE item_member_keys (
    item_id UUID NOT NULL REFERENCES items(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sealed_item_key BYTEA NOT NULL,
    PRIMARY KEY (item_id, user_id)
);
CREATE INDEX item_member_keys_user_idx ON item_member_keys(user_id);
