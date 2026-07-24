use crate::core::session::Session;

pub trait ProcessCheck {
    /// `None` when this platform can't determine liveness.
    fn is_alive(&self, pid: u32) -> Option<bool>;
}

pub struct SystemProcesses;

/// Hide sessions whose process is *known* dead. Anything else — no pid, or a
/// platform that can't answer — is kept: absence of proof is not proof of death,
/// and hiding a transcript-derived session would silently break the
/// hooks-not-installed path.
///
/// Only the list is filtered. Cost comes from transcripts, so a dead session's
/// spend still counts toward the totals.
pub fn retain_live(sessions: &mut Vec<Session>, check: &impl ProcessCheck) {
    sessions.retain(|session| match session.pid {
        Some(pid) => check.is_alive(pid) != Some(false),
        None => true,
    });
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
        }
    }

    fn survivors(pid: Option<u32>, alive: Option<bool>) -> usize {
        let mut sessions = vec![session(pid)];
        retain_live(&mut sessions, &Fake(alive));
        sessions.len()
    }

    #[test]
    fn a_live_process_keeps_its_session() {
        assert_eq!(survivors(Some(42), Some(true)), 1);
    }

    #[test]
    fn a_dead_process_hides_its_session() {
        assert_eq!(survivors(Some(42), Some(false)), 0);
    }

    #[test]
    fn a_session_with_no_pid_is_kept() {
        assert_eq!(
            survivors(None, Some(false)),
            1,
            "transcript-derived sessions carry no pid and must survive"
        );
    }

    #[test]
    fn a_platform_that_cannot_answer_keeps_the_session() {
        assert_eq!(survivors(Some(42), None), 1);
    }

    #[test]
    fn the_real_check_sees_this_test_process_as_alive() {
        assert_eq!(SystemProcesses.is_alive(std::process::id()), Some(true));
    }
}
