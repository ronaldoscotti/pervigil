use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::Deserialize;

use crate::core::event::{Event, NotificationKind, SessionSource, Timestamp};
use crate::core::terminal::Terminal;

#[derive(Deserialize)]
struct Hook {
    session_id: Option<String>,
    cwd: Option<String>,
    /// `permission_prompt`, `idle_prompt`, or whatever Claude Code adds next. Only
    /// `Notification` carries it.
    notification_type: Option<String>,
    /// `startup`, `resume`, `clear` or `compact`. Only `SessionStart` carries it.
    source: Option<String>,
}

/// Why a hook payload produced no event. Three different operational problems: a
/// truncated pipe, a payload whose shape changed, and a hook kind we do not handle.
#[derive(Debug, PartialEq, Eq)]
pub enum IngestError {
    Malformed(String),
    NoSessionId,
    UnknownKind(String),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestError::Malformed(why) => write!(f, "payload is not valid JSON: {why}"),
            IngestError::NoSessionId => write!(f, "payload carries no session_id"),
            IngestError::UnknownKind(kind) => write!(f, "unhandled hook kind: {kind}"),
        }
    }
}

impl std::error::Error for IngestError {}

/// Turn a hook payload into an event. The kind comes from the argv the hook snippet
/// supplies, so we never depend on the payload naming itself. `term` is captured from
/// the shim's environment and only rides on `SessionStart`.
pub fn build_event(
    kind: &str,
    payload: &str,
    at: Timestamp,
    pid: Option<u32>,
    term: Terminal,
) -> Result<Event, IngestError> {
    let hook: Hook =
        serde_json::from_str(payload).map_err(|e| IngestError::Malformed(e.to_string()))?;
    let id = hook.session_id.ok_or(IngestError::NoSessionId)?;

    match kind {
        "SessionStart" => Ok(Event::SessionStart {
            id,
            cwd: hook.cwd.unwrap_or_default(),
            pid,
            at,
            term: term.some(),
            source: SessionSource::from_hook(hook.source.as_deref()),
        }),
        "Notification" => Ok(Event::Notification {
            id,
            at,
            kind: NotificationKind::from_hook(hook.notification_type.as_deref()),
        }),
        "Stop" => Ok(Event::Stop { id, at }),
        "UserPromptSubmit" => Ok(Event::UserPromptSubmit { id, at }),
        _ => Err(IngestError::UnknownKind(kind.to_string())),
    }
}

