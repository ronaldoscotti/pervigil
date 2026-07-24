use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::core::event::{parse_log, Timestamp};
use crate::core::pricing::{self, PriceTable, UsageEntry};
use crate::core::prune::prune;
use crate::core::session::{Session, SessionState, ViewPrefs};
use crate::core::store::{self, Segment};
use crate::io;
use crate::io::scan::Scanner;
use crate::platform::liveness::{retain_live, SystemProcesses};

const LOG: &str = ".pervigil/events.jsonl";
const PROJECTS: &str = ".claude/projects";

/// The one filter in the UI. It scopes the lane, the cost readout, and how far back
/// transcripts are read — the panel shows the window you picked, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Span {
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "week")]
    Week,
}

/// `[from, now]` in epoch seconds. Today and this week are local calendar
/// boundaries; a rolling 24 hours would not be "today".
pub fn bounds(span: Span, now: DateTime<Local>) -> (Timestamp, Timestamp) {
    let from = match span {
        Span::FourHours => now - Duration::hours(4),
        Span::Today => start_of_day(now),
        Span::Week => {
            start_of_day(now - Duration::days(now.weekday().num_days_from_monday() as i64))
        }
    };

    (
        from.timestamp().max(0) as Timestamp,
        now.timestamp().max(0) as Timestamp,
    )
}

/// Local midnight — or, on a day whose clocks skip it, the first instant that exists.
fn start_of_day(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    (0..24)
        .filter_map(|hour| date.and_hms_opt(hour, 0, 0))
        .find_map(|at| now.timezone().from_local_datetime(&at).earliest())
        .unwrap_or(now)
}

/// One row of the panel. Everything the UI needs to draw it, and nothing it would
/// have to compute for itself.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub project: String,
    pub name: String,
    pub branch: Option<String>,
    pub state: SessionState,
    pub since: Timestamp,
    /// Live sessions in the same project. The branch chip and `×N` earn their space
    /// only above 1 — see `design/README.md`.
    pub siblings: usize,
    /// `None` when nothing in this session can be priced; the row shows `—`.
    pub cost: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub now: Timestamp,
    pub from: Timestamp,
    pub waiting: usize,
    pub sessions: Vec<SessionView>,
    pub segments: Vec<Segment>,
    pub waiting_share: f64,
    pub cost: f64,
    /// False when the event log is empty: state is transcript-derived and every row
    /// reads `idle`. The UI says so rather than implying the sessions are quiet.
    pub hooks: bool,
}

pub struct App {
    home: PathBuf,
    prices: PriceTable,
    scanner: Mutex<Scanner>,
    prefs: ViewPrefs,
}

impl App {
    pub fn new() -> Self {
        let home = io::home().unwrap_or_default();
        prune_log(&home);
        Self {
            home,
            prices: pricing::shipped(),
            scanner: Mutex::new(Scanner::default()),
            // ponytail: pin/dismiss land in M8 with the config file that persists them.
            prefs: ViewPrefs::default(),
        }
    }

