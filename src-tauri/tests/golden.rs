//! What the panel is handed, pinned to a file.
//!
//! Every other test asserts one rule. These assert the whole shape of a snapshot for a
//! handful of days worth having an opinion about, against a committed JSON file — so a
//! change nobody meant to make shows up as a diff instead of as something to notice by
//! looking at the app. The same files are the fixtures the frontend renders in
//! `src/render.test.ts`, which is the other half: this half says what the UI is given,
//! that half says what it draws.
//!
//! Regenerate after an intended change:
//!
//! ```sh
//! UPDATE_GOLDENS=1 cargo test --test golden
//! ```
//!
//! and read the diff before committing it. A golden nobody reads is worse than no test.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, SecondsFormat, TimeZone};
use specola_lib::app::App;
use specola_lib::core::span::Span;

/// A fixed clock, comfortably in the past so the fixtures' real file mtimes always clear
/// the scanner's window gate. Every timestamp below is an offset from it.
const NOW: i64 = 1_785_000_000;

fn at(offset: i64) -> i64 {
    NOW + offset
}

fn iso(epoch: i64) -> String {
    DateTime::from_timestamp(epoch, 0)
        .expect("fixture timestamp should be valid")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// A `$HOME` built for one scenario. Written per test rather than committed, because the
/// hook log has to carry this process's pid — liveness retires a session whose process
/// is gone, and a committed pid is always gone.
struct Home {
    root: PathBuf,
    events: Vec<String>,
}

impl Home {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("specola-golden-{name}-{nanos}"));
        std::fs::create_dir_all(root.join(".specola")).unwrap();
        Self {
            root,
            events: Vec::new(),
        }
    }

    fn started(mut self, id: &str, cwd: &str, offset: i64, source: &str) -> Self {
        let pid = std::process::id();
        self.events.push(format!(
            r#"{{"type":"SessionStart","id":"{id}","cwd":"{cwd}","pid":{pid},"at":{},"source":"{source}"}}"#,
            at(offset)
        ));
        self
    }

    fn prompted(mut self, id: &str, offset: i64) -> Self {
        self.events.push(format!(
            r#"{{"type":"UserPromptSubmit","id":"{id}","at":{}}}"#,
            at(offset)
        ));
        self
    }

    fn stopped(mut self, id: &str, offset: i64) -> Self {
        self.events.push(format!(
            r#"{{"type":"Stop","id":"{id}","at":{}}}"#,
            at(offset)
        ));
        self
    }

    fn notified(mut self, id: &str, offset: i64, kind: &str) -> Self {
        self.events.push(format!(
            r#"{{"type":"Notification","id":"{id}","at":{},"kind":"{kind}"}}"#,
            at(offset)
        ));
        self
    }

    /// One session's own transcript: what names the row and what it cost.
    fn transcript(self, project: &str, id: &str, prompt: &str, offset: i64, tokens: u64) -> Self {
        let dir = self.root.join(".claude/projects").join(project);
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = format!("/Users/x/{}", project.rsplit('-').next().unwrap());
        std::fs::write(
            dir.join(format!("{id}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{id}\",\"cwd\":\"{cwd}\",\"gitBranch\":\"main\",\"timestamp\":\"{ts}\"}}\n\
                 {{\"type\":\"last-prompt\",\"sessionId\":\"{id}\",\"lastPrompt\":\"{prompt}\"}}\n\
                 {{\"type\":\"assistant\",\"sessionId\":\"{id}\",\"timestamp\":\"{ts}\",\"message\":{{\"model\":\"claude-opus-4-8\",\"usage\":{{\"input_tokens\":100,\"output_tokens\":{tokens}}}}}}}\n",
                ts = iso(at(offset)),
            ),
        )
        .unwrap();
        self
    }

    /// A background agent's transcript, under the session that spawned it.
    fn agent(self, project: &str, id: &str, offset: i64, tokens: u64) -> Self {
        let dir = self
            .root
            .join(".claude/projects")
            .join(project)
            .join(id)
            .join("subagents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agent-abc123.jsonl"),
            format!(
                "{{\"type\":\"assistant\",\"isSidechain\":true,\"sessionId\":\"{id}\",\"cwd\":\"/Users/x/somewhere/deep\",\"gitBranch\":\"feat/x\",\"timestamp\":\"{ts}\",\"message\":{{\"model\":\"claude-opus-4-8\",\"usage\":{{\"input_tokens\":50,\"output_tokens\":{tokens}}}}}}}\n",
                ts = iso(at(offset)),
            ),
        )
        .unwrap();
        self
    }

    fn build(self) -> PathBuf {
        std::fs::write(
            self.root.join(".specola/events.jsonl"),
            self.events.join("\n") + "\n",
        )
        .unwrap();
        self.root
    }
}

