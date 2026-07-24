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

/// How long a `Notification` keeps painting the lane before it decays to idle. A
/// notification means Claude is blocked on you *now*; a session that fires one and
/// then goes silent (killed terminal, no `Stop` ever) is stale, not a day-long wait.
const WAITING_TTL_SECS: Timestamp = 30 * 60;

/// Takes no [`ViewPrefs`] on purpose: the lane is a record of the day, so dismissed
/// and hidden sessions still count. A `WaitingOnYou` state decays to idle after
/// [`WAITING_TTL_SECS`] of silence, so a session that was notified and then died —
/// no `Stop` ever — can't paint the whole window.
pub fn timeline(events: &[Event], from: Timestamp, to: Timestamp) -> Vec<Segment> {
    let mut ticks: Vec<Tick> = Vec::new();
    for event in events.iter().filter(|event| event.at() <= to) {
        ticks.push(Tick::Event(event));
        if let Event::Notification { id, at } = event {
            let expiry = at + WAITING_TTL_SECS;
            if expiry <= to {
                ticks.push(Tick::Expire(id, expiry));
            }
        }
    }
    // Stable by time; a real event beats an expiry at the same instant.
    ticks.sort_by_key(|tick| (tick.at(), tick.is_expire()));

    let mut states: HashMap<&SessionId, (SessionState, Timestamp)> = HashMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut cursor = from;

    for tick in &ticks {
        if tick.at() > cursor {
            extend(&mut segments, aggregate(&states), cursor, tick.at());
            cursor = tick.at();
        }
        match tick {
            Tick::Event(event) => {
                let expiry = match event {
                    Event::Notification { at, .. } => at + WAITING_TTL_SECS,
                    _ => 0,
                };
                states.insert(event.id(), (state_after(event), expiry));
            }
            // Ignore an expiry the session has already left or renotified past.
            Tick::Expire(id, expiry) => {
                if states.get(id) == Some(&(SessionState::WaitingOnYou, *expiry)) {
                    states.insert(id, (SessionState::Idle, 0));
                }
            }
        }
    }

    extend(&mut segments, aggregate(&states), cursor, to);
    segments
}

