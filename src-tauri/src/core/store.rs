use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::event::{Event, SessionId, Timestamp};
use super::session::{Session, SessionState, ViewPrefs};

/// A stretch of wall-clock during which the aggregate state held steady.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub state: SessionState,
    pub from: Timestamp,
    pub to: Timestamp,
}

/// Aggregate state across all sessions over `[from, to]`, for the activity lane.
///
/// Deliberately takes no [`ViewPrefs`]: the lane is a record of the day, so dismissed,
/// dead and hidden sessions all still count. Visibility filters the list, not history.
pub fn timeline(events: &[Event], from: Timestamp, to: Timestamp) -> Vec<Segment> {
    let mut states: HashMap<&SessionId, SessionState> = HashMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut cursor = from;

    for event in events.iter().filter(|event| event.at() <= to) {
        if event.at() > cursor {
            extend(&mut segments, aggregate(&states), cursor, event.at());
            cursor = event.at();
        }
        states.insert(event.id(), state_after(event));
    }

    extend(&mut segments, aggregate(&states), cursor, to);
    segments
}

/// Share of the window spent with at least one session waiting on you, in `0.0..=1.0`.
pub fn waiting_share(segments: &[Segment]) -> f64 {
    let span = |segment: &Segment| segment.to - segment.from;
    let total: Timestamp = segments.iter().map(span).sum();
    if total == 0 {
        return 0.0;
    }
    let waiting: Timestamp = segments
        .iter()
        .filter(|segment| segment.state == SessionState::WaitingOnYou)
        .map(span)
        .sum();
    waiting as f64 / total as f64
}

fn state_after(event: &Event) -> SessionState {
    match event {
        Event::Notification { .. } => SessionState::WaitingOnYou,
        Event::Stop { .. } => SessionState::Idle,
        Event::SessionStart { .. } | Event::UserPromptSubmit { .. } => SessionState::Working,
    }
}

/// Waiting outranks working, which outranks idle: the lane shows the most urgent
/// thing true at that moment.
fn aggregate(states: &HashMap<&SessionId, SessionState>) -> SessionState {
    if states.values().any(|s| *s == SessionState::WaitingOnYou) {
        SessionState::WaitingOnYou
    } else if states.values().any(|s| *s == SessionState::Working) {
        SessionState::Working
    } else {
        SessionState::Idle
    }
}

fn extend(segments: &mut Vec<Segment>, state: SessionState, from: Timestamp, to: Timestamp) {
    if from >= to {
        return;
    }
    if let Some(last) = segments.last_mut() {
        if last.state == state {
            last.to = to;
            return;
        }
    }
    segments.push(Segment { state, from, to });
}

/// Sort tier: waiting on you, then user-pinned, then everything else.
fn tier(session: &Session, prefs: &ViewPrefs) -> u8 {
    if session.state == SessionState::WaitingOnYou {
        0
    } else if prefs.pinned.contains(&session.id) {
        1
    } else {
        2
    }
}

fn transition(sessions: &mut [Session], id: &SessionId, state: SessionState, at: Timestamp) {
    if let Some(session) = sessions.iter_mut().find(|s| &s.id == id) {
        session.state = state;
        session.since = at;
        session.last_active = at;
    }
}

