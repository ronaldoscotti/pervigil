//! The one filter in the UI, and the window it resolves to. The caller supplies
//! `now`, so nothing here reads a clock.

use chrono::{DateTime, Duration, Local, TimeZone};
use serde::Deserialize;

use super::event::Timestamp;

/// The one filter in the UI. It scopes the lane, the cost readout, and how far back
/// transcripts are read — the panel shows the window you picked, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Span {
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "week")]
    Week,
}

/// `[from, now]` in epoch seconds. Today is a local calendar boundary; the week is
/// trailing, so its last day never falls off a reset.
pub fn bounds(span: Span, now: DateTime<Local>) -> (Timestamp, Timestamp) {
    let from = match span {
        Span::FourHours => now - Duration::hours(4),
        Span::Today => start_of_day(now),
        Span::Week => now - Duration::days(7),
    };

    (
        from.timestamp().max(0) as Timestamp,
        now.timestamp().max(0) as Timestamp,
    )
}

/// Local midnight — or, on a day whose clocks skip it, the first instant that exists.
pub fn start_of_day(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    (0..24)
        .filter_map(|hour| date.and_hms_opt(hour, 0, 0))
        .find_map(|at| now.timezone().from_local_datetime(&at).earliest())
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use chrono::Timelike;

    use super::*;

    #[test]
    fn four_hours_ends_now_and_starts_four_hours_back() {
        let now = Local::now();

        let (from, to) = bounds(Span::FourHours, now);

        assert_eq!(to, now.timestamp() as Timestamp);
        assert_eq!(to - from, 4 * 60 * 60);
    }

    #[test]
    fn today_starts_at_a_local_midnight_not_twenty_four_hours_ago() {
        let now = Local::now();

        let (from, _) = bounds(Span::Today, now);

        let start = Local.timestamp_opt(from as i64, 0).unwrap();
        assert_eq!(start.date_naive(), now.date_naive());
        assert_eq!((start.hour(), start.minute(), start.second()), (0, 0, 0));
    }

    #[test]
    fn the_week_is_a_trailing_seven_days_whatever_the_weekday() {
        let now = Local::now();

        let (from, to) = bounds(Span::Week, now);

        assert_eq!(to - from, Duration::days(7).num_seconds() as Timestamp);
    }
}
