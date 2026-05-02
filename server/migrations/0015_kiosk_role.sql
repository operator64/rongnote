-- v1.6: add 'kiosk' membership role.
--
-- Kiosk users sit between viewer and editor. Intended for an always-on
-- shared display (tablet on the wall) where any household member can
-- create new events or check off shopping-list entries, but the kiosk
-- itself can't reach back and modify content other people created.
--
-- Server enforcement (items.rs):
--   create  — allowed (any item type)
--   update  — allowed iff items.created_by = self  OR  items.type = 'list'
--   delete  — forbidden
--   move    — forbidden
--   read    — allowed (full space contents, like editor)
--
-- The list-type carve-out is what makes the shared shopping list usable
-- without giving the kiosk write access to events / notes / secrets that
-- other members own.

ALTER TABLE memberships DROP CONSTRAINT memberships_role_check;
ALTER TABLE memberships ADD CONSTRAINT memberships_role_check CHECK (
    role IN ('owner', 'editor', 'viewer', 'kiosk')
);
