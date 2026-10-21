//! Repeating events: which occurrences of an event fall inside a window.

use aether_sdk::dates::{self, Duration, NaiveDate, NaiveDateTime};

pub const REPEATS: [&str; 5] = ["none", "daily", "weekly", "monthly", "yearly"];
const MAX_STEPS: i64 = 4000;

/// One occurrence of an event: when it starts and ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

fn nth_start(first: NaiveDateTime, repeat: &str, n: i64) -> Option<NaiveDateTime> {
    match repeat {
        "daily" => Some(first + Duration::days(n)),
        "weekly" => Some(first + Duration::weeks(n)),
        // The day of the month comes from the first occurrence each time, so 31 Jan, 28 Feb, 31 Mar does not drift.
        "monthly" => Some(dates::add_months(first.date(), u32::try_from(n).ok()?).and_time(first.time())),
        "yearly" => Some(dates::add_months(first.date(), u32::try_from(n * 12).ok()?).and_time(first.time())),
        _ => None,
    }
}

/// The occurrences of an event that overlap `[from, to]`, leaving out days in `skipped` and anything after `until`.
pub fn occurrences(first: NaiveDateTime, last: NaiveDateTime, repeat: &str, until: Option<NaiveDate>, skipped: &[NaiveDate], from: NaiveDateTime, to: NaiveDateTime) -> Vec<Occurrence> {
    let length = if last > first { last - first } else { Duration::zero() };
    let overlaps = |start: NaiveDateTime| start <= to && start + length >= from;
    if repeat == "none" || !REPEATS.contains(&repeat) {
        return if overlaps(first) { vec![Occurrence { start: first, end: first + length }] } else { Vec::new() };
    }
    // Daily and weekly events can jump straight to the window.
    let step_days = match repeat {
        "daily" => 1,
        "weekly" => 7,
        _ => 0,
    };
    let mut n = if step_days > 0 { ((from - length - first).num_days() / step_days - 1).max(0) } else { 0 };
    let mut out = Vec::new();
    let mut steps = 0;
    while steps < MAX_STEPS {
        steps += 1;
        let Some(start) = nth_start(first, repeat, n) else { break };
        if start > to || until.is_some_and(|u| start.date() > u) {
            break;
        }
        if overlaps(start) && !skipped.contains(&start.date()) {
            out.push(Occurrence { start, end: start + length });
        }
        n += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        dates::parse_datetime(text).unwrap()
    }

    #[test]
    fn a_single_event_is_in_the_window_or_not() {
        let (s, e) = (at("2026-10-20T09:00:00Z"), at("2026-10-20T10:00:00Z"));
        assert_eq!(occurrences(s, e, "none", None, &[], at("2026-10-19T00:00:00Z"), at("2026-10-21T00:00:00Z")).len(), 1);
        assert!(occurrences(s, e, "none", None, &[], at("2026-10-21T00:00:00Z"), at("2026-10-22T00:00:00Z")).is_empty());
    }

    #[test]
    fn a_weekly_event_shows_each_week_until_it_ends() {
        let (s, e) = (at("2026-10-05T09:00:00Z"), at("2026-10-05T10:00:00Z"));
        let all = occurrences(s, e, "weekly", None, &[], at("2026-10-01T00:00:00Z"), at("2026-11-01T00:00:00Z"));
        assert_eq!(all.len(), 4);
        assert_eq!(all[3].start, at("2026-10-26T09:00:00Z"));
        let until = occurrences(s, e, "weekly", NaiveDate::from_ymd_opt(2026, 10, 14), &[], at("2026-10-01T00:00:00Z"), at("2026-11-01T00:00:00Z"));
        assert_eq!(until.len(), 2);
    }

    #[test]
    fn a_skipped_day_leaves_a_gap() {
        let (s, e) = (at("2026-10-05T09:00:00Z"), at("2026-10-05T10:00:00Z"));
        let skipped = [NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()];
        let all = occurrences(s, e, "weekly", None, &skipped, at("2026-10-01T00:00:00Z"), at("2026-10-31T00:00:00Z"));
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|o| o.start.date() != skipped[0]));
    }

    #[test]
    fn a_window_far_after_the_start_is_reached_without_walking_every_day() {
        let (s, e) = (at("2020-01-01T09:00:00Z"), at("2020-01-01T10:00:00Z"));
        let all = occurrences(s, e, "daily", None, &[], at("2026-10-18T00:00:00Z"), at("2026-10-20T23:59:59Z"));
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn a_monthly_event_keeps_its_day_across_short_months() {
        let (s, e) = (at("2026-01-31T09:00:00Z"), at("2026-01-31T10:00:00Z"));
        let all = occurrences(s, e, "monthly", None, &[], at("2026-01-01T00:00:00Z"), at("2026-04-30T23:00:00Z"));
        let days: Vec<_> = all.iter().map(|o| o.start.date().to_string()).collect();
        assert_eq!(days, ["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]);
    }

    #[test]
    fn a_long_event_that_began_before_the_window_still_shows() {
        let (s, e) = (at("2026-10-01T00:00:00Z"), at("2026-10-10T00:00:00Z"));
        assert_eq!(occurrences(s, e, "none", None, &[], at("2026-10-05T00:00:00Z"), at("2026-10-06T00:00:00Z")).len(), 1);
    }
}
