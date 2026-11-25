-- Check-out: an item's Owner is who it's checked out to. owner_since records when it
-- went to them (unknown for items assigned before this).
ALTER TABLE assets ADD COLUMN owner_since TEXT;
