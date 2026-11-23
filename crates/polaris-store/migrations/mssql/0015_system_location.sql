-- Instruments in a System take the System's location and department. Brings existing
-- Instruments in line once; from now on saves keep them in step.
UPDATE c
   SET c.location_id = p.location_id, c.department_id = p.department_id
  FROM assets c
  JOIN assets p ON p.id = c.parent_id;
