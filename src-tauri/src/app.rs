use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::core::event::{parse_log, SessionId, Timestamp};
use crate::core::pricing::{self, PriceTable, UsageEntry};
use crate::core::prune::prune;
use crate::core::session::{Session, SessionState};
use crate::core::store::{self, Segment};
use crate::core::terminal::Terminal;
use crate::io;
use crate::io::scan::Scanner;
use crate::platform::focuser::{self, Caps, Reach, Strategy, WindowFocuser};
use crate::platform::liveness::{retain_live, SystemProcesses};

const LOG: &str = ".pervigil/events.jsonl";
const PROJECTS: &str = ".claude/projects";
const CONFIG: &str = ".pervigil/config.json";
const SETTINGS: &str = ".claude/settings.json";

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
    /// What a click will do — "Jump to pane", "Copy resume command", … — so the row's
    /// tooltip is honest before it's clicked.
    pub focus: String,
    /// User-pinned: keeps the project at the top and marks the row's pin control.
    pub pinned: bool,
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
    /// Current settings the panel renders: the notifications toggle, and the projects
    /// the user hid (so they can be restored even with no live session).
    pub notifications: bool,
    pub hidden: Vec<String>,
    /// True once all four hooks are wired in `~/.claude/settings.json`. When false the
    /// panel shows the install card; pervigil never writes the file itself (spec item 12).
    pub hooks_installed: bool,
    /// The block to paste, with the bundled shim's real path baked in.
    pub hook_snippet: String,
}

/// What a click needs to reach a session — kept from the last snapshot so a click is
/// an O(1) lookup, never a re-scan of the log and the transcript tree.
#[derive(Clone)]
struct Target {
    cwd: String,
    terminal: Option<Terminal>,
}

/// The result of a click, for the UI's toast. `resume` is present whenever the user
/// may need to paste the command themselves — after a copy, or after a raise failed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusOutcome {
    pub raised: bool,
    pub label: String,
    pub resume: Option<String>,
    pub error: Option<String>,
}

/// A native notification to fire — computed in `snapshot`, drained and shown by the
/// command wrapper so the pure pipeline stays free of Tauri.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

pub struct App {
    home: PathBuf,
    prices: PriceTable,
    scanner: Mutex<Scanner>,
    caps: Caps,
    focuser: Box<dyn WindowFocuser + Send + Sync>,
    targets: Mutex<HashMap<String, Target>>,
    config: Mutex<Config>,
    /// Last seen state per session, for notification transitions. `None` until the
    /// first snapshot, which primes silently — the panel must not shout on launch
    /// about sessions that were already waiting.
    seen: Mutex<Option<HashMap<SessionId, SessionState>>>,
    pending: Mutex<Vec<Notice>>,
    /// Absolute path to the bundled `record` shim, baked into the install snippet.
    record_path: String,
}

impl App {
    pub fn new() -> Self {
        let home = io::home().unwrap_or_default();
        prune_log(&home);
        let config = Config::load(&home.join(CONFIG));
        Self {
            config: Mutex::new(config),
            prices: pricing::shipped(),
            scanner: Mutex::new(Scanner::default()),
            caps: Caps::detect(),
            focuser: focuser::platform(),
            targets: Mutex::new(HashMap::new()),
            seen: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            record_path: record_path(),
            home,
        }
    }

    /// Mutate the config, persist it, and let the next snapshot reflect it.
    fn update(&self, change: impl FnOnce(&mut Config)) {
        let mut config = self.config.lock().expect("config lock");
        change(&mut config);
        let _ = config.save(&self.home.join(CONFIG));
    }

    pub fn set_notifications(&self, on: bool) {
        self.update(|config| config.notifications = on);
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) {
        self.update(|config| {
            if pinned {
                config.pinned.insert(id.to_string());
            } else {
                config.pinned.remove(id);
            }
        });
    }

    pub fn set_project_hidden(&self, project: &str, hidden: bool) {
        self.update(|config| {
            if hidden {
                config.hidden_projects.insert(project.to_string());
            } else {
                config.hidden_projects.remove(project);
            }
        });
    }

    /// Dismiss hides the session until it next acts (a later event un-hides it via
    /// `fold`). Anchored at `at`, which the command passes as now.
    pub fn dismiss(&self, id: &str, at: Timestamp) {
        self.update(|config| {
            config.dismissed.insert(id.to_string(), at);
        });
    }

