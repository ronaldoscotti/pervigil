use std::collections::HashSet;

use crate::core::event::SessionId;
use crate::core::session::{Session, SessionState};

pub trait ProcessCheck {
    /// `None` when this platform can't determine liveness.
    fn is_alive(&self, pid: u32) -> Option<bool>;
}

pub struct SystemProcesses;

/// Read sessions whose process is *known* dead as idle, and return their ids. They are
/// not removed: the panel is the record of a day, and closing a window does not un-happen
/// it. Anything else — no pid, or a platform that can't answer — is left alone, since
/// absence of proof is not proof of death.
pub fn settle_dead(sessions: &mut [Session], check: &impl ProcessCheck) -> HashSet<SessionId> {
    let mut dead = HashSet::new();
    for session in sessions {
        if session
            .pid
            .is_some_and(|pid| check.is_alive(pid) == Some(false))
        {
            session.state = SessionState::Idle;
            dead.insert(session.id.clone());
        }
    }
    dead
}

#[cfg(unix)]
impl ProcessCheck for SystemProcesses {
    fn is_alive(&self, pid: u32) -> Option<bool> {
        // Signal 0 performs the permission and existence checks without delivering
        // anything. EPERM means the process exists but isn't ours.
        if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
            return Some(true);
        }
        Some(std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
    }
}

#[cfg(not(unix))]
impl ProcessCheck for SystemProcesses {
    fn is_alive(&self, _pid: u32) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::SessionState;

    struct Fake(Option<bool>);

    impl ProcessCheck for Fake {
        fn is_alive(&self, _pid: u32) -> Option<bool> {
            self.0
        }
    }

    fn session(pid: Option<u32>) -> Session {
        Session {
            id: "s1".into(),
            cwd: "/p".into(),
            pid,
            state: SessionState::Working,
            since: 0,
            last_active: 0,
            title: None,
            git_branch: None,
            terminal: None,
        }
    }

    fn settled(pid: Option<u32>, alive: Option<bool>) -> Vec<Session> {
        let mut sessions = vec![session(pid)];
        let dead = settle_dead(&mut sessions, &Fake(alive));

        assert_eq!(
            dead.contains(&sessions[0].id),
            sessions[0].state == SessionState::Idle,
            "the reported set and the settled state must say the same thing"
        );
        sessions
    }

    fn state(pid: Option<u32>, alive: Option<bool>) -> SessionState {
        settled(pid, alive)[0].state
    }

    #[test]
    fn a_live_process_keeps_its_session_as_it_is() {
        assert_eq!(state(Some(42), Some(true)), SessionState::Working);
    }

    /// The reported bug: closing the window erased the project from panel and settings.
    #[test]
    fn a_dead_process_settles_its_session_to_idle_without_dropping_it() {
        let sessions = settled(Some(42), Some(false));

        assert_eq!(sessions.len(), 1, "the day it worked still happened");
        assert_eq!(sessions[0].state, SessionState::Idle);
    }

    /// Why the state moves at all: a killed terminal fires no `Stop`, and a row still
    /// claiming a block would badge the tray forever.
    #[test]
    fn a_dead_process_cannot_still_be_blocked_on_you() {
        let mut sessions = vec![Session {
            state: SessionState::WaitingOnYou,
            ..session(Some(42))
        }];

        settle_dead(&mut sessions, &Fake(Some(false)));

        assert_eq!(sessions[0].state, SessionState::Idle);
    }

    #[test]
    fn a_session_with_no_pid_is_left_alone() {
        assert_eq!(
            state(None, Some(false)),
            SessionState::Working,
            "transcript-derived sessions carry no pid and nothing is known about them"
        );
    }

    #[test]
    fn a_platform_that_cannot_answer_leaves_the_session_alone() {
        assert_eq!(state(Some(42), None), SessionState::Working);
    }

    #[cfg(unix)]
    #[test]
    fn the_real_check_sees_this_test_process_as_alive() {
        assert_eq!(SystemProcesses.is_alive(std::process::id()), Some(true));
    }

    #[cfg(not(unix))]
    #[test]
    fn the_real_check_cannot_answer_off_unix() {
        // No syscall-backed impl yet, so liveness honestly degrades to "unknown"
        // and the caller keeps the session rather than hiding it.
        assert_eq!(SystemProcesses.is_alive(std::process::id()), None);
    }
}
