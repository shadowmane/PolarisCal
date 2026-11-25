//! Calibration scheduling rules: interval arithmetic, derived dates, and due status.

use chrono::{Datelike, Days, Months, NaiveDate};
use serde::Serialize;
use ts_rs::TS;

use crate::model::{Asset, CalInterval, DueStatus, IntervalUnit};

/// Items due within this many days are flagged "due soon".
pub const DUE_SOON_DAYS: i64 = 30;

/// Adds a calibration interval to a date. Month arithmetic clamps to the end
/// of the month, so Jan 31 + 1 month = Feb 28 (or 29).
pub fn add_interval(date: NaiveDate, interval: CalInterval) -> Option<NaiveDate> {
    let due = match interval.unit {
        IntervalUnit::Days => date.checked_add_days(Days::new(interval.value.into())),
        IntervalUnit::Months => date.checked_add_months(Months::new(interval.value)),
    }?;
    if interval.end_of_month {
        end_of_month(due)
    } else {
        Some(due)
    }
}

/// The last day of `date`'s month.
pub fn end_of_month(date: NaiveDate) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1)?
        .checked_add_months(Months::new(1))?
        .pred_opt()
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
/// * `first` is a recorded fact: kept if set, otherwise filled with the earliest
///   calibration known, which is the earliest record or a previously entered last date.
/// * `last` comes from the latest (non-archived) record; with no records the
///   manually entered value is kept, which supports items brought in from an older system.
/// * `next` is always `last + interval`, or empty if either is missing.
pub fn derive_dates(
    stored_first: Option<NaiveDate>,
    stored_last: Option<NaiveDate>,
    record_dates: &[NaiveDate],
    interval: Option<CalInterval>,
) -> DerivedDates {
    let earliest_record = record_dates.iter().min().copied();
    let latest = record_dates.iter().max().copied();
    let earliest_known = match (earliest_record, stored_last) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let last_cal_date = latest.or(stored_last);
    DerivedDates {
        first_cal_date: stored_first.or(earliest_known),
        last_cal_date,
        next_cal_date: last_cal_date
            .zip(interval)
            .and_then(|(d, i)| add_interval(d, i)),
    }
}

/// Applies a calibration extension to derived dates. `extension` is the latest active
/// extension record, `(date, days)`. It counts only if it's after the last calibration
/// (a calibration on or after its date ends it); the next calibration then becomes
/// `date + days` and the last calibration is unchanged. An extension never shortens
/// the schedule: one ending on or before the regular due date has no effect.
/// Returns `(next, extended)`.
pub fn apply_extension(
    last_cal_date: Option<NaiveDate>,
    next_cal_date: Option<NaiveDate>,
    extension: Option<(NaiveDate, u32)>,
) -> (Option<NaiveDate>, bool) {
    match extension {
        Some((date, days)) if last_cal_date.is_none_or(|last| date > last) => {
            match date.checked_add_days(Days::new(days.into())) {
                Some(end) if next_cal_date.is_none_or(|next| end > next) => (Some(end), true),
                _ => (next_cal_date, false),
            }
        }
        _ => (next_cal_date, false),
    }
}

/// [`due_status`], except that an item in a calibration extension that hasn't run out
/// is [`DueStatus::Extended`].
pub fn due_status_with_extension(
    next: Option<NaiveDate>,
    status_tracks_due: bool,
    extended: bool,
    today: NaiveDate,
) -> DueStatus {
    match next {
        Some(d) if status_tracks_due && extended && d >= today => DueStatus::Extended,
        _ => due_status(next, status_tracks_due, today),
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

/// The dashboard box an item is counted in. Exclusive: an overdue item is not also
/// due within 30 days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DueBucket {
    /// Overdue, including items in a calibration extension.
    Overdue,
    #[serde(rename = "due_30")]
    Due30,
    #[serde(rename = "due_31_90")]
    Due31To90,
    Current,
    NoSchedule,
}

impl DueBucket {
    /// `None` for items whose status doesn't track due dates.
    pub fn of(asset: &Asset, today: NaiveDate) -> Option<Self> {
        Some(match asset.due_status {
            DueStatus::NotTracked => return None,
            DueStatus::NoSchedule => Self::NoSchedule,
            // Extended items are past due, just authorized for use: count them as overdue.
            DueStatus::Overdue | DueStatus::Extended => Self::Overdue,
            DueStatus::DueSoon => Self::Due30,
            DueStatus::Current => {
                let days = asset
                    .next_cal_date
                    .map_or(i64::MAX, |d| (d - today).num_days());
                // DueSoon already covers 0..=30, so Current starts at 31 days out.
                if days <= 90 {
                    Self::Due31To90
                } else {
                    Self::Current
                }
            }
        })
    }
}

/// An item on the dashboard with the box it's counted in.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DueItem {
    pub bucket: DueBucket,
    pub asset: Asset,
}