/// Replay the event log into the current set of sessions.
///
/// Pure: no clock, no filesystem. `now` is supplied by the caller.
pub fn fold(events: &[Event], _now: Timestamp, prefs: &ViewPrefs) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();

    for event in events {
        match event {
            Event::SessionStart { id, cwd, pid, at } => sessions.push(Session {
                id: id.clone(),
                cwd: cwd.clone(),
                pid: Some(*pid),
                state: SessionState::Working,
                since: *at,
                last_active: *at,
            }),
            Event::Notification { id, at } => {
                transition(&mut sessions, id, SessionState::WaitingOnYou, *at)
            }
            Event::Stop { id, at } => transition(&mut sessions, id, SessionState::Idle, *at),
            Event::UserPromptSubmit { id, at } => {
                transition(&mut sessions, id, SessionState::Working, *at)
            }
        }
    }

    sessions.retain(|session| match prefs.dismissed.get(&session.id) {
        Some(dismissed_at) => session.last_active > *dismissed_at,
        None => true,
    });

    sessions.sort_by(|a, b| {
        tier(a, prefs)
            .cmp(&tier(b, prefs))
            .then(b.last_active.cmp(&a.last_active))
    });

    sessions
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    fn start(id: &str, at: Timestamp) -> Event {
        Event::SessionStart {
            id: id.into(),
            cwd: format!("/{id}"),
            pid: 10,
            at,
        }
    }

    fn fold_default(events: &[Event], now: Timestamp) -> Vec<Session> {
        fold(events, now, &ViewPrefs::default())
    }

    #[test]
    fn session_start_then_notification_is_waiting() {
        let events = vec![
            start("s1", 100),
            Event::Notification {
                id: "s1".into(),
                at: 200,
            },
        ];

        let sessions = fold_default(&events, 250);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
        assert_eq!(sessions[0].since, 200);
    }

    #[test]
    fn stop_makes_session_idle() {
        let events = vec![
            start("s1", 100),
            Event::Stop {
                id: "s1".into(),
                at: 300,
            },
        ];

        let sessions = fold_default(&events, 400);

        assert_eq!(sessions[0].state, SessionState::Idle);
        assert_eq!(sessions[0].since, 300);
    }

    #[test]
    fn user_prompt_after_waiting_resumes_working() {
        let events = vec![
            start("s1", 100),
            Event::Notification {
                id: "s1".into(),
                at: 200,
            },
            Event::UserPromptSubmit {
                id: "s1".into(),
                at: 250,
            },
        ];

        let sessions = fold_default(&events, 300);

        assert_eq!(sessions[0].state, SessionState::Working);
        assert_eq!(sessions[0].since, 250);
    }

    #[test]
    fn waiting_sorts_first_then_pinned_then_recency() {
        let events = vec![
            start("s1", 100),
            Event::Stop {
                id: "s1".into(),
                at: 500,
            },
            start("s2", 110),
            Event::Notification {
                id: "s2".into(),
                at: 200,
            },
            start("s3", 120),
            Event::Stop {
                id: "s3".into(),
                at: 300,
            },
        ];
        let prefs = ViewPrefs {
            pinned: HashSet::from(["s3".to_string()]),
            ..Default::default()
        };

        let sessions = fold(&events, 600, &prefs);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();

        assert_eq!(ids, vec!["s2", "s3", "s1"]);
    }

    #[test]
    fn dismissed_session_is_hidden() {
        let events = vec![
            start("s1", 100),
            Event::Stop {
                id: "s1".into(),
                at: 300,
            },
        ];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), 400)]),
            ..Default::default()
        };

        assert!(fold(&events, 500, &prefs).is_empty());
    }

    #[test]
    fn dismissed_session_returns_after_a_newer_event() {
        let events = vec![
            start("s1", 100),
            Event::Stop {
                id: "s1".into(),
                at: 300,
            },
            Event::UserPromptSubmit {
                id: "s1".into(),
                at: 500,
            },
        ];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), 400)]),
            ..Default::default()
        };

        assert_eq!(fold(&events, 600, &prefs).len(), 1);
    }

    /// s1 works from 0; s2 starts at 100 and blocks at 300. Waiting outranks working,
    /// so the window reads Working then WaitingOnYou.
    fn two_session_window() -> Vec<Event> {
        vec![
            start("s1", 0),
            start("s2", 100),
            Event::Notification {
                id: "s2".into(),
                at: 300,
            },
        ]
    }

    #[test]
    fn timeline_collapses_to_aggregate_segments() {
        let segments = timeline(&two_session_window(), 0, 600);

        assert_eq!(segments.first().unwrap().state, SessionState::Working);
        assert_eq!(segments.last().unwrap().state, SessionState::WaitingOnYou);
        assert_eq!(segments.last().unwrap().to, 600);
    }

    #[test]
    fn timeline_segments_are_contiguous_and_cover_the_window() {
        let segments = timeline(&two_session_window(), 0, 600);

        assert_eq!(segments.first().unwrap().from, 0);
        assert_eq!(segments.last().unwrap().to, 600);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
    }

    #[test]
    fn empty_log_is_one_idle_segment() {
        let segments = timeline(&[], 0, 600);

        assert_eq!(
            segments,
            vec![Segment {
                state: SessionState::Idle,
                from: 0,
                to: 600
            }]
        );
    }

    #[test]
    fn waiting_share_is_the_waiting_fraction_of_the_window() {
        let segments = timeline(&two_session_window(), 0, 600);

        assert!((waiting_share(&segments) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn same_cwd_different_ids_are_distinct_sessions() {
        let events = vec![
            start("s1", 100),
            Event::SessionStart {
                id: "s2".into(),
                cwd: "/s1".into(),
                pid: 11,
                at: 110,
            },
        ];

        let sessions = fold_default(&events, 200);

        assert_eq!(sessions.len(), 2);
    }
}
