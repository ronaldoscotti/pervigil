//! Property tests for the fold. Example tests pin the cases we thought of; these
//! pin the invariants, over sequences nobody chose.

use proptest::prelude::*;

use super::event::{Event, NotificationKind, Timestamp};
use super::session::ViewPrefs;
use super::store::{fold, timeline};

/// Four kinds over a small pool of ids and non-decreasing timestamps — the shape the
/// real log has, since it is append-only.
fn events() -> impl Strategy<Value = Vec<Event>> {
    proptest::collection::vec((0usize..4, 0usize..3, 0u32..3, 1 as Timestamp..400), 0..40).prop_map(
        |rows| {
            let mut at: Timestamp = 0;
            rows.into_iter()
                .map(|(kind, id, pid, step)| {
                    at += step;
                    let id = format!("s{id}");
                    match kind {
                        0 => Event::SessionStart {
                            id,
                            cwd: "/p".into(),
                            pid: Some(pid),
                            at,
                            term: None,
                        },
                        1 => Event::Notification {
                            id,
                            at,
                            kind: Some(NotificationKind::Permission),
                        },
                        2 => Event::Stop { id, at },
                        _ => Event::UserPromptSubmit { id, at },
                    }
                })
                .collect()
        },
    )
}

proptest! {
    #[test]
    fn fold_is_deterministic(events in events(), now in 0 as Timestamp..2000) {
        let prefs = ViewPrefs::default();

        prop_assert_eq!(fold(&events, now, &prefs), fold(&events, now, &prefs));
    }

    /// The lane must tile its window exactly: no gap the UI would render as a hole,
    /// no overlap that would make the waiting share exceed 1.
    #[test]
    fn timeline_tiles_the_window_with_no_gap_and_no_overlap(
        events in events(),
        from in 0 as Timestamp..500,
        width in 1 as Timestamp..2000,
    ) {
        let to = from + width;

        let segments = timeline(&events, &[], from, to);

        prop_assert!(!segments.is_empty());
        prop_assert_eq!(segments.first().unwrap().from, from);
        prop_assert_eq!(segments.last().unwrap().to, to);
        for pair in segments.windows(2) {
            prop_assert_eq!(pair[0].to, pair[1].from, "gap or overlap between segments");
        }
        for segment in &segments {
            prop_assert!(segment.from < segment.to, "an empty segment is not a segment");
        }
    }

    /// Replaying more of the log must never move a session backwards. `fold` is a
    /// full replay, so "prefix then rest" is the same vector — the invariant with
    /// teeth is that a longer prefix only ever advances what it reports.
    #[test]
    fn a_longer_prefix_never_moves_a_session_backwards(
        events in events(),
        cut in 0usize..40,
    ) {
        let cut = cut.min(events.len());
        let now = events.last().map(|e| e.at()).unwrap_or(0) + 1;
        let prefs = ViewPrefs::default();

        let early = fold(&events[..cut], now, &prefs);
        let full = fold(&events, now, &prefs);

        for session in &early {
            if let Some(later) = full.iter().find(|s| s.id == session.id) {
                prop_assert!(
                    later.last_active >= session.last_active,
                    "{} went backwards: {} -> {}",
                    session.id,
                    session.last_active,
                    later.last_active
                );
            }
        }
    }

    /// Every session's clock stays inside the log that produced it.
    #[test]
    fn a_session_is_never_active_before_it_started(events in events()) {
        let now = events.last().map(|e| e.at()).unwrap_or(0) + 1;

        for session in fold(&events, now, &ViewPrefs::default()) {
            prop_assert!(session.since <= session.last_active);
            prop_assert!(session.last_active < now);
        }
    }
}
