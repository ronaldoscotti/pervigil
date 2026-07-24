//! The full ingestion path: what the shim writes must fold back out.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pervigil_lib::core::event::parse_log;
use pervigil_lib::core::session::{SessionState, ViewPrefs};
use pervigil_lib::core::store::fold;
use pervigil_lib::core::terminal::Terminal;
use pervigil_lib::io::record::{append_line, build_event};

const PAYLOAD: &str = r#"{"session_id":"s1","cwd":"/p"}"#;

struct TempLog(PathBuf);

impl TempLog {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!("pervigil-{name}-{nanos}")))
    }

    fn path(&self) -> PathBuf {
        self.0.join("events.jsonl")
    }

    fn record(&self, kind: &str, at: u64) {
        let event = build_event(kind, PAYLOAD, at, Some(7), Terminal::default())
            .expect("payload should build");
        let line = serde_json::to_string(&event).expect("event should serialize");
        append_line(&self.path(), &line).expect("append should succeed");
    }

    fn fold_at(&self, now: u64) -> Vec<pervigil_lib::core::session::Session> {
        let contents = std::fs::read_to_string(self.path()).expect("log should be readable");
        let (events, _) = parse_log(&contents);
        fold(&events, now, &ViewPrefs::default())
    }
}

impl Drop for TempLog {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

#[test]
fn a_recorded_session_folds_back_out_of_the_log() {
    let log = TempLog::new("roundtrip");

    log.record("SessionStart", 100);
    log.record("Notification", 200);
    let sessions = log.fold_at(300);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "s1");
    assert_eq!(sessions[0].cwd, "/p");
    assert_eq!(sessions[0].pid, Some(7));
    assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
    assert_eq!(sessions[0].since, 200);
}

#[test]
fn a_torn_line_does_not_blind_the_panel() {
    let log = TempLog::new("torn");

    log.record("SessionStart", 100);
    append_line(&log.path(), "{\"type\":\"Notif").expect("append should succeed");
    log.record("Notification", 200);
    let sessions = log.fold_at(300);

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
}
