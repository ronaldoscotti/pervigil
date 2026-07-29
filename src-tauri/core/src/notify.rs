//! What to alert about — decided here, over values. Firing it is Tauri's job.

use std::collections::HashMap;

use super::event::SessionId;
use super::session::{project, Session, SessionState};
use super::store;

/// A native notification to fire — computed in `snapshot`, drained and shown by the
/// command wrapper so the pure pipeline stays free of Tauri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

/// The last tier of spec item 13: a session hooks saw but no transcript names still
/// gets something to be called.
pub fn name(session: &Session) -> String {
    session
        .title
        .clone()
        .unwrap_or_else(|| session.id.chars().take(8).collect())
}

/// Advance the seen-state baseline and return a notice per session that just entered
/// waiting. The first observation (`seen` is `None`) only primes — it never fires, so
/// launch is silent. The baseline advances even when notifications are off, so
/// re-enabling them doesn't replay a backlog.
pub fn notices(
    seen: &mut Option<HashMap<SessionId, SessionState>>,
    notifications: bool,
    sessions: &[Session],
) -> Vec<Notice> {
    let notices = match (seen.as_ref(), notifications) {
        (Some(previous), true) => store::newly_waiting(previous, sessions)
            .into_iter()
            .map(|session| Notice {
                title: format!("{} — waiting on you", project(&session.cwd)),
                body: name(session),
            })
            .collect(),
        _ => Vec::new(),
    };
    *seen = Some(store::states(sessions));
    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting_session(id: &str, project: &str) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/Users/x/{project}"),
            pid: Some(1),
            state: SessionState::WaitingOnYou,
            since: 0,
            last_active: 0,
            title: Some("do the thing".into()),
            git_branch: None,
            terminal: None,
        }
    }

    #[test]
    fn the_first_snapshot_primes_silently() {
        let mut seen = None;

        let fired = notices(&mut seen, true, &[waiting_session("s1", "proj")]);

        assert!(
            fired.is_empty(),
            "launch must not shout about existing waits"
        );
        assert!(seen.is_some(), "but the baseline is now set");
    }

    #[test]
    fn entering_waiting_after_priming_fires_a_notice() {
        let mut seen = Some(HashMap::from([("s1".to_string(), SessionState::Working)]));

        let fired = notices(&mut seen, true, &[waiting_session("s1", "proj")]);

        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].title, "proj — waiting on you");
        assert_eq!(fired[0].body, "do the thing");
    }

    #[test]
    fn notifications_off_fires_nothing_but_still_advances_the_baseline() {
        let mut seen = Some(HashMap::from([("s1".to_string(), SessionState::Working)]));
        let fired = notices(&mut seen, false, &[waiting_session("s1", "proj")]);

        assert!(fired.is_empty());
        assert_eq!(
            seen.unwrap().get("s1"),
            Some(&SessionState::WaitingOnYou),
            "re-enabling must not replay this as new"
        );
    }

    #[test]
    fn a_session_no_transcript_names_falls_back_to_a_short_id() {
        let session = Session {
            id: "abcdef1234-5678".into(),
            cwd: "/p".into(),
            pid: Some(1),
            state: SessionState::Working,
            since: 0,
            last_active: 0,
            title: None,
            git_branch: None,
            terminal: None,
        };

        assert_eq!(name(&session), "abcdef12");
    }

    #[test]
    fn a_project_is_the_last_segment_of_its_path() {
        assert_eq!(project("/Users/x/work/specola"), "specola");
        assert_eq!(project("/Users/x/work/specola/"), "specola");
        assert_eq!(project(r"C:\Users\x\specola"), "specola");
        assert_eq!(project(""), "");
    }
}
