-- Aggregate Systems: a System can take its schedule from its Instruments instead of
-- having its own cycle. Its next calibration is then the soonest next calibration among
-- its tracked, unarchived Instruments, worked out when read.
ALTER TABLE assets ADD cal_aggregate BIT NOT NULL CONSTRAINT df_assets_cal_aggregate DEFAULT 0;
