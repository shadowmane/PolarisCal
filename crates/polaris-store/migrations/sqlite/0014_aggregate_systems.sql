-- Aggregate Systems: a System can take its schedule from its Instruments instead of
-- having its own cycle. Its next calibration is then the soonest next calibration among
-- its tracked, unarchived Instruments, worked out when read.
ALTER TABLE assets ADD COLUMN cal_aggregate INTEGER NOT NULL DEFAULT 0 CHECK (cal_aggregate IN (0, 1));
