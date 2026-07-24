use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use pervigil_lib::io::record::{append_line, build_event};

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
    let event = build_event(&kind, &payload, at, parent_pid())?;
    let line = serde_json::to_string(&event).ok()?;

    let path = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)?
        .join(".pervigil")
        .join("events.jsonl");

    append_line(&path, &line).ok()
}

#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    None
}