/// An item with a calibration interval but no calibration on record: it has no due
/// date yet and needs calibrating now.
pub fn needs_first_calibration(asset: &Asset) -> bool {
    asset.due_status == DueStatus::NoSchedule
        && asset.cal_interval.is_some()
        && asset.last_cal_date.is_none()
}

/// Counts for the dashboard, one per [`DueBucket`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct DueSummary {
    pub today: NaiveDate,
    pub total: i64,
    pub overdue: i64,
    pub due_30: i64,
    /// Due in 31 to 90 days.
    pub due_31_90: i64,
    pub current: i64,
    pub no_schedule: i64,
    pub not_tracked: i64,
    /// Items needing attention: never calibrated despite having an interval, then
    /// overdue and due within 30 days, soonest first.
    pub attention: Vec<Asset>,
    /// Every tracked item with its box, soonest first (no date first), for filtering.
    pub items: Vec<DueItem>,
}

impl DueSummary {
    /// Summarizes non-archived assets. Callers pass the active asset list. Aggregate
    /// Systems are left out: their Instruments are counted instead.
    pub fn from_assets(assets: &[Asset], today: NaiveDate) -> Self {
        let mut s = DueSummary {
            today,
            ..Default::default()
        };
        for a in assets.iter().filter(|a| !a.is_archived && !a.cal_aggregate) {
            s.total += 1;
            let Some(bucket) = DueBucket::of(a, today) else {
                s.not_tracked += 1;
                continue;
            };
            *match bucket {
                DueBucket::Overdue => &mut s.overdue,
                DueBucket::Due30 => &mut s.due_30,
                DueBucket::Due31To90 => &mut s.due_31_90,
                DueBucket::Current => &mut s.current,
                DueBucket::NoSchedule => &mut s.no_schedule,
            } += 1;
            if matches!(bucket, DueBucket::Overdue | DueBucket::Due30) || needs_first_calibration(a)
            {
                s.attention.push(a.clone());
            }
            s.items.push(DueItem {
                bucket,
                asset: a.clone(),
            });
        }
        // `None` sorts first, so never-calibrated items lead the list.
        s.attention
            .sort_by_key(|a| (a.next_cal_date, a.asset_tag.clone()));
        s.items
            .sort_by_key(|i| (i.asset.next_cal_date, i.asset.asset_tag.clone()));
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
        end_of_month: false,
    };

