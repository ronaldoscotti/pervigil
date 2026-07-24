//! Regression suite: one realistic day of parallel sessions, including the ugly cases —
//! a terminal killed without a Stop event, a session resumed after hours, and two
//! sessions live in the same project.

use pervigil_lib::core::event::Event;
use pervigil_lib::core::session::{SessionState, ViewPrefs};
use pervigil_lib::core::store::{fold, timeline, waiting_share};

/// 19:00, seconds since midnight — the moment the day is read at.
const NOW: u64 = 68_400;
const DAY_START: u64 = 28_800;

fn full_day() -> Vec<Event> {
    include_str!("fixtures/full_day.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line should parse"))
        .collect()
}

#[test]
fn folds_to_waiting_first_then_most_recent() {
    let sessions = fold(&full_day(), NOW, &ViewPrefs::default());
    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();

    assert_eq!(ids, vec!["s-perv-main", "s-pos", "s-perv-feat", "s-killed"]);
    assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
    assert_eq!(sessions[0].since, 39_600);
}

#[test]
fn a_session_resumed_after_hours_is_working_again() {
    let sessions = fold(&full_day(), NOW, &ViewPrefs::default());
    let resumed = sessions.iter().find(|s| s.id == "s-pos").unwrap();

    assert_eq!(resumed.state, SessionState::Working);
    assert_eq!(resumed.since, 64_800);
}

#[test]
fn a_killed_terminal_never_sends_stop_and_stays_working() {
    let sessions = fold(&full_day(), NOW, &ViewPrefs::default());
    let killed = sessions.iter().find(|s| s.id == "s-killed").unwrap();

    // fold has no way to know the process died — liveness (M5) is what hides it.
    assert_eq!(killed.state, SessionState::Working);
}

#[test]
fn one_project_can_hold_two_live_sessions() {
    let sessions = fold(&full_day(), NOW, &ViewPrefs::default());
    let in_pervigil = sessions
        .iter()
        .filter(|s| s.cwd.ends_with("/pervigil"))
        .count();

    assert_eq!(in_pervigil, 2);
}

#[test]
fn the_lane_covers_the_whole_day_and_reports_a_waiting_share() {
    let segments = timeline(&full_day(), DAY_START, NOW);

    assert_eq!(segments.first().unwrap().from, DAY_START);
    assert_eq!(segments.last().unwrap().to, NOW);
    for pair in segments.windows(2) {
        assert_eq!(pair[0].to, pair[1].from);
    }

    let share = waiting_share(&segments);
    assert!(share > 0.0 && share < 1.0, "share was {share}");
}
