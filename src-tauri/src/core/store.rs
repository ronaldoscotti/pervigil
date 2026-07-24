use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::event::{Event, SessionId, Timestamp};
use super::session::{Session, SessionState, ViewPrefs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub state: SessionState,
    pub from: Timestamp,
    pub to: Timestamp,
}

/// Takes no [`ViewPrefs`] on purpose: the lane is a record of the day, so dismissed,
/// dead and hidden sessions all still count.
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

/// Fraction of the window spent waiting on you, in `0.0..=1.0`.
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

/// Union both discovery sources by session id. Hooks win on state and pid;
/// transcripts add the title and any session hooks never saw. Does not sort.
pub fn merge(mut hooks: Vec<Session>, transcripts: Vec<Session>) -> Vec<Session> {
    for transcript in transcripts {
        match hooks.iter_mut().find(|hook| hook.id == transcript.id) {
            Some(hook) => {
                hook.title = hook.title.take().or(transcript.title);
                hook.git_branch = hook.git_branch.take().or(transcript.git_branch);
            }
            None => hooks.push(transcript),
        }
    }
    hooks
}

/// The list's whole order, in one place: waiting-on-you, then pinned, then recency.
/// Public because [`merge`] appends transcript-derived sessions after [`fold`] has
/// already sorted, and both sources must end up in the same order.
pub fn sort(sessions: &mut [Session], prefs: &ViewPrefs) {
    sessions.sort_by(|a, b| {
        tier(a, prefs)
            .cmp(&tier(b, prefs))
            .then(b.last_active.cmp(&a.last_active))
    });
}

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

/// Replay the event log into the current sessions. Pure: no clock, no filesystem.
pub fn fold(events: &[Event], _now: Timestamp, prefs: &ViewPrefs) -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();

    for event in events {
        match event {
            Event::SessionStart {
                id,
                cwd,
                pid,
                at,
                term,
            } => sessions.push(Session {
                id: id.clone(),
                cwd: cwd.clone(),
                pid: *pid,
                state: SessionState::Working,
                since: *at,
                last_active: *at,
                title: None,
                git_branch: None,
                terminal: term.clone(),
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

    sort(&mut sessions, prefs);
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
            pid: Some(10),
            at,
            term: None,
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

    fn session(id: &str, state: SessionState, title: Option<&str>, pid: Option<u32>) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/{id}"),
            pid,
            state,
            since: 100,
            last_active: 100,
            title: title.map(str::to_string),
            git_branch: None,
            terminal: None,
        }
    }

    #[test]
    fn a_transcript_only_session_survives_the_merge() {
        let transcripts = vec![session("t1", SessionState::Idle, Some("Fix login"), None)];

        let merged = merge(Vec::new(), transcripts);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].state, SessionState::Idle);
        assert_eq!(merged[0].pid, None);
        assert_eq!(merged[0].title.as_deref(), Some("Fix login"));
    }

    #[test]
    fn hook_state_wins_but_the_transcript_supplies_the_title() {
        let hooks = vec![session("s1", SessionState::WaitingOnYou, None, Some(10))];
        let transcripts = vec![session(
            "s1",
            SessionState::Idle,
            Some("Design panel"),
            None,
        )];

        let merged = merge(hooks, transcripts);

        assert_eq!(merged.len(), 1, "same id must not duplicate");
        assert_eq!(merged[0].state, SessionState::WaitingOnYou);
        assert_eq!(merged[0].pid, Some(10));
        assert_eq!(merged[0].title.as_deref(), Some("Design panel"));
    }

    #[test]
    fn fold_carries_the_terminal_hint_from_session_start() {
        let events = vec![Event::SessionStart {
            id: "s1".into(),
            cwd: "/p".into(),
            pid: Some(10),
            at: 100,
            term: Some(crate::core::terminal::Terminal {
                tmux_pane: Some("%3".into()),
                ..Default::default()
            }),
        }];

        let sessions = fold_default(&events, 200);

        assert_eq!(
            sessions[0]
                .terminal
                .as_ref()
                .and_then(|t| t.tmux_pane.as_deref()),
            Some("%3")
        );
    }

    #[test]
    fn same_cwd_different_ids_are_distinct_sessions() {
        let events = vec![
            start("s1", 100),
            Event::SessionStart {
                id: "s2".into(),
                cwd: "/s1".into(),
                pid: Some(11),
                at: 110,
                term: None,
            },
        ];

        let sessions = fold_default(&events, 200);

        assert_eq!(sessions.len(), 2);
    }
}
