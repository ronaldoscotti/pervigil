use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::config::Config;
use crate::core::event::{parse_log, SessionId, Timestamp};
use crate::core::notify::{name, notices, Notice};
use crate::core::pricing::{self, PriceTable, UsageEntry};
use crate::core::prune::prune;
use crate::core::session::{project, Session, SessionState};
use crate::core::span::{bounds, start_of_day, Span};
use crate::core::store::{self, Segment};
use crate::core::terminal::Terminal;
use crate::core::tray::{tray_view, TrayStrings, TrayView};
use crate::io;
use crate::io::scan::Scanner;
use crate::platform::focuser::{self, Caps, Reach, Strategy, WindowFocuser};
use crate::platform::liveness::{retain_live, SystemProcesses};

const LOG: &str = ".specola/events.jsonl";
const PROJECTS: &str = ".claude/projects";
const CONFIG: &str = ".specola/config.json";
const SETTINGS: &str = ".claude/settings.json";

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
    /// Blocked on you, but last active before the chosen span. The tray has no window
    /// and still counts these, so the panel says how many it is not showing rather
    /// than contradicting the badge.
    pub waiting_outside_window: usize,
    pub sessions: Vec<SessionView>,
    pub segments: Vec<Segment>,
    pub waiting_share: f64,
    pub cost: f64,
    /// Spend since local midnight, whatever span the panel is showing. The tray has
    /// no filter UI, so its summary has to mean the same thing under either clock.
    pub today_cost: f64,
    /// Total tokens processed in the window — input, output, and cache.
    pub tokens: u64,
    /// Current settings the panel renders: the notifications toggle, and the projects
    /// the user hid (so they can be restored even with no live session).
    pub notifications: bool,
    /// When true, dismiss marks a session read (demoted to idle) instead of hiding it.
    pub dismiss_read: bool,
    pub hidden: Vec<String>,
    /// True once all four hooks are wired in `~/.claude/settings.json`. When false the
    /// panel shows the install card; specola never writes the file itself (spec item 12).
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

/// # Lock ordering
///
/// Only `snapshot` takes more than one, in field order: `scanner` → `config` → `seen`
/// → `pending` → `targets` → `tray` → `strings`. New code that needs two follows the
/// same order or says why it cannot.
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
    /// The tray's words, pushed down by the frontend, which owns the ten locales.
    /// English until it does — see `core::tray::TrayStrings`.
    strings: Mutex<TrayStrings>,
    /// The last tray view computed, so the tray is applied from the same snapshot the
    /// panel saw rather than from a second pass over the log.
    tray: Mutex<TrayView>,
    /// Absolute path to the bundled `record` shim, baked into the install snippet.
    record_path: String,
}

impl App {
    pub fn new() -> Self {
        let home = io::home().unwrap_or_default();
        prune_log(&home);
        Self::at(home)
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn settings_path(&self) -> PathBuf {
        self.home.join(SETTINGS)
    }

    /// Rooted at a given home, and it leaves that home's log alone. Retention is
    /// launch housekeeping and belongs to [`App::new`]: it prunes against the wall
    /// clock, which would quietly rewrite any fixture written against a fixed one.
    pub fn at(home: PathBuf) -> Self {
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
            tray: Mutex::new(tray_view(&[], 0.0, &TrayStrings::default())),
            strings: Mutex::new(TrayStrings::default()),
            record_path: record_path(),
            home,
        }
    }

    /// Mutate the config, persist it, and let the next snapshot reflect it. The write
    /// error is returned so a setting the disk dropped can't read as saved.
    fn update(&self, change: impl FnOnce(&mut Config)) -> std::io::Result<()> {
        let mut config = self.config.lock().expect("config lock");
        change(&mut config);
        config.save(&self.home.join(CONFIG))
    }

    pub fn set_notifications(&self, on: bool) -> std::io::Result<()> {
        self.update(|config| config.notifications = on)
    }

    pub fn set_dismiss_read(&self, on: bool) -> std::io::Result<()> {
        self.update(|config| config.dismiss_read = on)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> std::io::Result<()> {
        self.update(|config| {
            if pinned {
                config.pinned.insert(id.to_string());
            } else {
                config.pinned.remove(id);
            }
        })
    }

    pub fn set_project_hidden(&self, project: &str, hidden: bool) -> std::io::Result<()> {
        self.update(|config| {
            if hidden {
                config.hidden_projects.insert(project.to_string());
            } else {
                config.hidden_projects.remove(project);
            }
        })
    }

    /// Dismiss hides the session until it next acts (a later event un-hides it via
    /// `fold`). Anchored at `at`, which the command passes as now.
    pub fn dismiss(&self, id: &str, at: Timestamp) -> std::io::Result<()> {
        self.update(|config| {
            config.dismissed.insert(id.to_string(), at);
        })
    }

    /// What the tray should currently show. Cheap: the work happened in `snapshot`.
    pub fn tray(&self) -> TrayView {
        self.tray.lock().expect("tray lock").clone()
    }

    pub fn words(&self) -> TrayStrings {
        self.strings.lock().expect("strings lock").clone()
    }

    /// Whether ambient alerts are on. The tray's clipboard fallback asks, because a
    /// user who silenced alerts still needs to know a click did something.
    pub fn notifications_on(&self) -> bool {
        self.config.lock().expect("config lock").notifications
    }

    pub fn set_strings(&self, words: TrayStrings) {
        *self.strings.lock().expect("strings lock") = words;
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
            _ => focuser::resume_command(cwd, id),
        };
        focus_with(self.focuser.as_ref(), strategy, resume)
    }

