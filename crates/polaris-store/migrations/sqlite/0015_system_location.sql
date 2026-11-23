-- Instruments in a System take the System's location and department. Brings existing
-- Instruments in line once; from now on saves keep them in step.
UPDATE assets
   SET location_id   = (SELECT p.location_id   FROM assets p WHERE p.id = assets.parent_id),
       department_id = (SELECT p.department_id FROM assets p WHERE p.id = assets.parent_id)
 WHERE parent_id IS NOT NULL;
