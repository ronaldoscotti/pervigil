use chrono::DateTime;
use serde::Deserialize;

use crate::core::event::Timestamp;
use crate::core::session::{Session, SessionState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    session_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    timestamp: Option<String>,
    ai_title: Option<String>,
}

/// Build a session from one Claude Code transcript. Returns `None` if no record
/// carries a `sessionId`.
pub fn parse_session(contents: &str) -> Option<Session> {
    let mut id = None;
    let mut cwd = None;
    let mut git_branch = None;
    let mut title = None;
    let mut last_active: Timestamp = 0;

    for record in contents
        .lines()
        .filter_map(|line| serde_json::from_str::<Record>(line).ok())
    {
        id = id.or(record.session_id);
        cwd = cwd.or(record.cwd);
        title = record.ai_title.or(title);
        if let Some(branch) = record.git_branch.filter(|b| b != "HEAD" && !b.is_empty()) {
            git_branch = Some(branch);
        }
        if let Some(at) = record.timestamp.as_deref().and_then(to_epoch) {
            last_active = last_active.max(at);
        }
    }

    Some(Session {
        id: id?,
        cwd: cwd.unwrap_or_default(),
        pid: None,
        state: SessionState::Idle,
        since: last_active,
        last_active,
        title,
        git_branch,
    })
}

fn to_epoch(timestamp: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|at| at.timestamp() as Timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::SessionState;

    const TRANSCRIPT: &str = concat!(
        r#"{"type":"user","sessionId":"5f5f47fc","cwd":"/Users/x/pervigil","gitBranch":"feat/timeline","timestamp":"2026-07-23T10:00:00.000Z"}"#,
        "\n",
        r#"{"type":"ai-title","sessionId":"5f5f47fc","aiTitle":"Design the panel"}"#,
        "\n",
        r#"garbage that is not json"#,
        "\n",
        r#"{"type":"assistant","sessionId":"5f5f47fc","cwd":"/Users/x/pervigil","gitBranch":"feat/timeline","timestamp":"2026-07-23T16:39:50.417Z"}"#,
        "\n",
    );

    #[test]
    fn reads_id_cwd_title_and_last_activity() {
        let session = parse_session(TRANSCRIPT).expect("transcript should yield a session");

        assert_eq!(session.id, "5f5f47fc");
        assert_eq!(session.cwd, "/Users/x/pervigil");
        assert_eq!(session.title.as_deref(), Some("Design the panel"));
        assert_eq!(session.git_branch.as_deref(), Some("feat/timeline"));
        assert_eq!(session.last_active, 1_784_824_790);
    }

    #[test]
    fn a_transcript_session_is_idle_with_no_pid() {
        let session = parse_session(TRANSCRIPT).unwrap();

        assert_eq!(session.state, SessionState::Idle);
        assert_eq!(session.pid, None, "transcripts carry no pid");
    }

    #[test]
    fn a_transcript_without_a_title_still_yields_a_session() {
        let contents = r#"{"type":"user","sessionId":"abc","cwd":"/p","timestamp":"2026-07-23T10:00:00.000Z"}"#;

        let session = parse_session(contents).expect("session should still be built");

        assert_eq!(session.id, "abc");
        assert_eq!(session.title, None);
    }

    #[test]
    fn head_is_not_a_real_branch() {
        let contents = r#"{"type":"user","sessionId":"abc","cwd":"/p","gitBranch":"HEAD","timestamp":"2026-07-23T10:00:00.000Z"}"#;

        let session = parse_session(contents).unwrap();

        assert_eq!(session.git_branch, None);
    }

    #[test]
    fn a_transcript_with_no_session_id_yields_nothing() {
        assert!(parse_session(r#"{"type":"file-history-delta"}"#).is_none());
    }
}