    /// Queue a notice for each session that just entered `WaitingOnYou`. The first
    /// snapshot only primes the baseline — it never fires, so launching the panel
    /// doesn't alert on sessions that were already waiting. Off entirely when the
    /// user disabled notifications, but the baseline still advances so re-enabling
    /// doesn't replay history.
    /// Both inputs, folded into one view. Hooks give state, transcripts give cost,
    /// names, and any session hooks never saw; either can be missing.
    pub fn snapshot(&self, span: Span, now: DateTime<Local>) -> Snapshot {
        let (from, to) = bounds(span, now);

        let log = std::fs::read_to_string(self.home.join(LOG)).unwrap_or_default();
        let (events, _skipped) = parse_log(&log);
        let events = prune(events, to);

        // Usage is read from midnight even when the panel asks for a narrower span:
        // a transcript quiet since this morning is still today's spend, and the
        // mtime gate would otherwise never open its file at all.
        let midnight = start_of_day(now).timestamp().max(0) as Timestamp;
        // The scanner is shared by every span and outlives all of them, so what it may
        // forget is bounded by the widest one — not by whichever span asked first.
        let keep_since = bounds(Span::Week, now).0;
        let scan = self.scanner.lock().expect("scanner lock").scan(
            &self.home.join(PROJECTS),
            from,
            midnight,
            keep_since,
        );
        let spent: Vec<&UsageEntry> = scan.usage.values().flatten().collect();

        let config = self.config.lock().expect("config lock").clone();
        let prefs = config.view_prefs();

        let mut sessions =
            store::merge(store::fold(&events, to, &prefs), scan.sessions, scan.agents);
        retain_live(&mut sessions, &SystemProcesses);
        let retired = store::superseded(&events);
        sessions.retain(|session| !retired.contains(&session.id));
        store::apply_dismissed(&mut sessions, &prefs);
        // Drop context-less ghosts: a hook fired (a Notification) but no cwd ever
        // arrived and no transcript backfilled one, so the row would be nameless.
        sessions.retain(|session| !session.cwd.is_empty());
        sessions.retain(|session| config.shows(&project(&session.cwd)));
        store::sort(&mut sessions, &prefs);

        let queued = notices(
            &mut self.seen.lock().expect("seen lock"),
            config.notifications,
            &sessions,
        );
        self.pending.lock().expect("pending lock").extend(queued);

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

        // Computed here, where the raw sessions are still in scope, and kept for the
        // tray to read. One pass serves both surfaces, and whichever clock produced
        // this snapshot, the tray and the panel agree because they saw the same one.
        let today_cost = pricing::cost_in_window(&self.prices, spent.iter().copied(), midnight, to);
        *self.tray.lock().expect("tray lock") = tray_view(
            &sessions,
            today_cost,
            &self.strings.lock().expect("strings lock"),
        );

        let waiting_live = sessions
            .iter()
            .filter(|s| s.state == SessionState::WaitingOnYou)
            .count();

        // The span scopes the panel's list and nothing above it. The tray and the
        // notification baseline answer "what is blocked on you", which has no window —
        // scoping them would blank the badge and replay notices on the next wider poll.
        store::retain_within(&mut sessions, from);

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

        // Main-transcript records are the only witness that a permission prompt was
        // answered — no hook fires when it is, and a background agent writes either way.
        let segments = store::timeline(&events, &scan.activity, from, to);

        Snapshot {
            now: to,
            from,
            waiting_outside_window: waiting_live.saturating_sub(
                sessions
                    .iter()
                    .filter(|s| s.state == SessionState::WaitingOnYou)
                    .count(),
            ),
            waiting: sessions
                .iter()
                .filter(|s| s.state == SessionState::WaitingOnYou)
                .count(),
            waiting_share: store::waiting_share(&segments),
            cost: pricing::cost_in_window(&self.prices, spent.iter().copied(), from, to),
            today_cost: pricing::cost_in_window(&self.prices, spent.iter().copied(), midnight, to),
            tokens: spent
                .iter()
                .filter(|entry| entry.at >= from && entry.at <= to)
                .map(|entry| {
                    let u = &entry.usage;
                    u.input + u.output + u.cache_read + u.cache_write_5m + u.cache_write_1h
                })
                .sum(),
            notifications: config.notifications,
            dismiss_read: config.dismiss_read,
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// The defect this pass exists for: a settings change the disk refused used to
    /// return nothing, so the UI accepted it and the next launch lost it. Here the
    /// home is a *file*, so `~/.specola/` can never be created under it.
    #[test]
    fn a_setting_the_disk_refuses_is_reported_not_swallowed() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("specola-app-{nanos}"));
        std::fs::write(&home, "not a directory").unwrap();

        let result = App::at(home.clone()).set_notifications(false);

        assert!(
            result.is_err(),
            "a setting that was not persisted must say so"
        );

        std::fs::remove_file(&home).ok();
    }

    /// Construction used to prune against the wall clock, so a fixture written
    /// against a fixed clock in the past emptied itself the day it aged past
    /// retention — `tests/golden.rs` was 25 days from breaking with no commit to
    /// blame. Retention is [`App::new`]'s job now, and this pins that.
    #[test]
    fn construction_leaves_an_old_log_alone() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!("specola-retention-{nanos}"));
        std::fs::create_dir_all(home.join(".specola")).unwrap();
        let now = Local::now().timestamp().max(0) as Timestamp;
        let ancient = now - crate::core::prune::RETENTION_SECS - 86_400;
        let log = home.join(".specola/events.jsonl");
        let written = format!(r#"{{"type":"Stop","id":"s","at":{ancient}}}"#) + "\n";
        std::fs::write(&log, &written).unwrap();

        let _app = App::at(home.clone());

        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            written,
            "constructing an app must not rewrite the caller's log"
        );

        std::fs::remove_dir_all(&home).ok();
    }

    /// Reads the real `~/.claude` and `~/.specola`, so it only means anything on a
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
