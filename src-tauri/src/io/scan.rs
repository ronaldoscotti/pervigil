use std::collections::{HashMap, HashSet};
use std::fs::{DirEntry, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::core::event::{SessionId, Timestamp};
use crate::core::pricing::UsageEntry;
use crate::core::session::Session;
use crate::core::store::Origin;

use super::transcript::Transcript;

/// How often the whole tree is walked, in polls. Between sweeps only files that have
/// been growing are re-checked, so discovery of a brand-new session or agent is what
/// this bounds — and only for its cost and title. State comes from the event log, which
/// is one file and is read every poll regardless.
const SWEEP_EVERY: u32 = 10;

/// Polls a file may go without growing before it is left to the sweep. Records arrive
/// every second or two while a session or an agent is working, but a slow tool call is
/// quiet for longer, and demoting a live file would delay the appended bytes.
const HOT_POLLS: u32 = 30;

#[derive(Default)]
struct Cached {
    consumed: u64,
    /// Last mtime we saw. Between sweeps this stands in for a `stat` we deliberately
    /// do not make.
    mtime: Timestamp,
    /// Consecutive polls this file did not grow.
    quiet: u32,
    transcript: Transcript,
}

impl Cached {
    /// Usage older than the widest window a caller can ask for is dead weight: it is
    /// re-cloned into every snapshot, and the panel polls every second for as long as
    /// the app runs. The entries arrive in time order, so the common case is one
    /// comparison.
    fn forget_before(&mut self, floor: Timestamp) {
        if self
            .transcript
            .usage
            .first()
            .is_some_and(|entry| entry.at < floor)
        {
            self.transcript.usage.retain(|entry| entry.at >= floor);
        }
    }

    /// Reads only the bytes appended since the last poll, and only up to the last
    /// complete line — a transcript is often mid-write when we look at it. Tracks
    /// whether the file is still growing, which is what keeps it off the sweep.
    fn absorb_appended(&mut self, path: &Path) {
        self.quiet = self.quiet.saturating_add(1);
        let Ok(mut file) = File::open(path) else {
            return;
        };
        let (len, mtime) = match file.metadata() {
            Ok(meta) => (meta.len(), modified_from(meta)),
            Err(_) => (0, 0),
        };
        self.mtime = self.mtime.max(mtime);
        if len < self.consumed {
            let kept = std::mem::take(&mut self.quiet);
            *self = Cached::default();
            self.quiet = kept;
            self.mtime = mtime;
        }
        if len == self.consumed || file.seek(SeekFrom::Start(self.consumed)).is_err() {
            return;
        }
        self.quiet = 0;

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
    /// Rows read from background-agent files, under the same session ids as
    /// `sessions`. Kept apart because they are not evidence about you — see
    /// [`crate::core::store::merge`].
    pub agents: Vec<Session>,
    /// Every session's spend, agents included: an agent bills to the session that
    /// spawned it.
    pub usage: HashMap<SessionId, Vec<UsageEntry>>,
    /// Record times, each tagged with the file it came from: what may end a wait
    /// depends on that — see [`crate::core::store::timeline`].
    pub activity: Vec<(SessionId, Timestamp, Origin)>,
}

/// Incremental reader over `~/.claude/projects/**/*.jsonl`. Those files are
/// append-only and run to gigabytes, so re-reading them on every poll is not an
/// option: each file is remembered by how much of it we have already consumed.
#[derive(Default)]
pub struct Scanner {
    files: HashMap<PathBuf, Cached>,
    polls: u32,
}

impl Scanner {
    /// Two floors: between them a transcript is priced but not listed. Files
    /// untouched since the lower one are skipped — append-only, so they cannot hold
    /// anything in either window.
    pub fn scan(&mut self, root: &Path, sessions_since: Timestamp, usage_since: Timestamp) -> Scan {
        let floor = usage_since.min(sessions_since);
        // A `stat` per candidate file per poll was the whole cost of this walk — 14ms of
        // 15ms over 740 files here, and that count grows with every session ever created.
        // Between sweeps the files that are actually being written are re-read and the
        // rest are served from cache, which is what they would have returned anyway.
        let sweep = self.polls.is_multiple_of(SWEEP_EVERY);
        self.polls = self.polls.wrapping_add(1);
        let candidates: Vec<(PathBuf, Timestamp)> = if sweep {
            transcripts(root, floor).into_iter().collect()
        } else {
            self.files
                .iter()
                .map(|(path, cached)| (path.clone(), cached.mtime))
                .collect()
        };

        let mut agents = Vec::new();
        let mut usage: HashMap<SessionId, Vec<UsageEntry>> = HashMap::new();
        let mut activity = Vec::new();
        // Every session's own row, whether or not its transcript is the file that moved,
        // and the ids something moved for.
        let mut rows: HashMap<SessionId, Session> = HashMap::new();
        let mut listed: HashSet<SessionId> = HashSet::new();

        for (path, modified) in candidates {
            let by_agent = is_agent_file(&path);
            let cached = self.files.entry(path.clone()).or_default();
            if sweep || cached.quiet < HOT_POLLS {
                cached.absorb_appended(&path);
            }
            cached.forget_before(floor);
            let modified = modified.max(cached.mtime);

            let Some(mut session) = cached.transcript.session() else {
                continue;
            };
            usage
                .entry(session.id.clone())
                .or_default()
                .extend(cached.transcript.usage.iter().cloned());
            let origin = if by_agent {
                Origin::Agent
            } else {
                Origin::Main
            };
            activity.extend(
                cached
                    .transcript
                    .usage
                    .iter()
                    .map(|entry| (session.id.clone(), entry.at, origin)),
            );

            let inside = modified >= sessions_since;
            if inside {
                listed.insert(session.id.clone());
            }
            if by_agent {
                // An agent file describes the agent, not the session: its title would be
                // the branch, and its cwd is often a subdirectory the agent was pointed
                // at — 33 of 200 files on this machine — which `project()` would turn
                // into a project that does not exist. Both belong to the session.
                session.title = None;
                session.cwd = String::new();
                if inside {
                    agents.push(session);
                }
            } else {
                rows.insert(session.id.clone(), session);
            }
        }

        // A session is listed when anything of it moved inside the window — its own
        // transcript or an agent's — and the row always comes from its own transcript,
        // which `transcripts` reads for exactly that reason even when only an agent
        // moved. Failing that the hook log supplies the row, or the session is dropped
        // as context-less rather than shown under an invented project.
        let sessions: Vec<Session> = listed.iter().filter_map(|id| rows.remove(id)).collect();

        Scan {
            sessions,
            agents,
            usage,
            activity,
        }
    }
}

/// A project holds one `<session>.jsonl` per session and, beside it,
/// `<session>/subagents/agent-*.jsonl` per background or Task-tool agent. Those files
/// carry the parent's `sessionId` and are read too: nothing else witnesses a session
/// whose main loop went quiet while an agent it dispatched is still working.
fn transcripts(root: &Path, since: Timestamp) -> HashMap<PathBuf, Timestamp> {
    let mut paths = HashMap::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return paths;
    };

    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                paths.extend(jsonl(&entry, since));
                continue;
            }
            let session = entry.path();
            let before = paths.len();
            push_jsonl(&session.join("subagents"), since, &mut paths);
            if paths.len() > before {
                // An agent moved, so the session is live whatever its own transcript's
                // mtime says — and that transcript is the only file that knows the row's
                // name and cwd. A long agent run spanning midnight would otherwise leave
                // a session with no row at all. Keyed by path, so this cannot duplicate
                // a file the walk already found.
                let own = session.with_extension("jsonl");
                if let Ok(meta) = std::fs::metadata(&own) {
                    paths.insert(own, modified_from(meta));
                }
            }
        }
    }

    paths
}

