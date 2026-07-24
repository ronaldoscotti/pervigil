use serde::{Deserialize, Serialize};

/// Where a session is running, captured by the `record` shim at `SessionStart` — the
/// one moment pervigil is inside the session's own process tree and can read its
/// environment. Nothing else recovers this later; transcripts don't carry it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    /// `$TERM_PROGRAM` — `vscode`, `iTerm.app`, `Apple_Terminal`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// `$TMUX_PANE` — e.g. `%3`. Present only inside tmux, whatever hosts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_pane: Option<String>,
    /// `$ITERM_SESSION_ID` — `w0t1p2:UUID`; the UUID addresses the iTerm2 session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterm_session: Option<String>,
}

impl Terminal {
    /// `None` when no signal was captured — an empty hint is the same as no hint, and
    /// keeps such a session off the precise tiers.
    pub fn some(self) -> Option<Terminal> {
        (self != Terminal::default()).then_some(self)
    }
}
