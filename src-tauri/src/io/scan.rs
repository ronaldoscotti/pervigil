use std::collections::HashMap;
use std::fs::{DirEntry, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::core::event::{SessionId, Timestamp};
use crate::core::pricing::UsageEntry;
use crate::core::session::Session;

use super::transcript::Transcript;

#[derive(Default)]
struct Cached {
    consumed: u64,
    transcript: Transcript,
}

impl Cached {
    /// Reads only the bytes appended since the last poll, and only up to the last
    /// complete line — a transcript is often mid-write when we look at it.
    fn absorb_appended(&mut self, path: &Path) {
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let len = file.metadata().map_or(0, |meta| meta.len());
        if len < self.consumed {
            *self = Cached::default();
        }
        if len == self.consumed || file.seek(SeekFrom::Start(self.consumed)).is_err() {
            return;
        }

        let mut appended = Vec::new();
        if file.read_to_end(&mut appended).is_err() {
            return;
        }
        let Some(end) = appended.iter().rposition(|byte| *byte == b'\n') else {
            return;
        };

        for line in String::from_utf8_lossy(&appended[..=end]).lines() {
            self.transcript.absorb(line);
        }
        self.consumed += end as u64 + 1;
    }
}

/// One poll's worth of transcript-derived state.
pub struct Scan {
    pub sessions: Vec<Session>,
    pub usage: HashMap<SessionId, Vec<UsageEntry>>,
}

/// Incremental reader over `~/.claude/projects/**/*.jsonl`. Those files are
/// append-only and run to gigabytes, so re-reading them on every poll is not an
/// option: each file is remembered by how much of it we have already consumed.
#[derive(Default)]
pub struct Scanner {
    files: HashMap<PathBuf, Cached>,
}

impl Scanner {
    /// Two floors: between them a transcript is priced but not listed. Files
    /// untouched since the lower one are skipped — append-only, so they cannot hold
    /// anything in either window.
    pub fn scan(&mut self, root: &Path, sessions_since: Timestamp, usage_since: Timestamp) -> Scan {
        let mut sessions = Vec::new();
        let mut usage: HashMap<SessionId, Vec<UsageEntry>> = HashMap::new();

        for (path, modified) in transcripts(root, usage_since.min(sessions_since)) {
            let cached = self.files.entry(path.clone()).or_default();
            cached.absorb_appended(&path);

            let Some(session) = cached.transcript.session() else {
                continue;
            };
            usage
                .entry(session.id.clone())
                .or_default()
                .extend(cached.transcript.usage.iter().cloned());
            if modified >= sessions_since {
                sessions.push(session);
            }
        }

        Scan { sessions, usage }
    }
}

/// A project holds one `<session>.jsonl` per session and, beside it,
/// `<session>/subagents/agent-*.jsonl` per background or Task-tool agent. Those files
/// carry the parent's `sessionId` and are read too: nothing else witnesses a session
/// whose main loop went quiet while an agent it dispatched is still working.
fn transcripts(root: &Path, since: Timestamp) -> Vec<(PathBuf, Timestamp)> {
    let mut paths = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return paths;
    };

    for project in projects.flatten() {
        let project = project.path();
        push_jsonl(&project, since, &mut paths);

        let Ok(entries) = std::fs::read_dir(&project) else {
            continue;
        };
        for entry in entries.flatten() {
            push_jsonl(&entry.path().join("subagents"), since, &mut paths);
        }
    }

    paths
}

fn push_jsonl(dir: &Path, since: Timestamp, paths: &mut Vec<(PathBuf, Timestamp)>) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    for file in files.flatten() {
        let path = file.path();
        let modified = modified_at(&file);
        if path.extension().is_some_and(|ext| ext == "jsonl") && modified >= since {
            paths.push((path, modified));
        }
    }
}