/// Append one line. A single `O_APPEND` write below `PIPE_BUF` is atomic, so parallel
/// sessions can never interleave halfway through a line.
pub fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format!("{line}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    const PAYLOAD: &str = r#"{"session_id":"s1","cwd":"/p","transcript_path":"/t.jsonl"}"#;

    #[test]
    fn builds_a_session_start_carrying_cwd_pid_and_terminal() {
        let term = Terminal {
            tmux_pane: Some("%3".into()),
            ..Default::default()
        };

        let event = build_event("SessionStart", PAYLOAD, 1_000, Some(42), term.clone()).unwrap();

        assert_eq!(
            event,
            Event::SessionStart {
                id: "s1".into(),
                cwd: "/p".into(),
                pid: Some(42),
                at: 1_000,
                term: Some(term),
                source: None,
            }
        );
    }

    #[test]
    fn the_notification_type_decides_the_kind() {
        let kind_of = |payload: &str| match build_event(
            "Notification",
            payload,
            1,
            None,
            Terminal::default(),
        ) {
            Ok(Event::Notification { kind, .. }) => kind,
            other => panic!("expected a notification, got {other:?}"),
        };

        assert_eq!(
            kind_of(r#"{"session_id":"s1","notification_type":"idle_prompt"}"#),
            Some(NotificationKind::Idle)
        );
        assert_eq!(
            kind_of(r#"{"session_id":"s1","notification_type":"permission_prompt"}"#),
            Some(NotificationKind::Permission)
        );
        assert_eq!(
            kind_of(r#"{"session_id":"s1","notification_type":"something_new"}"#),
            None,
            "a type we do not know is recorded as unknown, not guessed at"
        );
        assert_eq!(kind_of(r#"{"session_id":"s1"}"#), None);
    }

    #[test]
    fn how_the_session_started_is_recorded() {
        let source_of = |payload: &str| match build_event(
            "SessionStart",
            payload,
            1,
            None,
            Terminal::default(),
        ) {
            Ok(Event::SessionStart { source, .. }) => source,
            other => panic!("expected a session start, got {other:?}"),
        };

        for opened in ["startup", "resume", "clear"] {
            let payload = format!(r#"{{"session_id":"s1","source":"{opened}"}}"#);
            assert_eq!(source_of(&payload), Some(SessionSource::Opened));
        }
        assert_eq!(
            source_of(r#"{"session_id":"s1","source":"compact"}"#),
            Some(SessionSource::Compact),
            "compaction is the one source that happens mid-turn"
        );
        assert_eq!(source_of(r#"{"session_id":"s1"}"#), None);
    }

    #[test]
    fn an_empty_terminal_hint_is_stored_as_none() {
        let event = build_event(
            "SessionStart",
            PAYLOAD,
            1_000,
            Some(42),
            Terminal::default(),
        );

        assert!(matches!(event, Ok(Event::SessionStart { term: None, .. })));
    }

    #[test]
    fn builds_the_stateless_kinds() {
        let notification = build_event(
            "Notification",
            PAYLOAD,
            2_000,
            Some(42),
            Terminal::default(),
        )
        .unwrap();
        let stop = build_event("Stop", PAYLOAD, 3_000, Some(42), Terminal::default()).unwrap();

        assert_eq!(
            notification,
            Event::Notification {
                kind: None,
                id: "s1".into(),
                at: 2_000
            }
        );
        assert_eq!(
            stop,
            Event::Stop {
                id: "s1".into(),
                at: 3_000
            }
        );
    }

    /// The three ways ingestion fails are three different operational problems: a
    /// broken pipe, a hook payload that changed shape, and a hook we do not handle.
    /// Collapsing them into `None` made them indistinguishable in a log.
    #[test]
    fn a_payload_that_is_not_json_says_so() {
        let error = build_event("Stop", "not json at all", 1, Some(1), Terminal::default());

        assert!(matches!(error, Err(IngestError::Malformed(_))));
    }

    #[test]
    fn a_payload_without_a_session_id_says_so() {
        let error = build_event("Stop", r#"{"cwd":"/p"}"#, 1, Some(1), Terminal::default());

        assert_eq!(error.unwrap_err(), IngestError::NoSessionId);
    }

    #[test]
    fn an_unknown_kind_names_the_kind() {
        let error = build_event("Nonsense", PAYLOAD, 1, Some(1), Terminal::default());

        assert_eq!(
            error.unwrap_err(),
            IngestError::UnknownKind("Nonsense".into())
        );
    }

    #[test]
    fn the_three_causes_are_distinguishable_to_an_operator() {
        let messages = [
            build_event("Stop", "{", 1, None, Terminal::default()),
            build_event("Stop", "{}", 1, None, Terminal::default()),
            build_event("Nope", PAYLOAD, 1, None, Terminal::default()),
        ]
        .map(|r| r.unwrap_err().to_string());

        assert_eq!(
            messages
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            3,
            "three causes must not read as one"
        );
    }

    #[test]
    fn concurrent_appends_never_interleave() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("specola-append-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("events.jsonl");

        let writers: Vec<_> = (0..8)
            .map(|writer| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        append_line(&path, &format!("{{\"writer\":{writer}}}")).unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }

        let written = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert_eq!(
            lines.len(),
            400,
            "every append must produce exactly one line"
        );
        for line in lines {
            assert!(
                line.starts_with("{\"writer\":") && line.ends_with('}'),
                "torn line: {line}"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
