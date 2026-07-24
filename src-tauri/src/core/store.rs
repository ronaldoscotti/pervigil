use super::event::{Event, Timestamp};
use super::session::{Session, SessionState, ViewPrefs};

/// Replay the event log into the current set of sessions.
///
/// Pure: no clock, no filesystem. `now` is supplied by the caller.
pub fn fold(events: &[Event], _now: Timestamp, _prefs: &ViewPrefs) -> Vec<Session> {
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
                if let Some(session) = sessions.iter_mut().find(|s| &s.id == id) {
                    session.state = SessionState::WaitingOnYou;
                    session.since = *at;
                    session.last_active = *at;
                }
            }
            _ => {}
        }
    }

    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::SessionState;

    #[test]
    fn session_start_then_notification_is_waiting() {
        let events = vec![
            Event::SessionStart {
                id: "s1".into(),
                cwd: "/p".into(),
                pid: 10,
                at: 100,
            },
            Event::Notification {
                id: "s1".into(),
                at: 200,
            },
        ];

        let sessions = fold(&events, 250, &ViewPrefs::default());

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
        assert_eq!(sessions[0].since, 200);
    }
}