fn modified_at(entry: &DirEntry) -> Timestamp {
    entry
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |age| age.as_secs())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::core::session::SessionState;

    const FIRST: &str = concat!(
        r#"{"type":"user","sessionId":"s1","cwd":"/Users/x/specola","timestamp":"2026-07-23T10:00:00.000Z"}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-07-23T10:00:01.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":10}}}"#,
        "\n",
    );

    const SUBAGENT: &str = concat!(
        r#"{"type":"user","isSidechain":true,"agentId":"abc123","sessionId":"s1","cwd":"/Users/x/specola","gitBranch":"feat/timeline","timestamp":"2026-07-23T12:00:00.000Z"}"#,
        "\n",
        r#"{"type":"assistant","isSidechain":true,"agentId":"abc123","sessionId":"s1","timestamp":"2026-07-23T12:00:01.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":30}}}"#,
        "\n",
    );

    struct TempProjects(PathBuf);

    impl TempProjects {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("specola-scan-{name}-{nanos}"));
            std::fs::create_dir_all(root.join("a-project")).unwrap();
            Self(root)
        }

        fn transcript(&self) -> PathBuf {
            self.0.join("a-project").join("s1.jsonl")
        }

        fn append(&self, contents: &str) {
            self.append_to(self.transcript(), contents);
        }

        fn append_subagent(&self, contents: &str) {
            let dir = self.0.join("a-project").join("s1").join("subagents");
            std::fs::create_dir_all(&dir).unwrap();
            self.append_to(dir.join("agent-abc123.jsonl"), contents);
        }

        fn append_to(&self, path: PathBuf, contents: &str) {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            file.write_all(contents.as_bytes()).unwrap();
        }
    }

    impl Drop for TempProjects {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[test]
    fn finds_a_session_nested_under_a_project_directory() {
        let projects = TempProjects::new("find");
        projects.append(FIRST);

        let scan = Scanner::default().scan(&projects.0, 0, 0);

        assert_eq!(scan.sessions.len(), 1);
        assert_eq!(scan.sessions[0].id, "s1");
        assert_eq!(scan.sessions[0].cwd, "/Users/x/specola");
        assert_eq!(scan.usage["s1"].len(), 1);
    }

    #[test]
    fn a_subagent_transcript_counts_as_activity_of_the_session_that_spawned_it() {
        let projects = TempProjects::new("subagents");
        projects.append(FIRST);
        projects.append_subagent(SUBAGENT);

        let scan = Scanner::default().scan(&projects.0, 0, 0);

        assert_eq!(
            scan.usage["s1"].len(),
            2,
            "a background agent's spend is its parent session's spend"
        );
        assert_eq!(
            scan.sessions.iter().map(|s| s.last_active).max(),
            Some(1784808001),
            "the agent's records are the only witness that the session is still working"
        );
    }

    /// The reported bug: `/code-review` goes to the background, the main loop returns
    /// to the prompt, Claude Code's idle notification fires 60s later — and the panel
    /// says "waiting on you" while the agent is still churning.
    #[test]
    fn a_session_whose_background_agent_is_still_working_is_not_waiting_on_you() {
        let projects = TempProjects::new("still-working");
        projects.append(FIRST);
        projects.append_subagent(SUBAGENT);
        let notified_at = 1784808000;
        let waiting = Session {
            id: "s1".into(),
            cwd: "/Users/x/specola".into(),
            pid: Some(7),
            state: SessionState::WaitingOnYou,
            since: notified_at,
            last_active: notified_at,
            title: Some("Run the migration".into()),
            git_branch: None,
            terminal: None,
        };

        let scan = Scanner::default().scan(&projects.0, 0, 0);
        let merged = crate::core::store::merge(vec![waiting], scan.sessions);

        assert_eq!(merged.len(), 1, "the agent must not open a second row");
        assert_eq!(merged[0].state, SessionState::Working);
        assert_eq!(
            merged[0].title.as_deref(),
            Some("Run the migration"),
            "the row keeps its name"
        );
    }

    #[test]
    fn a_second_scan_reads_only_what_was_appended() {
        let projects = TempProjects::new("append");
        projects.append(FIRST);
        let mut scanner = Scanner::default();
        scanner.scan(&projects.0, 0, 0);

        projects.append(concat!(
            r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-07-23T11:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":20}}}"#,
            "\n",
        ));
        let scan = scanner.scan(&projects.0, 0, 0);

        assert_eq!(
            scan.usage["s1"].len(),
            2,
            "the first read must not be repeated"
        );
    }

    #[test]
    fn a_half_written_last_line_waits_for_its_newline() {
        let projects = TempProjects::new("torn");
        projects.append(FIRST);
        projects.append(r#"{"type":"assistant","sessionId":"s1","timesta"#);
        let mut scanner = Scanner::default();

        assert_eq!(scanner.scan(&projects.0, 0, 0).usage["s1"].len(), 1);

        projects.append(concat!(
            r#"mp":"2026-07-23T11:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":20}}}"#,
            "\n",
        ));

        assert_eq!(
            scanner.scan(&projects.0, 0, 0).usage["s1"].len(),
            2,
            "the line completes on the next poll"
        );
    }

    #[test]
    fn a_transcript_untouched_since_the_window_is_not_read() {
        let projects = TempProjects::new("stale");
        projects.append(FIRST);
        let far_future = u64::MAX / 2;

        assert!(Scanner::default()
            .scan(&projects.0, far_future, far_future)
            .sessions
            .is_empty());
    }

    #[test]
    fn a_transcript_older_than_the_session_floor_still_contributes_usage() {
        let projects = TempProjects::new("floors");
        projects.append(FIRST);
        let far_future = u64::MAX / 2;

        let scan = Scanner::default().scan(&projects.0, far_future, 0);

        assert!(scan.sessions.is_empty(), "too old for the session window");
        assert_eq!(scan.usage["s1"].len(), 1, "but its cost still counts");
    }

    #[test]
    fn a_missing_projects_directory_is_not_an_error() {
        let scan = Scanner::default().scan(Path::new("/nope/not/here"), 0, 0);

        assert!(scan.sessions.is_empty());
    }
}
