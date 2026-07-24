use super::event::{Event, Timestamp};

pub const RETENTION_SECS: Timestamp = 30 * 24 * 60 * 60;

/// Drop events older than the retention window. Run on launch — the log is
/// append-only, so nothing else bounds its growth.
pub fn prune(events: Vec<Event>, now: Timestamp) -> Vec<Event> {
    events
        .into_iter()
        .filter(|event| now.saturating_sub(event.at()) <= RETENTION_SECS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(at: Timestamp) -> Event {
        Event::Stop {
            id: "s1".into(),
            at,
        }
    }

    #[test]
    fn drops_events_past_the_retention_window() {
        let now = RETENTION_SECS + 1_000;
        let events = vec![stop(500), stop(now - 10)];

        let kept = prune(events, now);

        assert_eq!(kept, vec![stop(now - 10)]);
    }

    #[test]
    fn an_event_exactly_at_the_boundary_is_kept() {
        let now = RETENTION_SECS + 1_000;

        let kept = prune(vec![stop(now - RETENTION_SECS)], now);

        assert_eq!(kept.len(), 1, "the boundary itself is inside the window");
    }

    #[test]
    fn an_event_one_second_past_the_boundary_is_dropped() {
        let now = RETENTION_SECS + 1_000;

        let kept = prune(vec![stop(now - RETENTION_SECS - 1)], now);

        assert!(kept.is_empty());
    }
}
