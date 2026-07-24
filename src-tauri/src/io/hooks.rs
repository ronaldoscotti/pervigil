use serde::Serialize;

/// The four Claude Code events pervigil records. `SessionStart` gives the row and its
/// terminal, `Notification` is the only honest "blocked on you", `Stop`/`UserPromptSubmit`
/// close and reopen the loop.
pub const EVENTS: [&str; 4] = ["SessionStart", "Notification", "Stop", "UserPromptSubmit"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookReport {
    /// One flag per event in [`EVENTS`] order — a partial install is shown honestly,
    /// not rounded to "installed".
    pub events: Vec<HookState>,
    pub all_installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookState {
    pub event: String,
    pub installed: bool,
}

/// Which of pervigil's hooks are wired in `~/.claude/settings.json`. Parses defensively:
/// an unreadable or hook-less file simply reports nothing installed, never an error.
pub fn detect(settings_json: &str) -> HookReport {
    let root: serde_json::Value = serde_json::from_str(settings_json).unwrap_or_default();
    let hooks = &root["hooks"];

    let events: Vec<HookState> = EVENTS
        .iter()
        .map(|event| HookState {
            event: (*event).to_string(),
            installed: wired(&hooks[event], event),
        })
        .collect();

    HookReport {
        all_installed: events.iter().all(|state| state.installed),
        events,
    }
}

/// True when a command under this event invokes the `record` shim with the event as its
/// argument. Matched on `record` + the event name — not the brand — so an install by
/// absolute path (`…/Pervigil.app/…/record`) is recognised regardless of case.
fn wired(entries: &serde_json::Value, event: &str) -> bool {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["hooks"].as_array())
        .flatten()
        .filter_map(|hook| hook["command"].as_str())
        .any(|command| command.contains("record") && command.contains(event))
}

/// The block to paste into `~/.claude/settings.json` — one command per event, calling
/// the bundled shim by absolute path. Pervigil never writes this file itself (spec item
/// 12); the user pastes it, so it's shown, not applied.
pub fn snippet(record_path: &str) -> String {
    let hooks: serde_json::Map<String, serde_json::Value> = EVENTS
        .iter()
        .map(|event| {
            let entry = serde_json::json!([{
                "hooks": [{ "type": "command", "command": format!("\"{record_path}\" {event}") }]
            }]);
            ((*event).to_string(), entry)
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({ "hooks": hooks }))
        .expect("hook snippet should serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(report: &HookReport, event: &str) -> bool {
        report
            .events
            .iter()
            .find(|state| state.event == event)
            .map(|state| state.installed)
            .unwrap()
    }

    #[test]
    fn an_empty_or_unreadable_settings_file_reports_nothing_installed() {
        for text in ["", "{}", "not json at all", r#"{"hooks":{}}"#] {
            let report = detect(text);
            assert!(!report.all_installed, "text {text:?} should read as absent");
            assert!(report.events.iter().all(|state| !state.installed));
        }
    }

    #[test]
    fn a_full_install_is_detected_whatever_the_binary_path() {
        let settings = r#"{
          "hooks": {
            "SessionStart": [{"hooks":[{"type":"command","command":"/Applications/Pervigil.app/Contents/MacOS/record SessionStart"}]}],
            "Notification": [{"hooks":[{"type":"command","command":"/opt/pervigil/record Notification"}]}],
            "Stop": [{"hooks":[{"type":"command","command":"pervigil record Stop"}]}],
            "UserPromptSubmit": [{"hooks":[{"type":"command","command":"pervigil record UserPromptSubmit"}]}]
          }
        }"#;

        assert!(detect(settings).all_installed);
    }

    #[test]
    fn a_partial_install_is_reported_honestly_not_rounded_up() {
        let settings = r#"{
          "hooks": {
            "SessionStart": [{"hooks":[{"type":"command","command":"pervigil record SessionStart"}]}],
            "Notification": [{"hooks":[{"type":"command","command":"some-other-tool notify"}]}]
          }
        }"#;

        let report = detect(settings);

        assert!(!report.all_installed);
        assert!(installed(&report, "SessionStart"));
        assert!(
            !installed(&report, "Notification"),
            "a foreign hook isn't ours"
        );
        assert!(!installed(&report, "Stop"));
    }

    #[test]
    fn the_snippet_wires_every_event_and_is_detected_by_our_own_reader() {
        let snippet = snippet("/opt/pervigil/record");

        let report = detect(&snippet);

        assert!(
            report.all_installed,
            "what we tell the user to paste must satisfy our own detection"
        );
        assert!(snippet.contains("/opt/pervigil/record"));
    }

    #[test]
    fn a_foreign_hooks_block_does_not_count_as_installed() {
        let settings =
            r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"echo hi"}]}]}}"#;

        assert!(!detect(settings).all_installed);
    }
}