    #[test]
    fn month_intervals_clamp_to_month_end() {
        let one_month = CalInterval {
            value: 1,
            unit: IntervalUnit::Months,
            end_of_month: false,
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
            end_of_month: false,
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
        let r = derive_dates(None, None, &records, Some(MONTHS_12));
        assert_eq!(r.first_cal_date, Some(d(2024, 6, 1)));
        assert_eq!(r.last_cal_date, Some(d(2026, 6, 1)));
        assert_eq!(r.next_cal_date, Some(d(2027, 6, 1)));
    }

    #[test]
    fn first_date_counts_a_manually_entered_last_date() {
        // Item brought over with a last calibration of 2026-03-15, then its first
        // calibration recorded in PolarisCal: March is still the earliest known one.
        let r = derive_dates(
            None,
            Some(d(2026, 3, 15)),
            &[d(2026, 11, 18)],
            Some(MONTHS_12),
        );
        assert_eq!(r.first_cal_date, Some(d(2026, 3, 15)));
        assert_eq!(r.last_cal_date, Some(d(2026, 11, 18)));
        assert_eq!(r.next_cal_date, Some(d(2027, 11, 18)));
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

    #[test]
    fn end_of_month_moves_the_due_date_to_the_months_last_day() {
        let eom = |value, unit| CalInterval {
            value,
            unit,
            end_of_month: true,
        };
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(
            add_interval(d(2026, 11, 20), eom(12, IntervalUnit::Months)),
            Some(d(2027, 11, 30))
        );
        assert_eq!(
            add_interval(d(2026, 11, 20), MONTHS_12),
            Some(d(2027, 11, 20))
        );
        // February, leap and not
        assert_eq!(
            add_interval(d(2027, 2, 3), eom(12, IntervalUnit::Months)),
            Some(d(2028, 2, 29))
        );
        assert_eq!(
            add_interval(d(2026, 1, 31), eom(1, IntervalUnit::Months)),
            Some(d(2026, 2, 28))
        );
        // December rolls into the next year correctly
        assert_eq!(
            add_interval(d(2026, 12, 5), eom(12, IntervalUnit::Months)),
            Some(d(2027, 12, 31))
        );
        // days intervals land in a month too
        assert_eq!(
            add_interval(d(2026, 11, 20), eom(30, IntervalUnit::Days)),
            Some(d(2026, 12, 31))
        );
    }

    #[test]
    fn extensions_move_the_next_date_until_the_next_calibration() {
        let last = Some(d(2025, 11, 1));
        let next = Some(d(2026, 11, 1));
        // Granted after the last calibration: next = date + days.
        assert_eq!(
            apply_extension(last, next, Some((d(2026, 11, 10), 30))),
            (Some(d(2026, 12, 10)), true)
        );
        // Older than the last calibration: a calibration since then ended it.
        assert_eq!(
            apply_extension(last, next, Some((d(2025, 10, 1), 30))),
            (next, false)
        );
        assert_eq!(apply_extension(last, next, None), (next, false));
        // Ending before the regular due date: no effect.
        assert_eq!(
            apply_extension(last, next, Some((d(2026, 6, 1), 30))),
            (next, false)
        );
        // A calibration the same day as the extension ends it.
        assert_eq!(
            apply_extension(Some(d(2026, 11, 10)), next, Some((d(2026, 11, 10), 30))),
            (next, false)
        );
        // No calibration yet: the extension applies.
        assert_eq!(
            apply_extension(None, None, Some((d(2026, 1, 1), 1))),
            (Some(d(2026, 1, 2)), true)
        );
    }

    fn asset(tag: &str, next: Option<NaiveDate>, interval: Option<CalInterval>) -> Asset {
        let today = d(2026, 11, 20);
        Asset {
            id: 0,
            asset_tag: tag.into(),
            kind: crate::model::AssetKind::Instrument,
            parent_id: None,
            parent_tag: None,
            description: String::new(),
            manufacturer_id: None,
            manufacturer_name: None,
            model_number: None,
            serial_number: None,
            location_id: None,
            location_name: None,
            department_id: None,
            department_name: None,
            owner_id: None,
            owner_name: None,
            status_id: 1,
            status_name: "Active".into(),
            status_role: None,
            previous_status_id: None,
            owner_since: None,
            cal_aggregate: false,
            warranty_end_date: None,
            cal_interval: interval,
            in_service_date: None,
            first_cal_date: None,
            last_cal_date: next.map(|n| n - Days::new(365)),
            last_cal_date_entered: None,
            next_cal_date: next,
            calibration_extended: false,
            due_status: due_status(next, true, today),
            child_count: 0,
            notes: None,
            is_archived: false,
            row_version: 1,
            created_at: Default::default(),
            updated_at: Default::default(),
        }
    }

    #[test]
    fn summary_buckets_and_attention() {
        let today = d(2026, 11, 20);
        let assets = [
            asset("OVER", Some(d(2026, 11, 1)), Some(MONTHS_12)),
            asset("SOON", Some(d(2026, 12, 1)), Some(MONTHS_12)),
            asset("NEXT", Some(d(2027, 1, 15)), Some(MONTHS_12)),
            asset("LATER", Some(d(2027, 6, 1)), Some(MONTHS_12)),
            asset("NEVER", None, Some(MONTHS_12)),
            asset("NOINT", None, None),
        ];
        let s = DueSummary::from_assets(&assets, today);
        assert_eq!(
            (s.overdue, s.due_30, s.due_31_90, s.current, s.no_schedule),
            (1, 1, 1, 1, 2)
        );
        // Never calibrated (with an interval) leads; no interval isn't urgent.
        let tags: Vec<_> = s.attention.iter().map(|a| a.asset_tag.as_str()).collect();
        assert_eq!(tags, ["NEVER", "OVER", "SOON"]);
        let buckets: Vec<_> = s
            .items
            .iter()
            .map(|i| (i.asset.asset_tag.as_str(), i.bucket))
            .collect();
        assert_eq!(
            buckets,
            [
                ("NEVER", DueBucket::NoSchedule),
                ("NOINT", DueBucket::NoSchedule),
                ("OVER", DueBucket::Overdue),
                ("SOON", DueBucket::Due30),
                ("NEXT", DueBucket::Due31To90),
                ("LATER", DueBucket::Current),
            ]
        );
    }

    #[test]
    fn extended_items_are_extended_until_the_extension_runs_out() {
        let today = d(2026, 11, 20);
        let ext = |next| due_status_with_extension(Some(next), true, true, today);
        assert_eq!(ext(d(2026, 12, 1)), DueStatus::Extended);
        assert_eq!(ext(today), DueStatus::Extended);
        assert_eq!(ext(d(2026, 11, 19)), DueStatus::Overdue);
        assert_eq!(
            due_status_with_extension(Some(d(2026, 12, 1)), false, true, today),
            DueStatus::NotTracked
        );
    }
}