    /// The notices the last snapshot produced, cleared as they're taken.
    pub fn take_pending(&self) -> Vec<Notice> {
        std::mem::take(&mut self.pending.lock().expect("pending lock"))
    }

    /// Jump to a session's window, tab, or pane — or, at the floor, copy its resume
    /// command. Reads the target the last snapshot recorded; an id the panel never
    /// showed still degrades to a copyable resume command rather than erroring.
    pub fn focus(&self, id: &str) -> FocusOutcome {
        let target = self.targets.lock().expect("targets lock").get(id).cloned();
        let cwd = target.as_ref().map(|t| t.cwd.as_str()).unwrap_or("");
        let terminal = target.as_ref().and_then(|t| t.terminal.as_ref());

        let strategy = focuser::select(terminal, cwd, id, self.caps);
        let resume = match &strategy {
            Strategy::Clipboard { resume } => resume.clone(),
            _ => format!("claude --resume {id}"),
        };
        focus_with(self.focuser.as_ref(), strategy, resume)
    }

    /// Queue a notice for each session that just entered `WaitingOnYou`. The first
    /// snapshot only primes the baseline — it never fires, so launching the panel
    /// doesn't alert on sessions that were already waiting. Off entirely when the
    /// user disabled notifications, but the baseline still advances so re-enabling
    /// doesn't replay history.
    fn notify(&self, config: &Config, sessions: &[Session]) {
        let mut seen = self.seen.lock().expect("seen lock");
        let notices = notices(&mut seen, config, sessions);
        self.pending.lock().expect("pending lock").extend(notices);
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

        let config = self.config.lock().expect("config lock").clone();
        let prefs = config.view_prefs();

        let mut sessions = store::merge(store::fold(&events, to, &prefs), scan.sessions);
        retain_live(&mut sessions, &SystemProcesses);
        store::drop_dismissed(&mut sessions, &prefs);
        sessions.retain(|session| config.shows(&project(&session.cwd)));
        store::sort(&mut sessions, &prefs);

        self.notify(&config, &sessions);

        // The shim caches each session's terminal on every hook, so even a session
        // that never fired SessionStart (no event-borne terminal) is focusable.
        let terminals = io::terminals::read_all(&self.home);
        let terminal_of = |session: &Session| {
            terminals
                .get(&session.id)
                .cloned()
                .or_else(|| session.terminal.clone())
        };

        *self.targets.lock().expect("targets lock") = sessions
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    Target {
                        cwd: s.cwd.clone(),
                        terminal: terminal_of(s),
                    },
                )
            })
            .collect();

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
                // Scoped to the same window as the footer — a row must not show a
                // session's whole lifetime cost against a "last 4 hours" filter.
                cost: scan.usage.get(&session.id).and_then(|entries| {
                    let windowed: Vec<UsageEntry> = entries
                        .iter()
                        .filter(|entry| entry.at >= from && entry.at <= to)
                        .cloned()
                        .collect();
                    pricing::total(&self.prices, &windowed)
                }),
                focus: focuser::select(
                    terminal_of(session).as_ref(),
                    &session.cwd,
                    &session.id,
                    self.caps,
                )
                .label()
                .to_string(),
                pinned: config.pinned.contains(&session.id),
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
            notifications: config.notifications,
            hidden: config.hidden_projects.iter().cloned().collect(),
            hooks_installed: io::hooks::detect(
                &std::fs::read_to_string(self.home.join(SETTINGS)).unwrap_or_default(),
            )
            .all_installed,
            hook_snippet: io::hooks::snippet(&self.record_path),
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

/// The bundled `record` shim sits beside the app binary. Falls back to the bare name
/// (assuming `PATH`) if the exe path can't be resolved — the snippet still reads
/// sensibly and the user can correct it.
fn record_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("record")))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "record".to_string())
}

/// Advance the seen-state baseline and return a notice per session that just entered
/// waiting. The first observation (`seen` is `None`) only primes — it never fires, so
/// launch is silent. The baseline advances even when notifications are off, so
/// re-enabling them doesn't replay a backlog.
fn notices(
    seen: &mut Option<HashMap<SessionId, SessionState>>,
    config: &Config,
    sessions: &[Session],
) -> Vec<Notice> {
    let notices = match (seen.as_ref(), config.notifications) {
        (Some(previous), true) => store::newly_waiting(previous, sessions)
            .into_iter()
            .map(|session| Notice {
                title: format!("{} — waiting on you", project(&session.cwd)),
                body: name(session),
            })
            .collect(),
        _ => Vec::new(),
    };
    *seen = Some(store::states(sessions));
    notices
}

