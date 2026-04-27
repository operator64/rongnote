-- v0.9: audit log.
--
-- Per spec §3, secret reads MUST be logged. Notes/files/tasks logging is
-- optional but cheap so we record everything that happens to user data plus
-- auth events. Reads of non-secret item types are NOT logged (avoid noise
-- and "did the server look at my notes" anxiety).

CREATE TABLE audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    space_id UUID REFERENCES spaces(id) ON DELETE SET NULL,
    item_id UUID REFERENCES items(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    meta JSONB,
    ts TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX audit_log_user_id_ts_idx ON audit_log(user_id, ts DESC);
CREATE INDEX audit_log_item_id_ts_idx ON audit_log(item_id, ts DESC) WHERE item_id IS NOT NULL;
