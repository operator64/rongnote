-- v1.4: add 'list' to the items.type enum (CHECK constraint).
--
-- Lists are checklist-style items (shopping lists, packing lists). Body
-- payload is { entries: [{id, text, done}] }, encrypted like every other
-- item type.

ALTER TABLE items DROP CONSTRAINT items_type_check;
ALTER TABLE items ADD CONSTRAINT items_type_check CHECK (
    type IN ('note', 'secret', 'file', 'event', 'task', 'snippet', 'bookmark', 'list')
);
