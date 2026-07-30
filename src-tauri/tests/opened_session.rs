//! Opening a project is not work. `SessionStart` fires on startup, `--resume` and
//! `/clear` before anything has been asked of Claude, and a row that reads `Working`
//! then is telling you the machine is busy when it is sitting at its prompt.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use specola_lib::app::App;
use specola_lib::core::session::SessionState;
use specola_lib::core::span::Span;

/// A project resumed twenty minutes ago, whose transcript has not moved since last
/// week — the reported case.
fn home_with_a_resumed_session(source: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("specola-opened-{source}-{nanos}"));
    let project = home.join(".claude/projects/-Users-x-bebe");
    std::fs::create_dir_all(home.join(".specola")).unwrap();
    std::fs::create_dir_all(&project).unwrap();

    let resumed = Local::now().timestamp() - 20 * 60;
    let last_week = Local::now().timestamp() - 7 * 24 * 60 * 60;
    let pid = std::process::id();
    std::fs::write(
        home.join(".specola/events.jsonl"),
        format!(
            r#"{{"type":"SessionStart","id":"bebe","cwd":"/Users/x/bebe","pid":{pid},"at":{resumed},"source":"{source}"}}
"#
        ),
    )
    .unwrap();
    std::fs::write(
        project.join("bebe.jsonl"),
        format!(
            "{{\"type\":\"user\",\"sessionId\":\"bebe\",\"cwd\":\"/Users/x/bebe\",\"timestamp\":\"{ts}\"}}\n\
             {{\"type\":\"last-prompt\",\"sessionId\":\"bebe\",\"lastPrompt\":\"extract the ebook text\"}}\n",
            ts = chrono::DateTime::from_timestamp(last_week, 0)
                .unwrap()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        ),
    )
    .unwrap();

    home
}

#[test]
fn a_session_resumed_with_nothing_asked_of_it_is_not_working() {
    let home = home_with_a_resumed_session("Opened");
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    assert_eq!(snapshot.sessions.len(), 1, "{:?}", snapshot.sessions);
    assert_eq!(snapshot.sessions[0].state, SessionState::Idle);
    assert_eq!(snapshot.waiting, 0);
    assert_eq!(
        snapshot.waiting_share, 0.0,
        "and it paints nothing in the lane: {:?}",
        snapshot.segments
    );

    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_compaction_mid_turn_still_reads_as_working() {
    let home = home_with_a_resumed_session("Compact");
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    assert_eq!(snapshot.sessions[0].state, SessionState::Working);

    std::fs::remove_dir_all(&home).ok();
}
