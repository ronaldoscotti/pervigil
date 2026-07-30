use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::event::{SessionId, Timestamp};
use super::terminal::Terminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Working,
    /// Claude explicitly needs an answer from you — a permission prompt, or any
    /// notification whose kind this version does not recognise.
    WaitingOnYou,
    /// A live session sitting at its prompt, your move: the turn finished, or the idle
    /// nudge that follows one. Softer than `WaitingOnYou`, which is reserved for Claude
    /// actually waiting on an answer.
    YourTurn,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub cwd: String,
    /// `None` for transcript-derived sessions. A missing pid is not evidence of death.
    pub pid: Option<u32>,
    pub state: SessionState,
    pub since: Timestamp,
    pub last_active: Timestamp,
    /// Only transcripts carry one; hook-derived sessions start `None`.
    pub title: Option<String>,
    /// `HEAD` and empty are normalised to `None` — neither names a branch.
    pub git_branch: Option<String>,
    /// Where it runs, for click-to-focus. `None` for transcript-derived sessions and
    /// sessions started before the shim captured it.
    pub terminal: Option<Terminal>,
}

/// What the dismiss (check) action does to a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DismissMode {
    /// Hide it until it acts again.
    #[default]
    Hide,
    /// Keep it in the list but demote it to idle — a "mark as read".
    Read,
}

/// Lives in config, not the event log, so it enters `fold` as data and keeps it pure.
#[derive(Debug, Clone, Default)]
pub struct ViewPrefs {
    pub pinned: HashSet<SessionId>,
    pub dismissed: HashMap<SessionId, Timestamp>,
    pub dismiss_mode: DismissMode,
}

/// A session's display name: the last non-empty path segment of its cwd. Handles
/// both separators — a Windows transcript can be read on any platform.
pub fn project(cwd: &str) -> String {
    cwd.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(cwd)
        .to_string()
}