/// Three fields are about the machine that ran the test, not about the day. The focus
/// label follows the platform's capabilities and the hook snippet carries an absolute
/// path to the shim; `todayCost` is counted from *local* midnight, so it moves with the
/// runner's timezone. None of them belongs in a file two machines compare.
fn redact(mut snapshot: serde_json::Value) -> serde_json::Value {
    snapshot["hookSnippet"] = serde_json::json!("<snippet>");
    snapshot["todayCost"] = serde_json::json!("<local midnight>");
    if let Some(sessions) = snapshot["sessions"].as_array_mut() {
        for session in sessions {
            session["focus"] = serde_json::json!("<focus>");
        }
    }
    snapshot
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/snapshots")
}

fn check(name: &str, home: PathBuf, span: Span) {
    let now = Local.timestamp_opt(NOW, 0).single().expect("fixed clock");
    let snapshot = App::at(home.clone()).snapshot(span, now);
    let actual = redact(serde_json::to_value(&snapshot).expect("snapshot should serialize"));
    let pretty = serde_json::to_string_pretty(&actual).unwrap() + "\n";
    let path = goldens().join(format!("{name}.json"));

    if std::env::var("UPDATE_GOLDENS").is_ok() {
        std::fs::create_dir_all(goldens()).unwrap();
        std::fs::write(&path, &pretty).unwrap();
    } else {
        // Git may check the file out with CRLF; the comparison is about the day, not
        // about which platform cloned the repo.
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| {
                panic!("no golden at {path:?} — run with UPDATE_GOLDENS=1 and read the diff")
            })
            .replace("\r\n", "\n");
        assert_eq!(
            pretty, expected,
            "snapshot changed for {name}; if you meant it, UPDATE_GOLDENS=1 and read the diff"
        );
    }

    std::fs::remove_dir_all(&home).ok();
}

/// Claude is asking for permission and nothing has answered it. The one thing the panel
/// exists to show.
#[test]
fn blocked_on_you() {
    let home = Home::new("blocked")
        .started("s-block", "/Users/x/api", -900, "Opened")
        .prompted("s-block", -600)
        .notified("s-block", -570, "Permission")
        .transcript("-Users-x-api", "s-block", "run the migration", -580, 400)
        .build();

    check("blocked-on-you", home, Span::FourHours);
}

/// The reported bug: the prompt went to a background agent, the main loop returned to its
/// own prompt, and the idle nudge followed. Your move, and nothing is blocked on you.
#[test]
fn a_background_agent_is_working() {
    let home = Home::new("agent")
        .started("s-agent", "/Users/x/web", -900, "Opened")
        .prompted("s-agent", -600)
        .notified("s-agent", -540, "Idle")
        .transcript("-Users-x-web", "s-agent", "review the diff", -600, 200)
        .agent("-Users-x-web", "s-agent", -120, 3000)
        .build();

    check("background-agent", home, Span::FourHours);
}

/// A project opened and a session resumed, with nothing asked of it.
#[test]
fn just_opened() {
    let home = Home::new("opened")
        .started("s-open", "/Users/x/bebe", -1200, "Opened")
        .transcript(
            "-Users-x-bebe",
            "s-open",
            "extract the ebook text",
            -604_800,
            50,
        )
        .build();

    check("just-opened", home, Span::FourHours);
}

/// Several projects at once, which is the state the panel is actually read in: one
/// blocked, one working, one finished and waiting on you to look, one quiet.
#[test]
fn a_working_day() {
    let home = Home::new("day")
        .started("s-block", "/Users/x/api", -3000, "Opened")
        .prompted("s-block", -1800)
        .notified("s-block", -1740, "Permission")
        .transcript(
            "-Users-x-api",
            "s-block",
            "fix the failing migration",
            -1750,
            400,
        )
        .started("s-work", "/Users/x/web", -2400, "Opened")
        .prompted("s-work", -300)
        .transcript(
            "-Users-x-web",
            "s-work",
            "add the dark theme toggle",
            -290,
            900,
        )
        .started("s-turn", "/Users/x/docs", -5000, "Opened")
        .prompted("s-turn", -900)
        .stopped("s-turn", -700)
        .transcript(
            "-Users-x-docs",
            "s-turn",
            "rewrite the readme intro",
            -710,
            150,
        )
        .started("s-quiet", "/Users/x/api", -9000, "Opened")
        .prompted("s-quiet", -8000)
        .stopped("s-quiet", -7800)
        .transcript("-Users-x-api", "s-quiet", "bump the toolchain", -7810, 80)
        .build();

    // `FourHours` on purpose: `Today` starts at *local* midnight, which is a different
    // instant on every runner. Every event above is inside four hours.
    check("a-working-day", home, Span::FourHours);
}
