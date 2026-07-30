use serde::{Deserialize, Serialize};

use super::terminal::Terminal;

pub type SessionId = String;
pub type Timestamp = u64;

/// Why Claude Code raised a notification. It fires for two reasons that look the same
/// in the log and mean opposite things to a panel: it needs an answer from you, or the
/// main loop has simply been idle for a minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationKind {
    /// A permission prompt, an elicitation — anything only you can resolve.
    Permission,
    /// The "waiting for your input" nudge, 60s after the main loop went quiet.
    Idle,
}

impl NotificationKind {
    /// Maps the hook payload's `notification_type`, and `None` for anything this
    /// version does not recognise — the log records what the payload said, never a
    /// guess. The reader supplies the default, and it is the one that cannot hide a
    /// block: see `store::after_notification`.
    pub fn from_hook(notification_type: Option<&str>) -> Option<Self> {
        match notification_type {
            Some("permission_prompt") => Some(Self::Permission),
            Some("idle_prompt") => Some(Self::Idle),
            _ => None,
        }
    }
}

/// How a session came to exist. `SessionStart` fires for all of these, and only one of
/// them means Claude is working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionSource {
    /// A new `claude`, a `--resume`, or a `/clear`: alive, with nothing running.
    Opened,
    /// Auto or manual compaction, which happens mid-turn.
    Compact,
}

impl SessionSource {
    /// Maps the hook payload's `source`, and `None` for anything this version does not
    /// recognise. As with [`NotificationKind::from_hook`], the reader defaults — there
    /// to the safe side, here to the old reading.
    pub fn from_hook(source: Option<&str>) -> Option<Self> {
        match source {
            Some("startup" | "resume" | "clear") => Some(Self::Opened),
            Some("compact") => Some(Self::Compact),
            _ => None,
        }
    }
}

/// One line of the append-only event log, written by the `record` shim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    SessionStart {
        id: SessionId,
        cwd: String,
        /// `None` where the platform can't report the parent pid.
        #[serde(default)]
        pid: Option<u32>,
        at: Timestamp,
        /// Where the session runs, for click-to-focus. Absent on older log lines and
        /// when the shim captured no signal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        term: Option<Terminal>,
        /// `None` on every line written before the shim recorded it, which keeps the
        /// old reading — see [`SessionSource::from_hook`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<SessionSource>,
    },
    Notification {
        id: SessionId,
        at: Timestamp,
        /// `None` on every line written before the shim recorded it, which counts as
        /// [`NotificationKind::Permission`] — see `store::answered`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<NotificationKind>,
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

/// Returns the valid events and the count of unreadable lines. A half-written line
/// from a crashed shim must never blind the panel, so corrupt lines are skipped.
pub fn parse_log(contents: &str) -> (Vec<Event>, usize) {
    let mut events = Vec::new();
    let mut skipped = 0;

    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str(line) {
            Ok(event) => events.push(event),
            Err(_) => skipped += 1,
        }
    }

    (events, skipped)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn every_variant() -> Vec<Event> {
        vec![
            Event::SessionStart {
                id: "s1".into(),
                cwd: "/p".into(),
                pid: Some(1),
                at: 10,
                term: None,
                source: Some(SessionSource::Opened),
            },
            Event::Notification {
                id: "s1".into(),
                at: 20,
                kind: Some(NotificationKind::Idle),
            },
            Event::Stop {
                id: "s1".into(),
                at: 30,
            },
            Event::UserPromptSubmit {
                id: "s1".into(),
                at: 40,
            },
        ]
    }

    #[test]
    fn a_session_start_without_a_pid_still_parses() {
        let line = r#"{"type":"SessionStart","id":"s1","cwd":"/p","at":10}"#;

        let (events, skipped) = parse_log(line);

        assert_eq!(skipped, 0);
        assert_eq!(
            events,
            vec![Event::SessionStart {
                id: "s1".into(),
                cwd: "/p".into(),
                pid: None,
                at: 10,
                term: None,
                source: None,
            }]
        );
    }

    #[test]
    fn every_variant_round_trips_through_the_log_format() {
        for event in every_variant() {
            let line = serde_json::to_string(&event).expect("event should serialize");

            let (parsed, skipped) = parse_log(&line);

            assert_eq!(skipped, 0);
            assert_eq!(parsed, vec![event]);
        }
    }

    #[test]
    fn corrupt_and_blank_lines_are_skipped_not_fatal() {
        let log = "{\"type\":\"Notification\",\"id\":\"s1\",\"at\":20}\n\
                   half-written garbage\n\
                   \n\
                   {\"type\":\"Stop\",\"id\":\"s1\",\"at\":30}\n";

        let (parsed, skipped) = parse_log(log);

        assert_eq!(parsed.len(), 2);
        assert_eq!(skipped, 1);
    }
}
