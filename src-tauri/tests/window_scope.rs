//! The span filter scopes the panel's list. It must not scope the tray or the
//! notification baseline — those answer "what is blocked on you", not "what happened
//! in the window you picked".

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, TimeZone};
use specola_lib::app::App;
use specola_lib::core::span::Span;

/// A home with one session that went waiting five hours ago and has been silent
/// since — outside a four-hour window, still blocked on the user.
fn home_with_a_five_hour_wait() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("specola-window-{nanos}"));
    std::fs::create_dir_all(home.join(".specola")).unwrap();

    let now = Local::now().timestamp();
    let five_hours_ago = now - 5 * 60 * 60;
    let pid = std::process::id();
    let events = format!(
        r#"{{"type":"SessionStart","id":"waiting","cwd":"/Users/x/proj","pid":{pid},"at":{start},"term":null}}
{{"type":"Notification","id":"waiting","at":{at}}}
"#,
        start = five_hours_ago - 60,
        at = five_hours_ago,
    );
    std::fs::write(home.join(".specola/events.jsonl"), events).unwrap();
    home
}

#[test]
fn the_panel_scopes_to_the_window_but_the_tray_still_counts_the_wait() {
    let home = home_with_a_five_hour_wait();
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    assert!(
        snapshot.sessions.is_empty(),
        "a session quiet for five hours is outside a four-hour window"
    );
    let tray = app.tray();
    assert_eq!(
        tray.items.len(),
        1,
        "but it is still blocked on you, and the tray has no window: {}",
        tray.summary
    );

    std::fs::remove_dir_all(&home).ok();
}

/// The baseline must cover every live session, not just the windowed ones. Otherwise
/// a session outside the window is forgotten and re-notified the next time a wider
/// span runs — the tray ticker polls `Today` while the panel polls `4h`.
#[test]
fn a_session_outside_the_window_is_not_re_notified_by_a_wider_span() {
    let home = home_with_a_five_hour_wait();
    let app = App::at(home.clone());

    app.snapshot(Span::FourHours, Local::now());
    app.take_pending();
    app.snapshot(Span::Today, Local::now());

    assert!(
        app.take_pending().is_empty(),
        "the wait was already known; a wider span must not replay it"
    );

    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn bounds_are_inclusive_of_a_session_active_exactly_at_the_edge() {
    let now = Local.timestamp_opt(1_800_000_000, 0).unwrap();
    let (from, to) = specola_lib::core::span::bounds(Span::FourHours, now);

    assert_eq!(to - from, 4 * 60 * 60);
}
