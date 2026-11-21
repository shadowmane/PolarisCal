-- Due on the last day of the month the interval lands in.
ALTER TABLE assets ADD COLUMN cal_interval_end_of_month INTEGER NOT NULL DEFAULT 0
    CHECK (cal_interval_end_of_month IN (0, 1));
