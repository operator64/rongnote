-- v0.8: tasks need due-date + done state visible at list level so the UI
-- can render checkboxes + due dates without decrypting every item.
-- Both fields stay clear-text on the server (timestamps + boolean state are
-- already server-visible per spec §4). Task description / notes remain
-- encrypted in items.encrypted_body.

ALTER TABLE items ADD COLUMN due_at DATE;
ALTER TABLE items ADD COLUMN done BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX items_due_at_idx ON items(due_at)
    WHERE due_at IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX items_open_tasks_idx ON items(due_at)
    WHERE type = 'task' AND done = FALSE AND deleted_at IS NULL;