/// Try the chosen tier; if a precise raise fails (a stale pane, an unreachable app),
/// fall to the clipboard floor so a click still does *something* real — degrade,
/// never fake. Only when even the floor fails is an error surfaced.
fn focus_with(
    focuser: &(dyn WindowFocuser + Send + Sync),
    strategy: Strategy,
    resume: String,
) -> FocusOutcome {
    match focuser.focus(&strategy) {
        Ok(reach) => outcome(Ok(reach), resume, strategy.label()),
        Err(error) => match strategy {
            Strategy::Clipboard { .. } => outcome(Err(error), resume, strategy.label()),
            _ => {
                let floor = Strategy::Clipboard {
                    resume: resume.clone(),
                };
                outcome(focuser.focus(&floor), resume, floor.label())
            }
        },
    }
}

/// Shape a focus attempt for the UI. A raise needs no follow-up; a copy or a failure
/// both hand back the resume command so the user is never stranded.
fn outcome(result: std::io::Result<Reach>, resume: String, label: &str) -> FocusOutcome {
    match result {
        Ok(Reach::Raised) => FocusOutcome {
            raised: true,
            label: label.into(),
            resume: None,
            error: None,
        },
        Ok(Reach::Copied) => FocusOutcome {
            raised: false,
            label: label.into(),
            resume: Some(resume),
            error: None,
        },
        Err(error) => FocusOutcome {
            raised: false,
            label: label.into(),
            resume: Some(resume),
            error: Some(error.to_string()),
        },
    }
}

#[tauri::command]
pub fn snapshot(span: Span, app: tauri::State<'_, App>, handle: tauri::AppHandle) -> Snapshot {
    let snapshot = app.snapshot(span, Local::now());
    badge(&handle, snapshot.waiting);
    fire(&handle, app.take_pending());
    snapshot
}

#[tauri::command]
pub fn focus(id: String, app: tauri::State<'_, App>) -> FocusOutcome {
    app.focus(&id)
}

#[tauri::command]
pub fn set_notifications(on: bool, app: tauri::State<'_, App>) {
    app.set_notifications(on);
}

#[tauri::command]
pub fn set_pinned(id: String, pinned: bool, app: tauri::State<'_, App>) {
    app.set_pinned(&id, pinned);
}

#[tauri::command]
pub fn set_project_hidden(project: String, hidden: bool, app: tauri::State<'_, App>) {
    app.set_project_hidden(&project, hidden);
}

#[tauri::command]
pub fn dismiss(id: String, app: tauri::State<'_, App>) {
    app.dismiss(&id, Local::now().timestamp().max(0) as Timestamp);
}

/// Open `~/.claude/settings.json` in the user's default editor for JSON — pervigil
/// never edits it, so the fastest honest help is to take you there.
#[tauri::command]
pub fn open_settings(app: tauri::State<'_, App>) {
    let path = app.home.join(SETTINGS);
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}

/// Toggle the panel's always-on-top. A watch instrument you keep in view, but yours
/// to unpin when it's in the way.
#[tauri::command]
pub fn set_window_pinned(pinned: bool, window: tauri::WebviewWindow) {
    let _ = window.set_always_on_top(pinned);
}

/// The tray title is the count of sessions blocked on you — the product thesis, at
/// menu-bar size. Blank at zero: nothing is waiting, so nothing shouts.
fn badge(handle: &tauri::AppHandle, waiting: usize) {
    if let Some(tray) = handle.tray_by_id(crate::TRAY_ID) {
        let _ = tray.set_title((waiting > 0).then(|| waiting.to_string()));
    }
}

