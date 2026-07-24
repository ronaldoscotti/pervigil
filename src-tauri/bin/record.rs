use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use pervigil_lib::core::terminal::Terminal;
use pervigil_lib::io::record::{append_line, build_event};
use pervigil_lib::io::terminals;

/// Always exits 0. A monitor that can fail the turn it monitors is worse than none,
/// so every error path here is silently swallowed.
fn main() {
    let _ = record();
}

fn record() -> Option<()> {
    let kind = std::env::args().nth(1)?;

    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload).ok()?;

    let at = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let term = terminal();
    let event = build_event(&kind, &payload, at, parent_pid(), term.clone())?;
    let line = serde_json::to_string(&event).ok()?;

    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;

    // Cache the terminal for focus, keyed by session id, on every hook — so a session
    // that never fired SessionStart still becomes focusable on its next event.
    if let Some(hint) = term.some() {
        let _ = terminals::write(&home, event.id(), &hint);
    }

    append_line(&home.join(".pervigil").join("events.jsonl"), &line).ok()
}

/// Read the terminal context from the shim's own environment — the hook runs inside
/// the session's process tree, so these vars are the session's.
fn terminal() -> Terminal {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    Terminal {
        program: var("TERM_PROGRAM"),
        tmux_pane: var("TMUX_PANE"),
        iterm_session: var("ITERM_SESSION_ID"),
    }
}

#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    None
}
