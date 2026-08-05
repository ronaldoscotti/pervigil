//! A session that dispatched a background agent, through the whole app: hook log plus
//! the transcript tree, folded into the panel a user would be looking at.
//!
//! The two notifications are indistinguishable in the log without their kind, and they
//! mean opposite things here. Getting them the wrong way round is either a false alarm
//! or — worse — silence about a real block.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;
use specola_lib::app::App;
use specola_lib::core::session::SessionState;
use specola_lib::core::span::Span;

const ID: &str = "61790ba9";

/// The reported case, to the minute: the prompt goes to a background agent at 21:38,
/// the main transcript stops there, the idle nudge lands at 21:39, and the agent is
/// still writing at 21:44.
fn home_with_a_working_agent(notification_type: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!("specola-agent-{notification_type}-{nanos}"));
    let project = home.join(".claude/projects/-Users-x-proj");
    std::fs::create_dir_all(home.join(".specola")).unwrap();
    std::fs::create_dir_all(project.join(format!("{ID}/subagents"))).unwrap();

    let now = Local::now();
    let prompt = now.timestamp() - 6 * 60;
    let notified = prompt + 60;
    let agent_wrote = prompt + 5 * 60;
    let stamp = |at: i64| {
        chrono::DateTime::from_timestamp(at, 0)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };

    // No SessionStart: this session was already running when the hooks went in, so it
    // has no pid, which is also what keeps liveness from retiring a fabricated one.
    std::fs::write(
        home.join(".specola/events.jsonl"),
        format!(
            "{{\"type\":\"UserPromptSubmit\",\"id\":\"{ID}\",\"at\":{prompt}}}\n\
             {{\"type\":\"Notification\",\"id\":\"{ID}\",\"at\":{notified},\"kind\":\"{kind}\"}}\n",
            kind = if notification_type == "idle_prompt" {
                "Idle"
            } else {
                "Permission"
            },
        ),
    )
    .unwrap();

    std::fs::write(
        project.join(format!("{ID}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"sessionId\":\"{ID}\",\"cwd\":\"/Users/x/proj\",\"timestamp\":\"{ts}\"}}\n\
             {{\"type\":\"last-prompt\",\"sessionId\":\"{ID}\",\"lastPrompt\":\"run the migration\"}}\n\
             {{\"type\":\"assistant\",\"sessionId\":\"{ID}\",\"timestamp\":\"{ts}\",\"message\":{{\"model\":\"claude-opus-4-8\",\"usage\":{{\"output_tokens\":10}}}}}}\n",
            ts = stamp(prompt),
        ),
    )
    .unwrap();

    std::fs::write(
        project.join(format!("{ID}/subagents/agent-afc3523.jsonl")),
        format!(
            "{{\"type\":\"assistant\",\"isSidechain\":true,\"sessionId\":\"{ID}\",\"cwd\":\"/Users/x/proj\",\"gitBranch\":\"feat/x\",\"timestamp\":\"{ts}\",\"message\":{{\"model\":\"claude-opus-4-8\",\"usage\":{{\"output_tokens\":300}}}}}}\n",
            ts = stamp(agent_wrote),
        ),
    )
    .unwrap();

    home
}

#[test]
fn an_idle_nudge_beside_a_working_agent_is_not_blocked_on_you() {
    let home = home_with_a_working_agent("idle_prompt");
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    assert_eq!(snapshot.sessions.len(), 1, "{:?}", snapshot.sessions);
    // The nudge fired because the main loop went quiet; the agent it dispatched has not.
    assert_eq!(snapshot.sessions[0].state, SessionState::Working);
    assert_eq!(snapshot.waiting, 0, "nothing here is waiting on the user");
    assert_eq!(
        snapshot.sessions[0].name, "run the migration",
        "the agent's file must not rename the row to its branch"
    );
    assert_eq!(
        snapshot.waiting_share, 0.0,
        "and the lane agrees, with no amber at all: {:?}",
        snapshot.segments
    );

    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn a_permission_prompt_beside_a_working_agent_still_is() {
    let home = home_with_a_working_agent("permission_prompt");
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    assert_eq!(snapshot.sessions.len(), 1, "{:?}", snapshot.sessions);
    assert_eq!(
        snapshot.sessions[0].state,
        SessionState::WaitingOnYou,
        "an agent writing is not you answering"
    );
    assert_eq!(snapshot.waiting, 1);

    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn the_agents_spend_is_billed_to_the_session_that_spawned_it() {
    let home = home_with_a_working_agent("idle_prompt");
    let app = App::at(home.clone());

    let snapshot = app.snapshot(Span::FourHours, Local::now());

    let row = snapshot.sessions[0].cost.expect("the row should be priced");
    assert!(
        snapshot.tokens >= 310,
        "310 output tokens were spent, 300 of them by the agent: {} counted",
        snapshot.tokens
    );
    assert!(row > 0.0);

    std::fs::remove_dir_all(&home).ok();
}
