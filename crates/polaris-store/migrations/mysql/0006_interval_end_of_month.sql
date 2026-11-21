-- Due on the last day of the month the interval lands in.
ALTER TABLE assets ADD COLUMN cal_interval_end_of_month BOOLEAN NOT NULL DEFAULT FALSE;
