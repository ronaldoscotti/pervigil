use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::event::{Event, NotificationKind, SessionId, SessionSource, Timestamp};
use super::session::{DismissMode, Session, SessionState, ViewPrefs};

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

/// How long a background agent's last record still proves it is running. Its records
/// land every second or two (p50 1s, p99 68s across 54k gaps here), but a slow tool
/// call goes quiet for minutes: 209 of 601 agent transcripts have a stretch over two
/// minutes and 63 over five, so a tighter bound would flap mid-run. Ten minutes covers
/// 95% of the worst stretches.
///
/// The common case never reaches it — when an agent finishes, the main loop wakes and
/// fires its own hook. This is the backstop for the case where it does not, so that
/// `Working` inferred from an agent cannot paint the rest of the day.
const AGENT_TTL_SECS: Timestamp = 10 * 60;

/// Takes no [`ViewPrefs`] on purpose: the lane is a record of the day, so dismissed
/// and hidden sessions still count. A `WaitingOnYou` state decays to idle after
/// [`WAITING_TTL_SECS`] of silence, so a session that was notified and then died —
/// no `Stop` ever — can't paint the whole window.
/// `activity` is `(session, when, where-from)` per transcript record — the evidence a
/// wait was answered, since approving a permission prompt fires no hook. What counts
/// as evidence depends on where the record came from: see [`moves_to_working`], which
/// the lane mirrors.
pub fn timeline(
    events: &[Event],
    activity: &[(SessionId, Timestamp, Origin)],
    from: Timestamp,
    to: Timestamp,
) -> Vec<Segment> {
    let mut ticks: Vec<Tick> = Vec::new();
    for event in events.iter().filter(|event| event.at() <= to) {
        ticks.push(Tick::Event(event));
        if let Event::Notification { id, at, kind } = event {
            let expiry = at + WAITING_TTL_SECS;
            if after_notification(*kind) == SessionState::WaitingOnYou && expiry <= to {
                ticks.push(Tick::Expire(id, expiry));
            }
        }
    }
    for (id, at, origin) in activity.iter().filter(|(_, at, _)| *at <= to) {
        ticks.push(Tick::Active(id, *at, *origin));
        if *origin == Origin::Agent && at + AGENT_TTL_SECS <= to {
            ticks.push(Tick::Expire(id, at + AGENT_TTL_SECS));
        }
    }
    // Activity is applied before the events of the same instant: the assistant record
    // that triggers a permission prompt shares its second, and must not cancel it.
    ticks.sort_by_key(|tick| (tick.at(), tick.rank()));

    let mut states: HashMap<&SessionId, Wait> = HashMap::new();
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
                states.insert(
                    event.id(),
                    Wait {
                        state: state_after(event),
                        expiry,
                    },
                );
            }
            Tick::Active(id, at, origin) => {
                if states
                    .get(id)
                    .is_some_and(|wait| wait.moves_to_working(*origin))
                {
                    // An agent's word is good for `AGENT_TTL_SECS`; answering a prompt
                    // yourself needs no expiry, like any other event-borne `Working`.
                    let expiry = match origin {
                        Origin::Main => 0,
                        Origin::Agent => at + AGENT_TTL_SECS,
                    };
                    states.insert(id, Wait::working_until(expiry));
                }
            }
            // Only a wait and an agent-inferred `Working` carry an expiry, and only the
            // latest one matches: an expiry the session has already left, renotified
            // past, or outlived by a newer agent record is ignored.
            Tick::Expire(id, expiry) => {
                if states.get(id).is_some_and(|wait| wait.expiry == *expiry) {
                    states.insert(id, Wait::idle());
                }
            }
        }
    }

    extend(&mut segments, aggregate(&states), cursor, to);
    segments
}

/// One session's state inside the lane, with when its wait would decay.
struct Wait {
    state: SessionState,
    expiry: Timestamp,
}

impl Wait {
    fn working_until(expiry: Timestamp) -> Self {
        Self {
            state: SessionState::Working,
            expiry,
        }
    }

    fn idle() -> Self {
        Self {
            state: SessionState::Idle,
            expiry: 0,
        }
    }

    /// The lane's half of [`moves_to_working`]. It paints three states and has no
    /// `YourTurn`, so a quiet main loop is `Idle` here.
    fn moves_to_working(&self, origin: Origin) -> bool {
        matches!(
            (origin, self.state),
            (Origin::Main, SessionState::WaitingOnYou) | (Origin::Agent, SessionState::Idle)
        )
    }
}

