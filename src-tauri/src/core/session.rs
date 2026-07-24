use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::event::{SessionId, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Working,
    WaitingOnYou,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub cwd: String,
    /// `None` for sessions discovered from transcripts, which carry no pid. A missing
    /// pid is not evidence the process died.
    pub pid: Option<u32>,
    pub state: SessionState,
    /// When the current state began — drives the elapsed timer.
    pub since: Timestamp,
    pub last_active: Timestamp,
}

/// View-layer preferences that `fold` needs to sort and filter. They live in config,
/// not the event log, so they enter as data to keep `fold` pure.
#[derive(Debug, Clone, Default)]
pub struct ViewPrefs {
    pub pinned: HashSet<SessionId>,
    pub dismissed: HashMap<SessionId, Timestamp>,
}