    /// Both inputs, folded into one view. Hooks give state, transcripts give cost,
    /// names, and any session hooks never saw; either can be missing.
    pub fn snapshot(&self, span: Span, now: DateTime<Local>) -> Snapshot {
        let (from, to) = bounds(span, now);

        let log = std::fs::read_to_string(self.home.join(LOG)).unwrap_or_default();
        let (events, _skipped) = parse_log(&log);
        let events = prune(events, to);

        let scan = self
            .scanner
            .lock()
            .expect("scanner lock")
            .scan(&self.home.join(PROJECTS), from);

        let mut sessions = store::merge(store::fold(&events, to, &self.prefs), scan.sessions);
        retain_live(&mut sessions, &SystemProcesses);
        store::sort(&mut sessions, &self.prefs);

        let projects: Vec<String> = sessions.iter().map(|s| project(&s.cwd)).collect();
        let sessions: Vec<SessionView> = sessions
            .iter()
            .zip(&projects)
            .map(|(session, project)| SessionView {
                name: name(session),
                id: session.id.clone(),
                branch: session.git_branch.clone(),
                state: session.state,
                since: session.since,
                siblings: projects.iter().filter(|other| *other == project).count(),
                cost: scan
                    .usage
                    .get(&session.id)
                    .and_then(|entries| pricing::total(&self.prices, entries)),
                project: project.clone(),
            })
            .collect();

        let segments = store::timeline(&events, from, to);
        let spent: Vec<&UsageEntry> = scan.usage.values().flatten().collect();

        Snapshot {
            now: to,
            from,
            waiting: sessions
                .iter()
                .filter(|s| s.state == SessionState::WaitingOnYou)
                .count(),
            waiting_share: store::waiting_share(&segments),
            cost: spent
                .iter()
                .filter(|entry| entry.at >= from && entry.at <= to)
                .filter_map(|entry| pricing::cost(&self.prices, &entry.model, &entry.usage))
                .sum(),
            hooks: !events.is_empty(),
            sessions,
            segments,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// The log is append-only, so nothing else bounds its growth. Rewritten once, at
/// launch, and only when it actually shrinks.
fn prune_log(home: &Path) {
    let path = home.join(LOG);
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return;
    };
    let (events, _) = parse_log(&contents);
    let now = Local::now().timestamp().max(0) as Timestamp;
    let kept = prune(events, now);

    let lines: Vec<String> = kept
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .collect();
    if lines.len() == contents.lines().filter(|l| !l.trim().is_empty()).count() {
        return;
    }
    std::fs::write(&path, lines.join("\n") + "\n").ok();
}

/// The last tier of spec item 13: a session hooks saw but no transcript names still
/// gets something to be called.
fn name(session: &Session) -> String {
    session
        .title
        .clone()
        .unwrap_or_else(|| session.id.chars().take(8).collect())
}

fn project(cwd: &str) -> String {
    cwd.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(cwd)
        .to_string()
}

#[tauri::command]
pub fn snapshot(span: Span, app: tauri::State<'_, App>, handle: tauri::AppHandle) -> Snapshot {
    let snapshot = app.snapshot(span, Local::now());
    badge(&handle, snapshot.waiting);
    snapshot
}

/// The tray title is the count of sessions blocked on you — the product thesis, at
/// menu-bar size. Blank at zero: nothing is waiting, so nothing shouts.
fn badge(handle: &tauri::AppHandle, waiting: usize) {
    if let Some(tray) = handle.tray_by_id(crate::TRAY_ID) {
        let _ = tray.set_title((waiting > 0).then(|| waiting.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use chrono::Timelike;

    use super::*;

    fn now() -> DateTime<Local> {
        Local::now()
    }

    #[test]
    fn four_hours_ends_now_and_starts_four_hours_back() {
        let now = now();

        let (from, to) = bounds(Span::FourHours, now);

        assert_eq!(to, now.timestamp() as Timestamp);
        assert_eq!(to - from, 4 * 60 * 60);
    }

    #[test]
    fn today_starts_at_a_local_midnight_not_twenty_four_hours_ago() {
        let now = now();

        let (from, _) = bounds(Span::Today, now);

        let start = Local.timestamp_opt(from as i64, 0).unwrap();
        assert_eq!(start.date_naive(), now.date_naive());
        assert_eq!((start.hour(), start.minute(), start.second()), (0, 0, 0));
    }

    #[test]
    fn the_week_starts_on_a_monday_no_later_than_today() {
        let now = now();

        let (from, _) = bounds(Span::Week, now);

        let start = Local.timestamp_opt(from as i64, 0).unwrap();
        assert_eq!(start.weekday(), chrono::Weekday::Mon);
        assert!(start <= now);
        assert!(now - start < Duration::days(7));
    }

    #[test]
    fn a_session_no_transcript_names_falls_back_to_a_short_id() {
        let session = Session {
            id: "abcdef1234-5678".into(),
            cwd: "/p".into(),
            pid: Some(1),
            state: SessionState::Working,
            since: 0,
            last_active: 0,
            title: None,
            git_branch: None,
        };

        assert_eq!(name(&session), "abcdef12");
    }

    #[test]
    fn a_project_is_the_last_segment_of_its_path() {
        assert_eq!(project("/Users/x/work/pervigil"), "pervigil");
        assert_eq!(project("/Users/x/work/pervigil/"), "pervigil");
        assert_eq!(project(r"C:\Users\x\pervigil"), "pervigil");
        assert_eq!(project(""), "");
    }

    /// Reads the real `~/.claude` and `~/.pervigil`, so it only means anything on a
    /// machine that has them. Ignored by default; the QA entry point:
    /// `cargo test real_home -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_real_home_produces_a_snapshot() {
        let snapshot = App::new().snapshot(Span::FourHours, Local::now());

        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
        assert!(snapshot.now > snapshot.from);
    }
}
