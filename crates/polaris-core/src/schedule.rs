//! Calibration scheduling rules: interval arithmetic, derived dates, and due status.

use chrono::{Days, Months, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use crate::model::{Asset, CalInterval, DueStatus, IntervalUnit};

/// Items due within this many days are flagged "due soon".
pub const DUE_SOON_DAYS: i64 = 30;

/// Adds a calibration interval to a date. Month arithmetic clamps to the end
/// of the month, so Jan 31 + 1 month = Feb 28 (or 29).
pub fn add_interval(date: NaiveDate, interval: CalInterval) -> Option<NaiveDate> {
    match interval.unit {
        IntervalUnit::Days => date.checked_add_days(Days::new(interval.value.into())),
        IntervalUnit::Months => date.checked_add_months(Months::new(interval.value)),
    }
}

/// The schedule dates stored on an asset that the system maintains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedDates {
    pub first_cal_date: Option<NaiveDate>,
    pub last_cal_date: Option<NaiveDate>,
    pub next_cal_date: Option<NaiveDate>,
}

/// Works out an asset's first/last/next calibration dates.
///
/// * `first` is a recorded fact: kept if set, otherwise filled from the earliest record.
/// * `last` comes from the latest (non-archived) record; with no records the
///   manually entered value is kept, which supports items brought in from an older system.
/// * `next` is always `last + interval`, or empty if either is missing.
pub fn derive_dates(
    stored_first: Option<NaiveDate>,
    stored_last: Option<NaiveDate>,
    record_dates: &[NaiveDate],
    interval: Option<CalInterval>,
) -> DerivedDates {
    let earliest = record_dates.iter().min().copied();
    let latest = record_dates.iter().max().copied();
    let last_cal_date = latest.or(stored_last);
    DerivedDates {
        first_cal_date: stored_first.or(earliest),
        last_cal_date,
        next_cal_date: last_cal_date
            .zip(interval)
            .and_then(|(d, i)| add_interval(d, i)),
    }
}

pub fn due_status(next: Option<NaiveDate>, status_tracks_due: bool, today: NaiveDate) -> DueStatus {
    if !status_tracks_due {
        return DueStatus::NotTracked;
    }
    match next {
        None => DueStatus::NoSchedule,
        Some(d) if d < today => DueStatus::Overdue,
        Some(d) if (d - today).num_days() <= DUE_SOON_DAYS => DueStatus::DueSoon,
        Some(_) => DueStatus::Current,
    }
}

/// Counts for the dashboard. Buckets are exclusive: an overdue item is not
/// also counted as due within 30 days.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DueSummary {
    pub today: NaiveDate,
    pub total: i64,
    pub overdue: i64,
    pub due_30: i64,
    pub due_31_60: i64,
    pub due_61_90: i64,
    pub current: i64,
    pub no_schedule: i64,
    pub not_tracked: i64,
    /// Overdue and due-within-30-days items, soonest first.
    pub attention: Vec<Asset>,
}

impl DueSummary {
    /// Summarizes non-archived assets. Callers pass the active asset list.
    pub fn from_assets(assets: &[Asset], today: NaiveDate) -> Self {
        let mut s = DueSummary {
            today,
            ..Default::default()
        };
        for a in assets.iter().filter(|a| !a.is_archived) {
            s.total += 1;
            match a.due_status {
                DueStatus::NotTracked => s.not_tracked += 1,
                DueStatus::NoSchedule => s.no_schedule += 1,
                DueStatus::Overdue => s.overdue += 1,
                DueStatus::DueSoon => s.due_30 += 1,
                DueStatus::Current => {
                    let days = a.next_cal_date.map_or(i64::MAX, |d| (d - today).num_days());
                    // DueSoon already covers 0..=30, so Current starts at 31 days out.
                    match days {
                        ..=60 => s.due_31_60 += 1,
                        61..=90 => s.due_61_90 += 1,
                        _ => s.current += 1,
                    }
                }
            }
            if matches!(a.due_status, DueStatus::Overdue | DueStatus::DueSoon) {
                s.attention.push(a.clone());
            }
        }
        s.attention
            .sort_by_key(|a| (a.next_cal_date, a.asset_tag.clone()));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    const MONTHS_12: CalInterval = CalInterval {
        value: 12,
        unit: IntervalUnit::Months,
    };

    #[test]
    fn month_intervals_clamp_to_month_end() {
        let one_month = CalInterval {
            value: 1,
            unit: IntervalUnit::Months,
        };
        assert_eq!(
            add_interval(d(2027, 1, 31), one_month),
            Some(d(2027, 2, 28))
        );
        assert_eq!(
            add_interval(d(2028, 1, 31), one_month),
            Some(d(2028, 2, 29))
        );
        assert_eq!(
            add_interval(d(2026, 3, 15), MONTHS_12),
            Some(d(2027, 3, 15))
        );
    }

    #[test]
    fn day_intervals() {
        let ninety = CalInterval {
            value: 90,
            unit: IntervalUnit::Days,
        };
        assert_eq!(add_interval(d(2026, 1, 1), ninety), Some(d(2026, 4, 1)));
    }

    #[test]
    fn derive_with_no_records_keeps_manual_dates() {
        let r = derive_dates(
            Some(d(2020, 5, 1)),
            Some(d(2026, 2, 1)),
            &[],
            Some(MONTHS_12),
        );
        assert_eq!(r.first_cal_date, Some(d(2020, 5, 1)));
        assert_eq!(r.last_cal_date, Some(d(2026, 2, 1)));
        assert_eq!(r.next_cal_date, Some(d(2027, 2, 1)));
    }

    #[test]
    fn derive_uses_latest_record_and_fills_first() {
        let records = [d(2025, 6, 1), d(2026, 6, 1), d(2024, 6, 1)];
        let r = derive_dates(None, Some(d(2023, 1, 1)), &records, Some(MONTHS_12));
        assert_eq!(r.first_cal_date, Some(d(2024, 6, 1)));
        assert_eq!(r.last_cal_date, Some(d(2026, 6, 1)));
        assert_eq!(r.next_cal_date, Some(d(2027, 6, 1)));
    }

    #[test]
    fn derive_keeps_recorded_first_date() {
        let r = derive_dates(Some(d(2019, 1, 1)), None, &[d(2026, 6, 1)], None);
        assert_eq!(r.first_cal_date, Some(d(2019, 1, 1)));
        assert_eq!(r.next_cal_date, None, "no interval means no next date");
    }

    #[test]
    fn due_status_boundaries() {
        let today = d(2026, 11, 20);
        assert_eq!(
            due_status(Some(d(2026, 11, 19)), true, today),
            DueStatus::Overdue
        );
        assert_eq!(due_status(Some(today), true, today), DueStatus::DueSoon);
        assert_eq!(
            due_status(Some(d(2026, 12, 20)), true, today),
            DueStatus::DueSoon
        );
        assert_eq!(
            due_status(Some(d(2026, 12, 21)), true, today),
            DueStatus::Current
        );
        assert_eq!(due_status(None, true, today), DueStatus::NoSchedule);
        assert_eq!(
            due_status(Some(d(2020, 1, 1)), false, today),
            DueStatus::NotTracked
        );
    }
}