enum Tick<'a> {
    Event(&'a Event),
    Active(&'a SessionId, Timestamp, Origin),
    Expire(&'a SessionId, Timestamp),
}

impl Tick<'_> {
    fn at(&self) -> Timestamp {
        match self {
            Tick::Event(event) => event.at(),
            Tick::Active(_, at, _) | Tick::Expire(_, at) => *at,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Tick::Active(..) => 0,
            Tick::Event(_) => 1,
            Tick::Expire(..) => 2,
        }
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

/// Opening a project is not work. `SessionStart` fires on startup, `--resume` and
/// `/clear` with nothing running yet — only compaction happens mid-turn. A line with no
/// recorded source keeps the old reading, since 30 days of them are still in the log.
fn after_start(source: Option<SessionSource>) -> SessionState {
    match source {
        Some(SessionSource::Opened) => SessionState::Idle,
        _ => SessionState::Working,
    }
}

/// A notification's state in the lane: only a real block paints amber. The idle nudge
/// says the main loop went quiet, which the lane already has a colour for.
fn after_notification(kind: Option<NotificationKind>) -> SessionState {
    match kind {
        Some(NotificationKind::Idle) => SessionState::Idle,
        _ => SessionState::WaitingOnYou,
    }
}

fn state_after(event: &Event) -> SessionState {
    match event {
        Event::Notification { kind, .. } => after_notification(*kind),
        Event::Stop { .. } => SessionState::Idle,
        Event::SessionStart { source, .. } => after_start(*source),
        Event::UserPromptSubmit { .. } => SessionState::Working,
    }
}

fn aggregate(states: &HashMap<&SessionId, Wait>) -> SessionState {
    let any = |target| states.values().any(|wait| wait.state == target);
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

/// Union every discovery source by session id. Hooks win on state and pid;
/// transcripts add the title, the freshest recency, and any session hooks never saw.
/// Does not sort.
///
/// `agents` are the rows read from a session's background-agent files. They carry the
/// same session id and count for recency and cost, but they are kept apart because
/// they are not evidence about you: see [`moves_to_working`].
pub fn merge(
    mut hooks: Vec<Session>,
    transcripts: Vec<Session>,
    agents: Vec<Session>,
    now: Timestamp,
) -> Vec<Session> {
    for transcript in transcripts {
        absorb(&mut hooks, transcript, Origin::Main, now);
    }
    for agent in agents {
        absorb(&mut hooks, agent, Origin::Agent, now);
    }
    hooks
}

/// Which file a transcript record came from. A session's own transcript speaks for
/// you; a background agent's speaks only for itself. See [`moves_to_working`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Main,
    Agent,
}

fn absorb(hooks: &mut Vec<Session>, transcript: Session, origin: Origin, now: Timestamp) {
    match hooks.iter_mut().find(|hook| hook.id == transcript.id) {
        Some(hook) => {
            // Recency, unlike state, takes whichever source is fresher: hook events
            // are sparse, and a session can own several transcript files.
            hook.last_active = hook.last_active.max(transcript.last_active);
            if moves_to_working(hook, &transcript, origin, now) {
                hook.state = SessionState::Working;
                hook.since = transcript.last_active;
            }
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

/// What a transcript record written after the session's last event proves about it.
///
/// Approving a permission prompt fires no hook, so a record in the session's *own*
/// transcript is the only proof you answered. A background agent's records prove
/// something else entirely: it writes whether you answered or walked away, so it can
/// never clear a wait — but it is proof the session is not sitting quiet.
///
/// The two arms that are absent are the point. An agent cannot end a block, and the
/// session's own trailing writes cannot revive a turn that finished. Agent evidence
/// also goes stale, which is what [`AGENT_TTL_SECS`] bounds.
fn moves_to_working(hook: &Session, transcript: &Session, origin: Origin, now: Timestamp) -> bool {
    if transcript.last_active <= hook.since {
        return false;
    }
    match (origin, hook.state) {
        (Origin::Main, SessionState::WaitingOnYou) => true,
        // An agent that has not written for `AGENT_TTL_SECS` is no longer evidence of
        // anything, or a finished agent would leave the row green until the next event.
        (Origin::Agent, SessionState::YourTurn | SessionState::Idle) => {
            now.saturating_sub(transcript.last_active) <= AGENT_TTL_SECS
        }
        _ => false,
    }
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

fn blocked(sessions: &[Session], id: &SessionId) -> bool {
    sessions
        .iter()
        .any(|s| &s.id == id && s.state == SessionState::WaitingOnYou)
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
                source,
            } => {
                if let Some(session) = sessions.iter_mut().find(|s| &s.id == id) {
                    session.cwd = cwd.clone();
                    session.pid = *pid;
                    session.terminal = term.clone();
                    session.state = after_start(*source);
                    session.since = *at;
                    session.last_active = *at;
                }
            }
            // The row has a state for "your move" and the lane does not, so the nudge
            // reads as `YourTurn` here and as `Idle` there. Both say: not blocked.
            Event::Notification { id, at, kind } => {
                let state = match kind {
                    Some(NotificationKind::Idle) => SessionState::YourTurn,
                    _ => SessionState::WaitingOnYou,
                };
                // A nudge never downgrades a block that is still open. Claude Code does
                // not appear to raise one while a prompt is pending, but a prompt can
                // sit for hours, and only answering it or the TTL may end it.
                if !(state == SessionState::YourTurn && blocked(&sessions, id)) {
                    transition(&mut sessions, id, state, *at);
                }
            }
            Event::Stop { id, at } => transition(&mut sessions, id, SessionState::YourTurn, *at),
            Event::UserPromptSubmit { id, at } => {
                transition(&mut sessions, id, SessionState::Working, *at)
            }
        }
    }

    apply_dismissed(&mut sessions, prefs);
    sort(&mut sessions, prefs);
    sessions
}

/// Sessions the process has moved on from: `/resume` starts a new id inside the same
/// `claude`, and the one it left never emits again, so neither age nor liveness can
/// retire it.
///
/// Only a session whose *last* event is its own `SessionStart` can be retired — it
/// started and did nothing. A session that ever reached waiting, your-turn, or a
/// prompt is therefore untouchable, which matters because `/resume` can also switch
/// back and forth: two live sessions genuinely interleave under one pid.
pub fn superseded(events: &[Event]) -> std::collections::HashSet<SessionId> {
    let mut last: HashMap<&SessionId, (Timestamp, bool)> = HashMap::new();
    let mut process: HashMap<&SessionId, (u32, &str)> = HashMap::new();

    for event in events {
        let started = matches!(event, Event::SessionStart { .. });
        let seen = last.entry(event.id()).or_insert((event.at(), started));
        if event.at() >= seen.0 {
            *seen = (event.at(), started);
        }
        if let Event::SessionStart {
            id,
            pid: Some(pid),
            cwd,
            ..
        } = event
        {
            process.insert(id, (*pid, cwd.as_str()));
        }
    }

    let mut newest: HashMap<(u32, &str), Timestamp> = HashMap::new();
    for (id, (at, _)) in &last {
        if let Some(key) = process.get(id) {
            let seen = newest.entry(*key).or_insert(*at);
            *seen = (*seen).max(*at);
        }
    }

    last.iter()
        .filter(|(id, (at, started))| {
            *started
                && process
                    .get(*id)
                    .and_then(|key| newest.get(key))
                    .is_some_and(|moved_on| *moved_on > *at)
        })
        .map(|(id, _)| (*id).clone())
        .collect()
}

/// Scope the list to the span the panel is showing. Applied after [`merge`], so it
/// covers transcript-only sessions too.
pub fn retain_within(sessions: &mut Vec<Session>, from: Timestamp) {
    sessions.retain(|session| session.last_active >= from);
}

/// Resolve dismissed sessions per the chosen mode, until they act again: `Hide`
/// removes them; `Read` keeps them but demotes to idle — a "mark as read". Applied
/// after [`merge`] too, so it also covers transcript-only sessions — which never pass
/// through [`fold`] and would otherwise ignore a dismiss.
/// Compares `since`, not `last_active`: "acts again" means the session changed state,
/// not that bytes appeared. A background agent writes every few seconds, and against
/// `last_active` — which now takes the freshest transcript — that made a dismissed row
/// reappear on the next poll and stay undismissable for as long as the agent ran.
pub fn apply_dismissed(sessions: &mut Vec<Session>, prefs: &ViewPrefs) {
    let dismissed = |session: &Session| matches!(prefs.dismissed.get(&session.id), Some(at) if session.since <= *at);
    match prefs.dismiss_mode {
        DismissMode::Hide => sessions.retain(|session| !dismissed(session)),
        DismissMode::Read => {
            for session in sessions.iter_mut().filter(|s| dismissed(s)) {
                session.state = SessionState::Idle;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    fn start(id: &str, at: Timestamp) -> Event {
        Event::SessionStart {
            id: id.into(),
            cwd: format!("/{id}"),
            pid: Some(pid_for(id)),
            at,
            term: None,
            source: None,
        }
    }

    fn pid_for(id: &str) -> u32 {
        id.bytes().fold(7u32, |acc, byte| {
            acc.wrapping_mul(31).wrapping_add(u32::from(byte))
        })
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
                kind: None,
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
                kind: None,
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
                kind: None,
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
                kind: None,
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

    #[test]
    fn read_mode_demotes_a_dismissed_session_to_idle_but_keeps_it() {
        let events = vec![
            start("s1", 100),
            Event::Notification {
                id: "s1".into(),
                at: 300,
                kind: None,
            },
        ];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), 400)]),
            dismiss_mode: DismissMode::Read,
            ..Default::default()
        };

        let sessions = fold(&events, 500, &prefs);

        assert_eq!(sessions.len(), 1, "read mode keeps the session");
        assert_eq!(
            sessions[0].state,
            SessionState::Idle,
            "demoted, not waiting"
        );
    }

    #[test]
    fn read_mode_leaves_a_session_that_acted_after_dismiss_alone() {
        let events = vec![
            start("s1", 100),
            Event::Notification {
                id: "s1".into(),
                at: 500,
                kind: None,
            },
        ];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), 400)]),
            dismiss_mode: DismissMode::Read,
            ..Default::default()
        };

        let sessions = fold(&events, 600, &prefs);

        assert_eq!(
            sessions[0].state,
            SessionState::WaitingOnYou,
            "acted after read"
        );
    }

    fn two_session_window() -> Vec<Event> {
        vec![
            start("s1", 0),
            start("s2", 100),
            Event::Notification {
                id: "s2".into(),
                at: 300,
                kind: None,
            },
        ]
    }

    /// The assistant's tool_use record and the permission `Notification` it triggers
    /// land in the same epoch second, so activity must not cancel a wait it precedes.
    /// `merge` uses a strict `>` for the row; the lane has to agree.
    #[test]
    fn activity_in_the_same_second_does_not_cancel_the_wait_it_opened() {
        let events = vec![Event::Notification {
            id: "s1".into(),
            at: 100,
            kind: None,
        }];
        let activity = vec![("s1".to_string(), 100, Origin::Main)];

        let segments = timeline(&events, &activity, 100, 1_900);

        assert_eq!(
            waiting_share(&segments),
            1.0,
            "a real block must not read as zero waiting: {segments:?}"
        );
    }

    /// The lane is painted from events alone, so a permission prompt kept painting
    /// amber for as long as the turn ran — the row said Working while the lane said
    /// 86% waiting. Transcript activity ends the wait here too.
    #[test]
    fn transcript_activity_ends_a_wait_in_the_lane() {
        let events = vec![Event::Notification {
            id: "s1".into(),
            at: 0,
            kind: None,
        }];
        let activity = vec![("s1".to_string(), 100, Origin::Main)];

        let segments = timeline(&events, &activity, 0, 200);

        assert!(
            (waiting_share(&segments) - 0.5).abs() < 1e-9,
            "waiting until the transcript moved, working after: {segments:?}"
        );
    }

    #[test]
    fn activity_before_the_wait_does_not_end_it() {
        let events = vec![Event::Notification {
            id: "s1".into(),
            at: 100,
            kind: None,
        }];
        let activity = vec![("s1".to_string(), 50, Origin::Main)];

        let segments = timeline(&events, &activity, 0, 200);

        assert_eq!(waiting_share(&segments), 0.5, "the wait still stands");
    }

    #[test]
    fn activity_never_revives_a_session_that_stopped() {
        let events = vec![Event::Stop {
            id: "s1".into(),
            at: 0,
        }];
        let activity = vec![("s1".to_string(), 100, Origin::Main)];

        let segments = timeline(&events, &activity, 0, 200);

        assert_eq!(waiting_share(&segments), 0.0);
    }

    #[test]
    fn timeline_collapses_to_aggregate_segments() {
        let segments = timeline(&two_session_window(), &[], 0, 600);

        assert_eq!(segments.first().unwrap().state, SessionState::Working);
        assert_eq!(segments.last().unwrap().state, SessionState::WaitingOnYou);
        assert_eq!(segments.last().unwrap().to, 600);
    }

    #[test]
    fn timeline_segments_are_contiguous_and_cover_the_window() {
        let segments = timeline(&two_session_window(), &[], 0, 600);

        assert_eq!(segments.first().unwrap().from, 0);
        assert_eq!(segments.last().unwrap().to, 600);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
    }

    #[test]
    fn empty_log_is_one_idle_segment() {
        let segments = timeline(&[], &[], 0, 600);

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
        let segments = timeline(&two_session_window(), &[], 0, 600);

        assert!((waiting_share(&segments) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_stale_notification_decays_instead_of_painting_the_whole_lane() {
        // The reported bug: a session fires a Notification then goes silent (killed
        // terminal, no Stop ever). It must not fill the window with amber.
        let events = vec![Event::Notification {
            id: "dead".into(),
            at: 0,
            kind: None,
        }];

        let segments = timeline(&events, &[], 0, 10 * WAITING_TTL_SECS);

        let share = waiting_share(&segments);
        assert!(
            share < 0.2,
            "one stale notification painted {share} of the lane"
        );
    }

    #[test]
    fn a_notification_already_expired_before_the_window_paints_nothing() {
        // The worst offender: notified hours before the window, never seen again. By
        // `from` the wait has long decayed, so the window is all idle.
        let events = vec![Event::Notification {
            id: "dead".into(),
            at: 0,
            kind: None,
        }];
        let from = 5 * WAITING_TTL_SECS;

        let segments = timeline(&events, &[], from, from + WAITING_TTL_SECS);

        assert_eq!(waiting_share(&segments), 0.0);
    }

    #[test]
    fn a_recent_notification_still_counts_as_waiting_until_it_expires() {
        // Decay must not erase a genuine, current block.
        let events = vec![Event::Notification {
            id: "s1".into(),
            at: 0,
            kind: None,
        }];

        let segments = timeline(&events, &[], 0, WAITING_TTL_SECS);

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
                kind: None,
            },
            Event::Notification {
                id: "s1".into(),
                at: WAITING_TTL_SECS - 10,
                kind: None,
            },
        ];

        let segments = timeline(&events, &[], 0, 2 * WAITING_TTL_SECS - 20);

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

    /// Claude Code fires `Notification` for a permission prompt, but approving it
    /// fires nothing — the hook log has no event that clears the wait, so the row
    /// stayed amber while Claude worked. Transcript records written after the
    /// notification are proof the turn continued.
    #[test]
    fn transcript_activity_after_a_wait_means_the_prompt_was_answered() {
        let hooks = vec![Session {
            state: SessionState::WaitingOnYou,
            since: 1_000,
            last_active: 1_000,
            ..session("s1", SessionState::WaitingOnYou, None, Some(10))
        }];
        let transcripts = vec![Session {
            since: 1_224,
            last_active: 1_224,
            ..session("s1", SessionState::Idle, Some("working"), None)
        }];

        let merged = merge(hooks, transcripts, Vec::new(), 10_000);

        assert_eq!(merged[0].state, SessionState::Working);
        assert_eq!(
            merged[0].last_active, 1_224,
            "the transcript is the newest thing that happened"
        );
    }

    #[test]
    fn a_wait_with_no_transcript_activity_after_it_stays_waiting() {
        let hooks = vec![Session {
            state: SessionState::WaitingOnYou,
            since: 2_000,
            last_active: 2_000,
            ..session("s1", SessionState::WaitingOnYou, None, Some(10))
        }];
        let transcripts = vec![Session {
            since: 1_500,
            last_active: 1_500,
            ..session("s1", SessionState::Idle, Some("blocked"), None)
        }];

        let merged = merge(hooks, transcripts, Vec::new(), 10_000);

        assert_eq!(
            merged[0].state,
            SessionState::WaitingOnYou,
            "nothing happened after the prompt, so it is a real block"
        );
    }

    #[test]
    fn a_stopped_session_is_not_revived_by_its_own_final_records() {
        // `Stop` is an honest end of turn. Trailing transcript writes must not drag
        // it back to Working, or every finished session reads as busy.
        let hooks = vec![Session {
            state: SessionState::YourTurn,
            since: 1_000,
            last_active: 1_000,
            ..session("s1", SessionState::YourTurn, None, Some(10))
        }];
        let transcripts = vec![Session {
            since: 1_050,
            last_active: 1_050,
            ..session("s1", SessionState::Idle, Some("done"), None)
        }];

        let merged = merge(hooks, transcripts, Vec::new(), 10_000);

        assert_eq!(merged[0].state, SessionState::YourTurn);
    }

    fn quiet(state: SessionState, since: Timestamp) -> Vec<Session> {
        vec![Session {
            state,
            since,
            last_active: since,
            ..session("s1", state, None, Some(10))
        }]
    }

    fn agent_row(at: Timestamp) -> Vec<Session> {
        vec![Session {
            since: at,
            last_active: at,
            ..session("s1", SessionState::Idle, None, None)
        }]
    }

    /// The reported bug, at its source: the "waiting for your input" nudge means the
    /// main loop went quiet, which is your move — not Claude needing an answer. Only a
    /// permission prompt is a block.
    #[test]
    fn the_idle_nudge_is_your_turn_and_a_permission_prompt_is_a_block() {
        let nudge = |kind| {
            fold_default(
                &[Event::Notification {
                    id: "s1".into(),
                    at: 100,
                    kind,
                }],
                200,
            )[0]
            .state
        };

        assert_eq!(nudge(Some(NotificationKind::Idle)), SessionState::YourTurn);
        assert_eq!(
            nudge(Some(NotificationKind::Permission)),
            SessionState::WaitingOnYou
        );
        assert_eq!(
            nudge(None),
            SessionState::WaitingOnYou,
            "the log retains 30 days of lines written before the kind was recorded, \
             and they must not be the one case that hides a block"
        );
    }

    #[test]
    fn an_agents_records_make_a_quiet_session_working() {
        for state in [SessionState::YourTurn, SessionState::Idle] {
            let merged = merge(quiet(state, 1_000), Vec::new(), agent_row(1_060), 1_100);

            assert_eq!(
                merged[0].state,
                SessionState::Working,
                "an agent writing is work, whatever the main loop was doing ({state:?})"
            );
        }
    }

    /// A finished agent fires no hook of its own. Without a bound, one last record left
    /// the row green for the rest of the day — the same failure `WAITING_TTL_SECS` exists
    /// to prevent, on the other colour.
    #[test]
    fn an_agent_that_has_gone_quiet_stops_counting_as_work() {
        let wrote_at = 1_060;

        let fresh = merge(
            quiet(SessionState::YourTurn, 1_000),
            Vec::new(),
            agent_row(wrote_at),
            wrote_at + AGENT_TTL_SECS,
        );
        let stale = merge(
            quiet(SessionState::YourTurn, 1_000),
            Vec::new(),
            agent_row(wrote_at),
            wrote_at + AGENT_TTL_SECS + 1,
        );

        assert_eq!(
            fresh[0].state,
            SessionState::Working,
            "still within the TTL"
        );
        assert_eq!(
            stale[0].state,
            SessionState::YourTurn,
            "past it, the row goes back to what the hooks said"
        );
        assert_eq!(
            stale[0].last_active, wrote_at,
            "the records still happened, so recency keeps them"
        );
    }

    #[test]
    fn the_lane_lets_an_agents_work_decay_too() {
        let stopped = vec![Event::Stop {
            id: "s1".into(),
            at: 0,
        }];
        let wrote = vec![("s1".to_string(), 100, Origin::Agent)];
        let window = 100 + AGENT_TTL_SECS * 3;

        let segments = timeline(&stopped, &wrote, 0, window);

        let working: Timestamp = segments
            .iter()
            .filter(|s| s.state == SessionState::Working)
            .map(|s| s.to - s.from)
            .sum();
        assert_eq!(
            working, AGENT_TTL_SECS,
            "green for exactly as long as the agent's word is good: {segments:?}"
        );
        assert_eq!(
            segments.last().unwrap().state,
            SessionState::Idle,
            "and quiet after"
        );
    }

    #[test]
    fn a_block_is_not_ended_by_a_background_agents_records() {
        let merged = merge(
            quiet(SessionState::WaitingOnYou, 1_000),
            Vec::new(),
            agent_row(1_060),
            1_100,
        );

        assert_eq!(
            merged[0].state,
            SessionState::WaitingOnYou,
            "the agent writes whether or not you answered"
        );
    }

    #[test]
    fn the_lane_agrees_with_the_row_about_the_two_notifications() {
        let notified = |kind| {
            vec![Event::Notification {
                id: "s1".into(),
                at: 0,
                kind,
            }]
        };
        let by_agent = vec![("s1".to_string(), 100, Origin::Agent)];

        assert_eq!(
            waiting_share(&timeline(
                &notified(Some(NotificationKind::Idle)),
                &by_agent,
                0,
                200
            )),
            0.0,
            "a nudge never paints the lane amber"
        );
        assert_eq!(
            waiting_share(&timeline(
                &notified(Some(NotificationKind::Permission)),
                &by_agent,
                0,
                200
            )),
            1.0,
            "and an agent never paints a real block away"
        );
    }

    #[test]
    fn recency_takes_the_freshest_of_a_sessions_transcripts() {
        // A session's main transcript and its background agents' files arrive as
        // several rows with one id. The row must carry the latest of them: on the
        // stale one it drops out of a narrow window and stays dismissed, both of
        // which read as "nothing is happening here" while an agent works.
        let main = Session {
            since: 36_000,
            last_active: 36_000,
            ..session("s1", SessionState::Idle, Some("Fix login"), None)
        };
        let agent = Session {
            since: 43_200,
            last_active: 43_200,
            ..session("s1", SessionState::Idle, None, None)
        };

        let mut merged = merge(Vec::new(), vec![main], vec![agent], 43_300);

        assert_eq!(merged.len(), 1, "one session, not one row per file");
        assert_eq!(merged[0].last_active, 43_200);
        assert_eq!(
            merged[0].title.as_deref(),
            Some("Fix login"),
            "the agent lends no name"
        );

        retain_within(&mut merged, 40_000);
        assert_eq!(
            merged.len(),
            1,
            "so it stays inside a window it was active in"
        );
    }

    #[test]
    fn a_transcript_only_session_survives_the_merge() {
        let transcripts = vec![session("t1", SessionState::Idle, Some("Fix login"), None)];

        let merged = merge(Vec::new(), transcripts, Vec::new(), 10_000);

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

        let merged = merge(hooks, transcripts, Vec::new(), 10_000);

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
                kind: None,
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

        let merged = merge(hooks, transcripts, Vec::new(), 10_000);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].cwd, "/s1", "cwd comes from the transcript");
        assert_eq!(
            merged[0].state,
            SessionState::WaitingOnYou,
            "hook state still wins"
        );
    }

    /// Reported: a project opened and a session resumed, nothing typed, and the row read
    /// `Working` for twenty minutes. `SessionStart` means a session exists, not that
    /// Claude is doing anything — except for compaction, which happens mid-turn.
    #[test]
    fn opening_a_project_is_not_work() {
        let state_of = |source| {
            fold_default(
                &[Event::SessionStart {
                    id: "s1".into(),
                    cwd: "/p".into(),
                    pid: Some(10),
                    at: 100,
                    term: None,
                    source,
                }],
                200,
            )[0]
            .state
        };

        assert_eq!(state_of(Some(SessionSource::Opened)), SessionState::Idle);
        assert_eq!(
            state_of(Some(SessionSource::Compact)),
            SessionState::Working,
            "compaction interrupts a turn that is already running"
        );
        assert_eq!(
            state_of(None),
            SessionState::Working,
            "a line written before the source was recorded keeps its old reading"
        );
    }

    /// An agent writes every few seconds. Against `last_active` that revoked an explicit
    /// dismiss on the next poll, and kept revoking it for as long as the agent ran.
    #[test]
    fn a_dismissed_session_stays_dismissed_while_its_agent_writes() {
        let dismissed_at = 1_000;
        let mut sessions = vec![Session {
            state: SessionState::Working,
            since: 900,
            last_active: 1_500,
            ..session("s1", SessionState::Working, None, Some(10))
        }];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), dismissed_at)]),
            ..Default::default()
        };

        apply_dismissed(&mut sessions, &prefs);

        assert!(
            sessions.is_empty(),
            "the agent's writes are not the session acting again"
        );
    }

    #[test]
    fn a_dismissed_session_returns_when_it_changes_state() {
        let mut sessions = vec![Session {
            state: SessionState::WaitingOnYou,
            since: 1_100,
            last_active: 1_100,
            ..session("s1", SessionState::WaitingOnYou, None, Some(10))
        }];
        let prefs = ViewPrefs {
            dismissed: HashMap::from([("s1".to_string(), 1_000)]),
            ..Default::default()
        };

        apply_dismissed(&mut sessions, &prefs);

        assert_eq!(sessions.len(), 1, "it blocked on you after the dismiss");
    }

    #[test]
    fn an_idle_nudge_does_not_downgrade_a_block_that_is_still_open() {
        let events = vec![
            Event::Notification {
                id: "s1".into(),
                at: 100,
                kind: Some(NotificationKind::Permission),
            },
            Event::Notification {
                id: "s1".into(),
                at: 160,
                kind: Some(NotificationKind::Idle),
            },
        ];

        let sessions = fold_default(&events, 200);

        assert_eq!(
            sessions[0].state,
            SessionState::WaitingOnYou,
            "the prompt is still pending; only answering it or the TTL ends it"
        );
        assert_eq!(sessions[0].since, 100, "and the wait keeps its own clock");
    }

    #[test]
    fn fold_carries_the_terminal_hint_from_session_start() {
        let events = vec![Event::SessionStart {
            id: "s1".into(),
            cwd: "/p".into(),
            pid: Some(10),
            at: 100,
            term: Some(crate::terminal::Terminal {
                tmux_pane: Some("%3".into()),
                ..Default::default()
            }),
            source: None,
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

        apply_dismissed(&mut sessions, &prefs);

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

    fn started(id: &str, pid: u32, cwd: &str, at: Timestamp) -> Event {
        Event::SessionStart {
            id: id.into(),
            cwd: cwd.into(),
            pid: Some(pid),
            at,
            term: None,
            source: None,
        }
    }

    #[test]
    fn a_session_that_started_and_did_nothing_is_retired() {
        let events = vec![
            started("ghost", 9, "/p", 100),
            started("resumed", 9, "/p", 110),
            Event::UserPromptSubmit {
                id: "resumed".into(),
                at: 200,
            },
        ];

        let retired = superseded(&events);

        assert_eq!(retired.len(), 1);
        assert!(retired.contains("ghost"));
    }

    /// `/resume` switches back and forth, so two live sessions genuinely interleave
    /// under one pid. Retiring whichever acted least recently would hide a real one.
    #[test]
    fn interleaved_sessions_are_both_kept() {
        let events = vec![
            started("a", 9, "/p", 100),
            Event::UserPromptSubmit {
                id: "a".into(),
                at: 150,
            },
            started("b", 9, "/p", 200),
            Event::UserPromptSubmit {
                id: "b".into(),
                at: 250,
            },
            Event::UserPromptSubmit {
                id: "a".into(),
                at: 300,
            },
            Event::Stop {
                id: "b".into(),
                at: 350,
            },
        ];

        assert!(superseded(&events).is_empty());
    }

    /// The guarantee that makes this safe: a session blocked on you is never hidden,
    /// whatever else its process went on to do.
    #[test]
    fn a_waiting_session_is_never_retired() {
        let events = vec![
            started("waiting", 9, "/p", 100),
            Event::Notification {
                id: "waiting".into(),
                at: 150,
                kind: None,
            },
            started("other", 9, "/p", 200),
            Event::UserPromptSubmit {
                id: "other".into(),
                at: 900,
            },
        ];

        assert!(superseded(&events).is_empty());
    }

    #[test]
    fn the_session_the_process_is_actually_on_is_kept() {
        let events = vec![
            started("older", 9, "/p", 100),
            Event::Stop {
                id: "older".into(),
                at: 150,
            },
            started("current", 9, "/p", 200),
        ];

        assert!(
            superseded(&events).is_empty(),
            "nothing acted after `current` started, so it is the live one"
        );
    }

    #[test]
    fn a_recycled_pid_in_another_project_is_not_the_same_process() {
        let events = vec![
            started("old", 42, "/old-project", 100),
            started("new", 42, "/new-project", 900),
            Event::UserPromptSubmit {
                id: "new".into(),
                at: 950,
            },
        ];

        assert!(superseded(&events).is_empty());
    }

    fn at(id: &str, last_active: Timestamp) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/{id}"),
            pid: None,
            state: SessionState::Idle,
            since: last_active,
            last_active,
            title: None,
            git_branch: None,
            terminal: None,
        }
    }

    /// The span filter scoped the lane, the cost and the tokens, but never the list —
    /// so "last 4 hours" showed the same rows as "today", some of them days old.
    #[test]
    fn the_window_drops_a_session_that_went_quiet_before_it() {
        let mut sessions = vec![at("recent", 900), at("stale", 100)];

        retain_within(&mut sessions, 500);

        let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["recent"]);
    }

    #[test]
    fn a_session_active_exactly_at_the_boundary_is_inside_the_window() {
        let mut sessions = vec![at("edge", 500)];

        retain_within(&mut sessions, 500);

        assert_eq!(sessions.len(), 1);
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
                source: None,
            },
        ];

        let sessions = fold_default(&events, 200);

        assert_eq!(sessions.len(), 2);
    }
}