enum Tick<'a> {
    Event(&'a Event),
    Expire(&'a SessionId, Timestamp),
}

impl Tick<'_> {
    fn at(&self) -> Timestamp {
        match self {
            Tick::Event(event) => event.at(),
            Tick::Expire(_, at) => *at,
        }
    }

    fn is_expire(&self) -> bool {
        matches!(self, Tick::Expire(..))
    }
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

fn aggregate(states: &HashMap<&SessionId, (SessionState, Timestamp)>) -> SessionState {
    let any = |target| states.values().any(|(state, _)| *state == target);
    if any(SessionState::WaitingOnYou) {
        SessionState::WaitingOnYou
    } else if any(SessionState::Working) {
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
                // A hook session born from a Notification has no cwd; the transcript does.
                if hook.cwd.is_empty() {
                    hook.cwd = transcript.cwd;
                }
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

/// Sessions that *entered* `WaitingOnYou` since `previous` — the only honest trigger
/// for a notification (spec item 8). A session already waiting last tick is not
/// returned, so a still-blocked session never re-notifies; one unseen in `previous`
/// but waiting now is (a fresh block), which the caller silences only on the very
/// first observation.
pub fn newly_waiting<'a>(
    previous: &HashMap<SessionId, SessionState>,
    current: &'a [Session],
) -> Vec<&'a Session> {
    current
        .iter()
        .filter(|session| session.state == SessionState::WaitingOnYou)
        .filter(|session| previous.get(&session.id) != Some(&SessionState::WaitingOnYou))
        .collect()
}

/// Every current session's state, to carry into the next tick's [`newly_waiting`].
pub fn states(sessions: &[Session]) -> HashMap<SessionId, SessionState> {
    sessions
        .iter()
        .map(|session| (session.id.clone(), session.state))
        .collect()
}

fn tier(session: &Session, prefs: &ViewPrefs) -> u8 {
    match session.state {
        SessionState::WaitingOnYou => 0,
        SessionState::YourTurn => 1,
        _ if prefs.pinned.contains(&session.id) => 2,
        _ => 3,
    }
}

/// Add a bare session for `id` if none exists yet — cwd/pid/state get set by the
/// event that follows.
fn ensure(sessions: &mut Vec<Session>, id: &SessionId, at: Timestamp) {
    if !sessions.iter().any(|s| &s.id == id) {
        sessions.push(Session {
            id: id.clone(),
            cwd: String::new(),
            pid: None,
            state: SessionState::Working,
            since: at,
            last_active: at,
            title: None,
            git_branch: None,
            terminal: None,
        });
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
        // Any event materialises its session. `SessionStart` only fires for *new*
        // `claude` runs, so a session already running when hooks are installed never
        // sees one — but its Notification/Stop/UserPromptSubmit still arrive, and
        // dropping them would freeze it at the transcript's idle. cwd/pid/terminal
        // stay empty until a SessionStart (or the transcript, via `merge`) fills them.
        ensure(&mut sessions, event.id(), event.at());

        match event {
            Event::SessionStart {
                id,
                cwd,
                pid,
                at,
                term,
            } => {
                if let Some(session) = sessions.iter_mut().find(|s| &s.id == id) {
                    session.cwd = cwd.clone();
                    session.pid = *pid;
                    session.terminal = term.clone();
                    session.state = SessionState::Working;
                    session.since = *at;
                    session.last_active = *at;
                }
            }
            Event::Notification { id, at } => {
                transition(&mut sessions, id, SessionState::WaitingOnYou, *at)
            }
            Event::Stop { id, at } => transition(&mut sessions, id, SessionState::YourTurn, *at),
            Event::UserPromptSubmit { id, at } => {
                transition(&mut sessions, id, SessionState::Working, *at)
            }
        }
    }

    drop_dismissed(&mut sessions, prefs);
    sort(&mut sessions, prefs);
    sessions
}

/// Hide dismissed sessions until they act again. Applied after [`merge`] too, so it
/// also covers transcript-only sessions — which never pass through [`fold`] and would
/// otherwise ignore a dismiss.
pub fn drop_dismissed(sessions: &mut Vec<Session>, prefs: &ViewPrefs) {
    sessions.retain(|session| match prefs.dismissed.get(&session.id) {
        Some(dismissed_at) => session.last_active > *dismissed_at,
        None => true,
    });
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
    fn stop_means_your_turn_a_live_session_waiting_at_its_prompt() {
        let events = vec![
            start("s1", 100),
            Event::Stop {
                id: "s1".into(),
                at: 300,
            },
        ];

        let sessions = fold_default(&events, 400);

        assert_eq!(sessions[0].state, SessionState::YourTurn);
        assert_eq!(sessions[0].since, 300);
    }

    #[test]
    fn your_turn_sorts_above_working_but_below_waiting() {
        let events = vec![
            start("waiting", 100),
            Event::Notification {
                id: "waiting".into(),
                at: 500,
            },
            start("your-turn", 110),
            Event::Stop {
                id: "your-turn".into(),
                at: 400,
            },
            start("working", 120),
        ];

        let sessions = fold_default(&events, 600);
        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();

        assert_eq!(ids, vec!["waiting", "your-turn", "working"]);
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
        // Among plain live (working) sessions, a pin beats recency; waiting still tops
        // all. Your-turn's own tier is covered separately.
        let events = vec![
            start("s1", 100),
            start("s2", 110),
            Event::Notification {
                id: "s2".into(),
                at: 200,
            },
            start("s3", 120),
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

    #[test]
    fn a_stale_notification_decays_instead_of_painting_the_whole_lane() {
        // The reported bug: a session fires a Notification then goes silent (killed
        // terminal, no Stop ever). It must not fill the window with amber.
        let events = vec![Event::Notification {
            id: "dead".into(),
            at: 0,
        }];

        let segments = timeline(&events, 0, 10 * WAITING_TTL_SECS);

        let share = waiting_share(&segments);
        assert!(share < 0.2, "one stale notification painted {share} of the lane");
    }

    #[test]
    fn a_notification_already_expired_before_the_window_paints_nothing() {
        // The worst offender: notified hours before the window, never seen again. By
        // `from` the wait has long decayed, so the window is all idle.
        let events = vec![Event::Notification {
            id: "dead".into(),
            at: 0,
        }];
        let from = 5 * WAITING_TTL_SECS;

        let segments = timeline(&events, from, from + WAITING_TTL_SECS);

        assert_eq!(waiting_share(&segments), 0.0);
    }

    #[test]
    fn a_recent_notification_still_counts_as_waiting_until_it_expires() {
        // Decay must not erase a genuine, current block.
        let events = vec![Event::Notification {
            id: "s1".into(),
            at: 0,
        }];

        let segments = timeline(&events, 0, WAITING_TTL_SECS);

        assert_eq!(waiting_share(&segments), 1.0);
    }

    #[test]
    fn a_second_notification_restarts_the_waiting_clock() {
        // The first notification's expiry must not cut the wait short while a later
        // one still holds.
        let events = vec![
            Event::Notification {
                id: "s1".into(),
                at: 0,
            },
            Event::Notification {
                id: "s1".into(),
                at: WAITING_TTL_SECS - 10,
            },
        ];

        let segments = timeline(&events, 0, 2 * WAITING_TTL_SECS - 20);

        assert_eq!(waiting_share(&segments), 1.0);
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
    fn an_event_without_a_prior_session_start_still_creates_the_session() {
        // Hooks installed mid-session: SessionStart never fired for an already-running
        // session, but its Notification/Stop/UserPromptSubmit still do. Those must not
        // be dropped, or the session stays frozen at the transcript's idle.
        let events = vec![
            Event::UserPromptSubmit {
                id: "s1".into(),
                at: 100,
            },
            Event::Notification {
                id: "s1".into(),
                at: 200,
            },
        ];

        let sessions = fold_default(&events, 300);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "s1");
        assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
        assert_eq!(sessions[0].since, 200);
        assert_eq!(sessions[0].pid, None, "no SessionStart, so no pid");
    }

    #[test]
    fn merge_backfills_cwd_from_the_transcript_for_a_hookmade_session() {
        // A session born from a Notification has no cwd of its own; the transcript has it.
        let hooks = vec![Session {
            cwd: String::new(),
            ..session("s1", SessionState::WaitingOnYou, None, Some(10))
        }];
        let transcripts = vec![session("s1", SessionState::Idle, Some("Fix login"), None)];

        let merged = merge(hooks, transcripts);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].cwd, "/s1", "cwd comes from the transcript");
        assert_eq!(
            merged[0].state,
            SessionState::WaitingOnYou,
            "hook state still wins"
        );
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

    fn waiting(id: &str) -> Session {
        session(id, SessionState::WaitingOnYou, None, None)
    }

    #[test]
    fn dismiss_hides_a_transcript_only_session_too() {
        // Transcript sessions are added by merge, after fold's own dismiss pass, so
        // the filter has to run again on the merged list.
        let mut sessions = vec![session("t1", SessionState::Idle, Some("done"), None)];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("t1".to_string(), 200)]),
            ..Default::default()
        };

        drop_dismissed(&mut sessions, &prefs);

        assert!(
            sessions.is_empty(),
            "a dismissed transcript session must hide"
        );
    }

    #[test]
    fn a_session_entering_waiting_is_a_notification() {
        let previous = HashMap::from([("s1".to_string(), SessionState::Working)]);
        let current = vec![waiting("s1")];

        let fire = newly_waiting(&previous, &current);

        assert_eq!(fire.len(), 1);
        assert_eq!(fire[0].id, "s1");
    }

    #[test]
    fn a_session_already_waiting_does_not_re_notify() {
        let previous = HashMap::from([("s1".to_string(), SessionState::WaitingOnYou)]);
        let current = vec![waiting("s1")];

        assert!(newly_waiting(&previous, &current).is_empty());
    }

    #[test]
    fn a_session_leaving_waiting_is_not_a_notification() {
        let previous = HashMap::from([("s1".to_string(), SessionState::WaitingOnYou)]);
        let current = vec![session("s1", SessionState::Working, None, None)];

        assert!(newly_waiting(&previous, &current).is_empty());
    }

    #[test]
    fn a_freshly_appeared_waiting_session_notifies() {
        let current = vec![waiting("new")];

        assert_eq!(newly_waiting(&HashMap::new(), &current).len(), 1);
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