/// Background-agent transcripts live in a `subagents` directory and nothing else does.
/// Read from the path, not from the records: an `isSidechain` line has appeared inside
/// a main transcript before, and one such line must not reclassify the whole file.
fn is_agent_file(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|dir| dir == "subagents")
}

fn push_jsonl(dir: &Path, since: Timestamp, paths: &mut HashMap<PathBuf, Timestamp>) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    paths.extend(files.flatten().filter_map(|file| jsonl(&file, since)));
}

fn jsonl(entry: &DirEntry, since: Timestamp) -> Option<(PathBuf, Timestamp)> {
    let path = entry.path();
    let modified = modified_at(entry);
    (path.extension().is_some_and(|ext| ext == "jsonl") && modified >= since)
        .then_some((path, modified))
}

fn modified_at(entry: &DirEntry) -> Timestamp {
    entry.metadata().map(modified_from).unwrap_or(0)
}

fn modified_from(meta: std::fs::Metadata) -> Timestamp {
    meta.modified()
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

    const SECOND: &str = concat!(
        r#"{"type":"user","sessionId":"s2","cwd":"/Users/x/other","timestamp":"2026-07-23T10:00:00.000Z"}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"s2","timestamp":"2026-07-23T10:00:02.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":5}}}"#,
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

        /// A temp file is always modified "now", so the mtime gate can only be
        /// exercised by backdating it on purpose.
        fn backdate(&self, path: PathBuf, at: Timestamp) {
            let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
            file.set_times(
                std::fs::FileTimes::new()
                    .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(at)),
            )
            .unwrap();
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
            scan.agents.iter().map(|s| s.last_active).max(),
            Some(1784808001),
            "the agent's records are the only witness that the session is still working"
        );
    }

    /// A permission prompt is answered in the main loop, so only the main transcript
    /// can prove it was. An agent runs whether you are at the keyboard or not, and
    /// letting its records clear the wait hides a real block — the one thing the panel
    /// exists to show. Part B of the spec relaxes this for the idle nudge, which is a
    /// different notification, not different evidence.
    #[test]
    fn an_agents_records_never_clear_a_wait() {
        let projects = TempProjects::new("still-blocked");
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
        let merged =
            crate::core::store::merge(vec![waiting], scan.sessions, scan.agents, 1784808100);

        assert_eq!(merged.len(), 1, "the agent must not open a second row");
        assert_eq!(merged[0].state, SessionState::WaitingOnYou);
        assert_eq!(
            merged[0].last_active, 1784808001,
            "but its recency is still the agent's"
        );
        assert_eq!(
            merged[0].title.as_deref(),
            Some("Run the migration"),
            "the row keeps its name"
        );
    }

    #[test]
    fn an_agents_file_yields_a_nameless_row_and_tagged_activity() {
        let projects = TempProjects::new("origin");
        projects.append(FIRST);
        projects.append_subagent(SUBAGENT);

        let scan = Scanner::default().scan(&projects.0, 0, 0);

        assert_eq!(scan.sessions.len(), 1, "one row from the main transcript");
        assert_eq!(scan.sessions[0].last_active, 1784800801);
        assert_eq!(scan.agents.len(), 1, "and one from the agent's file");
        assert_eq!(
            scan.agents[0].title, None,
            "an agent file offers only the branch, which must not name the row"
        );
        assert_eq!(
            scan.agents[0].cwd, "",
            "nor its own cwd, which is often a subdirectory the agent was pointed at"
        );
        // read_dir has no order and the lane sorts its own ticks, so only the contents
        // are asserted.
        let mut activity = scan.activity.clone();
        activity.sort_by_key(|(_, at, _)| *at);
        assert_eq!(
            activity,
            vec![
                ("s1".to_string(), 1784800801, Origin::Main),
                ("s1".to_string(), 1784808001, Origin::Agent)
            ],
            "the lane is told which file each record came from"
        );
        assert_eq!(scan.usage["s1"].len(), 2, "cost still counts both");
    }

    /// The mtime gate skips a file that cannot hold anything in the window — but a
    /// session whose agent is the only thing writing still needs its own transcript,
    /// because that is the only file that knows the row's name and cwd.
    #[test]
    fn a_session_known_only_through_its_agent_still_gets_its_own_row() {
        let projects = TempProjects::new("agent-only");
        projects.append(FIRST);
        projects.append(concat!(
            r#"{"type":"last-prompt","sessionId":"s1","lastPrompt":"run the migration"}"#,
            "\n"
        ));
        projects.append_subagent(SUBAGENT);
        // Only the agent's file is inside the window: the session's own transcript has
        // not been touched since before the floor — a long agent run spanning midnight.
        let floor = 1784808000;
        projects.backdate(projects.transcript(), floor - 60 * 60);

        // Both floors: below either of them the walk would not open the file at all.
        let scan = Scanner::default().scan(&projects.0, floor, floor);

        assert_eq!(scan.sessions.len(), 1, "the session is listed");
        assert_eq!(
            scan.sessions[0].title.as_deref(),
            Some("run the migration"),
            "named by its own transcript, not by the agent's file"
        );
        assert_eq!(scan.sessions[0].cwd, "/Users/x/specola");
    }

    #[test]
    fn usage_older_than_the_floor_is_not_kept_or_re_cloned() {
        let projects = TempProjects::new("forget");
        projects.append(FIRST);
        let mut scanner = Scanner::default();

        assert_eq!(scanner.scan(&projects.0, 0, 0).usage["s1"].len(), 1);

        // A later poll whose floors are both past that entry must not carry it around:
        // the panel polls every second for as long as the app runs.
        let scan = scanner.scan(&projects.0, 1784800802, 1784800802);

        assert!(
            scan.usage
                .get("s1")
                .is_none_or(|entries| entries.is_empty()),
            "stale usage is dropped, not re-cloned every poll"
        );
        assert!(scan.activity.is_empty());
    }

    /// The panel polls every second and the tree holds every session ever created, so
    /// the walk is a sweep, not a per-poll cost. What that buys is paid for here: a file
    /// nobody was already reading is found on the next sweep rather than instantly.
    #[test]
    fn a_file_that_appears_between_sweeps_is_found_on_the_next_one() {
        let projects = TempProjects::new("sweep");
        projects.append(FIRST);
        let mut scanner = Scanner::default();

        assert_eq!(
            scanner.scan(&projects.0, 0, 0).usage.len(),
            1,
            "first poll sweeps"
        );

        projects.append_to(projects.0.join("a-project").join("s2.jsonl"), SECOND);

        for poll in 1..SWEEP_EVERY {
            assert_eq!(
                scanner.scan(&projects.0, 0, 0).usage.len(),
                1,
                "poll {poll} serves the files it already knows"
            );
        }

        assert_eq!(
            scanner.scan(&projects.0, 0, 0).usage.len(),
            2,
            "and the sweep picks up the new one"
        );
    }

    /// A file already being written is re-read every poll, sweep or not — the whole
    /// point of the tiering is that live files never wait.
    #[test]
    fn a_growing_file_is_read_on_every_poll() {
        let projects = TempProjects::new("hot");
        projects.append(FIRST);
        let mut scanner = Scanner::default();
        scanner.scan(&projects.0, 0, 0);

        for poll in 1..SWEEP_EVERY {
            projects.append(concat!(
                r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-07-23T11:00:00.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":1}}}"#,
                "\n",
            ));
            assert_eq!(
                scanner.scan(&projects.0, 0, 0).usage["s1"].len(),
                1 + poll as usize,
                "poll {poll} must not wait for a sweep"
            );
        }
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
