-- Instruments in a System take the System's location and department. Brings existing
-- Instruments in line once; from now on saves keep them in step.
UPDATE assets c
   SET location_id = p.location_id, department_id = p.department_id
  FROM assets p
 WHERE p.id = c.parent_id;
