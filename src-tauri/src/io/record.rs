use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use serde::Deserialize;

use crate::core::event::{Event, Timestamp};
use crate::core::terminal::Terminal;

#[derive(Deserialize)]
struct Hook {
    session_id: Option<String>,
    cwd: Option<String>,
}

/// Turn a hook payload into an event. The kind comes from the argv the hook snippet
/// supplies, so we never depend on the payload naming itself. `term` is captured from
/// the shim's environment and only rides on `SessionStart`.
pub fn build_event(
    kind: &str,
    payload: &str,
    at: Timestamp,
    pid: Option<u32>,
    term: Terminal,
) -> Option<Event> {
    let hook: Hook = serde_json::from_str(payload).ok()?;
    let id = hook.session_id?;

    match kind {
        "SessionStart" => Some(Event::SessionStart {
            id,
            cwd: hook.cwd.unwrap_or_default(),
            pid,
            at,
            term: term.some(),
        }),
        "Notification" => Some(Event::Notification { id, at }),
        "Stop" => Some(Event::Stop { id, at }),
        "UserPromptSubmit" => Some(Event::UserPromptSubmit { id, at }),
        _ => None,
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
            }
        );
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

        assert!(matches!(
            event,
            Some(Event::SessionStart { term: None, .. })
        ));
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

    #[test]
    fn an_unknown_kind_is_ignored_rather_than_fatal() {
        assert!(build_event("Nonsense", PAYLOAD, 1, Some(1), Terminal::default()).is_none());
    }

    #[test]
    fn a_payload_without_a_session_id_is_ignored() {
        assert!(build_event("Stop", r#"{"cwd":"/p"}"#, 1, Some(1), Terminal::default()).is_none());
    }

    #[test]
    fn a_payload_that_is_not_json_is_ignored() {
        assert!(build_event("Stop", "not json at all", 1, Some(1), Terminal::default()).is_none());
    }

    #[test]
    fn concurrent_appends_never_interleave() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pervigil-append-{nanos}"));
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
