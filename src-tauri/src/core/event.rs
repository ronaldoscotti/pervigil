use serde::{Deserialize, Serialize};

pub type SessionId = String;
pub type Timestamp = u64;

/// One line of the append-only event log, written by the `record` hook shim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    SessionStart {
        id: SessionId,
        cwd: String,
        pid: u32,
        at: Timestamp,
    },
    Notification {
        id: SessionId,
        at: Timestamp,
    },
    Stop {
        id: SessionId,
        at: Timestamp,
    },
    UserPromptSubmit {
        id: SessionId,
        at: Timestamp,
    },
}

impl Event {
    pub fn id(&self) -> &SessionId {
        match self {
            Event::SessionStart { id, .. }
            | Event::Notification { id, .. }
            | Event::Stop { id, .. }
            | Event::UserPromptSubmit { id, .. } => id,
        }
    }

    pub fn at(&self) -> Timestamp {
        match self {
            Event::SessionStart { at, .. }
            | Event::Notification { at, .. }
            | Event::Stop { at, .. }
            | Event::UserPromptSubmit { at, .. } => *at,
        }
    }
}
