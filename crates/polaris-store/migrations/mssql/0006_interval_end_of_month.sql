-- Due on the last day of the month the interval lands in.
ALTER TABLE assets ADD cal_interval_end_of_month BIT NOT NULL
    CONSTRAINT df_assets_cal_interval_end_of_month DEFAULT 0;