/// Show the queued notices. Best-effort: a notification that won't show must never
/// disturb the panel.
fn fire(handle: &tauri::AppHandle, notices: Vec<Notice>) {
    use tauri_plugin_notification::NotificationExt;

    for notice in notices {
        let _ = handle
            .notification()
            .builder()
            .title(notice.title)
            .body(notice.body)
            .show();
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

    fn waiting_session(id: &str, project: &str) -> Session {
        Session {
            id: id.into(),
            cwd: format!("/Users/x/{project}"),
            pid: Some(1),
            state: SessionState::WaitingOnYou,
            since: 0,
            last_active: 0,
            title: Some("do the thing".into()),
            git_branch: None,
            terminal: None,
        }
    }

    #[test]
    fn the_first_snapshot_primes_silently() {
        let mut seen = None;

        let fired = notices(
            &mut seen,
            &Config::default(),
            &[waiting_session("s1", "proj")],
        );

        assert!(
            fired.is_empty(),
            "launch must not shout about existing waits"
        );
        assert!(seen.is_some(), "but the baseline is now set");
    }

    #[test]
    fn entering_waiting_after_priming_fires_a_notice() {
        let mut seen = Some(HashMap::from([("s1".to_string(), SessionState::Working)]));

        let fired = notices(
            &mut seen,
            &Config::default(),
            &[waiting_session("s1", "proj")],
        );

        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].title, "proj — waiting on you");
        assert_eq!(fired[0].body, "do the thing");
    }

    #[test]
    fn notifications_off_fires_nothing_but_still_advances_the_baseline() {
        let mut seen = Some(HashMap::from([("s1".to_string(), SessionState::Working)]));
        let off = Config {
            notifications: false,
            ..Config::default()
        };

        let fired = notices(&mut seen, &off, &[waiting_session("s1", "proj")]);

        assert!(fired.is_empty());
        assert_eq!(
            seen.unwrap().get("s1"),
            Some(&SessionState::WaitingOnYou),
            "re-enabling must not replay this as new"
        );
    }

    /// Fails the precise tiers; only the clipboard floor works — like a Mac where a
    /// stale tmux pane id can't be selected but `pbcopy` still runs.
    struct OnlyClipboard;
    impl WindowFocuser for OnlyClipboard {
        fn focus(&self, strategy: &Strategy) -> std::io::Result<Reach> {
            match strategy {
                Strategy::Clipboard { .. } => Ok(Reach::Copied),
                _ => Err(std::io::Error::other("stale window")),
            }
        }
    }

    /// Nothing works — an unsupported platform.
    struct FocusNothing;
    impl WindowFocuser for FocusNothing {
        fn focus(&self, _strategy: &Strategy) -> std::io::Result<Reach> {
            Err(std::io::Error::other("unsupported"))
        }
    }

    #[test]
    fn a_failed_raise_degrades_to_actually_copying_the_resume_command() {
        let strategy = Strategy::Tmux { pane: "%3".into() };

        let result = focus_with(&OnlyClipboard, strategy, "claude --resume s1".into());

        assert!(!result.raised, "the pane was not raised");
        assert_eq!(
            result.error, None,
            "the copy fallback succeeded, so no error"
        );
        assert_eq!(
            result.resume.as_deref(),
            Some("claude --resume s1"),
            "the copied command is reported"
        );
        assert_eq!(result.label, "Copy resume command");
    }

    #[test]
    fn when_even_the_clipboard_fails_the_error_is_surfaced_not_a_false_copy() {
        let strategy = Strategy::VsCode { path: "/p".into() };

        let result = focus_with(&FocusNothing, strategy, "claude --resume s1".into());

        assert!(!result.raised);
        assert!(
            result.error.is_some(),
            "a real failure must not read as a copy"
        );
        assert_eq!(result.resume.as_deref(), Some("claude --resume s1"));
    }

    #[test]
    fn a_raised_window_reports_no_resume_to_copy() {
        let result = outcome(
            Ok(Reach::Raised),
            "claude --resume s1".into(),
            "Jump to pane",
        );

        assert!(result.raised);
        assert_eq!(result.resume, None);
        assert_eq!(result.error, None);
    }

    #[test]
    fn a_copied_fallback_hands_back_the_resume_command() {
        let result = outcome(
            Ok(Reach::Copied),
            "claude --resume s1".into(),
            "Copy resume command",
        );

        assert!(!result.raised);
        assert_eq!(result.resume.as_deref(), Some("claude --resume s1"));
        assert_eq!(result.error, None);
    }

    #[test]
    fn a_failed_focus_still_offers_the_resume_command_and_the_reason() {
        let result = outcome(
            Err(std::io::Error::other("boom")),
            "claude --resume s1".into(),
            "Focus tab",
        );

        assert!(!result.raised);
        assert_eq!(result.resume.as_deref(), Some("claude --resume s1"));
        assert_eq!(result.error.as_deref(), Some("boom"));
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
            terminal: None,
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
