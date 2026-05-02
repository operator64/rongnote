-- v1.x: event item type with start/end timestamps for the calendar view.
-- The 'event' type was already in the items.type CHECK from 0001 — we
-- only add the time columns now.
--
-- Times are stored UTC. The UI renders in user-local. all_day is a flag,
-- not a separate column: for all-day events, start_at is YYYY-MM-DD 00:00 UTC
-- and end_at is the next-midnight (exclusive end, like iCal DTEND semantics).

ALTER TABLE items
    ADD COLUMN start_at TIMESTAMPTZ,
    ADD COLUMN end_at   TIMESTAMPTZ,
    ADD COLUMN all_day  BOOLEAN NOT NULL DEFAULT FALSE;

-- Partial index: only events have start_at, only non-trashed are visible
-- in the calendar.
CREATE INDEX items_start_at_idx
    ON items(space_id, start_at)
    WHERE start_at IS NOT NULL AND deleted_at IS NULL;
