use chrono::DateTime;
use serde::Deserialize;

use crate::core::event::Timestamp;
use crate::core::pricing::{TokenUsage, UsageEntry};
use crate::core::session::{Session, SessionState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    session_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    timestamp: Option<String>,
    ai_title: Option<String>,
    last_prompt: Option<String>,
    message: Option<AssistantMessage>,
}

#[derive(Deserialize)]
struct AssistantMessage {
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_creation: Option<CacheCreation>,
}

#[derive(Deserialize)]
struct CacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

impl From<RawUsage> for TokenUsage {
    fn from(raw: RawUsage) -> Self {
        let split = raw.cache_creation;
        TokenUsage {
            input: raw.input_tokens.unwrap_or(0),
            output: raw.output_tokens.unwrap_or(0),
            cache_read: raw.cache_read_input_tokens.unwrap_or(0),
            // Without the per-TTL split, bill the cheaper rate rather than
            // overstating cost.
            cache_write_5m: match &split {
                Some(split) => split.ephemeral_5m_input_tokens.unwrap_or(0),
                None => raw.cache_creation_input_tokens.unwrap_or(0),
            },
            cache_write_1h: split
                .and_then(|split| split.ephemeral_1h_input_tokens)
                .unwrap_or(0),
        }
    }
}

/// One Claude Code transcript, folded line by line. Transcripts are append-only and
/// run to gigabytes across all projects, so a growing file is absorbed incrementally
/// instead of re-read.
#[derive(Debug, Default)]
pub struct Transcript {
    id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    ai_title: Option<String>,
    last_prompt: Option<String>,
    last_active: Timestamp,
    pub usage: Vec<UsageEntry>,
}

/// The longest session name a row can carry; a `lastPrompt` can be kilobytes.
const NAME_LIMIT: usize = 80;

impl Transcript {
    /// Unreadable lines are ignored: a half-written tail must never blind the panel.
    pub fn absorb(&mut self, line: &str) {
        let Ok(record) = serde_json::from_str::<Line>(line) else {
            return;
        };

        self.id = self.id.take().or(record.session_id);
        self.cwd = self.cwd.take().or(record.cwd);
        if let Some(branch) = record.git_branch.filter(|b| b != "HEAD" && !b.is_empty()) {
            self.git_branch = Some(branch);
        }
        self.ai_title = record.ai_title.or_else(|| self.ai_title.take());
        self.last_prompt = record.last_prompt.or_else(|| self.last_prompt.take());

        let at = record.timestamp.as_deref().and_then(to_epoch);
        if let Some(at) = at {
            self.last_active = self.last_active.max(at);
        }

        if record.kind.as_deref() != Some("assistant") {
            return;
        }
        if let (Some(at), Some(message)) = (at, record.message) {
            if let (Some(raw), model) = (message.usage, message.model.unwrap_or_default()) {
                self.usage.push(UsageEntry {
                    at,
                    model,
                    usage: raw.into(),
                });
            }
        }
    }

    /// `None` until some line names the session.
    pub fn session(&self) -> Option<Session> {
        let id = self.id.clone()?;
        Some(Session {
            title: self.name(),
            id,
            cwd: self.cwd.clone().unwrap_or_default(),
            pid: None,
            state: SessionState::Idle,
            since: self.last_active,
            last_active: self.last_active,
            git_branch: self.git_branch.clone(),
            terminal: None,
        })
    }

    /// The transcript's share of spec item 13's tiers: Claude Code's own title, else
    /// the last prompt, else the branch. The short-id floor is the caller's, because
    /// hook-only sessions have no transcript to reach it through. Never
    /// authoritative — a title lags its session.
    fn name(&self) -> Option<String> {
        self.ai_title
            .clone()
            .or_else(|| self.last_prompt.as_deref().map(one_line))
            .or_else(|| self.git_branch.clone())
    }
}

fn one_line(prompt: &str) -> String {
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(NAME_LIMIT) {
        Some((cut, _)) => format!("{}…", flat[..cut].trim_end()),
        None => flat,
    }
}

fn to_epoch(timestamp: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|at| at.timestamp() as Timestamp)
}

#[cfg(test)]
pub fn parse(contents: &str) -> Transcript {
    let mut transcript = Transcript::default();
    for line in contents.lines() {
        transcript.absorb(line);
    }
    transcript
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = concat!(
        r#"{"type":"user","sessionId":"5f5f47fc","cwd":"/Users/x/pervigil","gitBranch":"feat/timeline","timestamp":"2026-07-23T10:00:00.000Z"}"#,
        "\n",
        r#"{"type":"ai-title","sessionId":"5f5f47fc","aiTitle":"Design the panel"}"#,
        "\n",
        r#"garbage that is not json"#,
        "\n",
        r#"{"type":"assistant","sessionId":"5f5f47fc","cwd":"/Users/x/pervigil","gitBranch":"feat/timeline","timestamp":"2026-07-23T16:39:50.417Z"}"#,
        "\n",
    );

    #[test]
    fn reads_id_cwd_title_and_last_activity() {
        let session = parse(TRANSCRIPT)
            .session()
            .expect("transcript should yield a session");

        assert_eq!(session.id, "5f5f47fc");
        assert_eq!(session.cwd, "/Users/x/pervigil");
        assert_eq!(session.title.as_deref(), Some("Design the panel"));
        assert_eq!(session.git_branch.as_deref(), Some("feat/timeline"));
        assert_eq!(session.last_active, 1_784_824_790);
    }

    #[test]
    fn a_transcript_session_is_idle_with_no_pid() {
        let session = parse(TRANSCRIPT).session().unwrap();

        assert_eq!(session.state, SessionState::Idle);
        assert_eq!(session.pid, None, "transcripts carry no pid");
    }

    #[test]
    fn head_is_not_a_real_branch() {
        let contents = r#"{"type":"user","sessionId":"abc","cwd":"/p","gitBranch":"HEAD","timestamp":"2026-07-23T10:00:00.000Z"}"#;

        let session = parse(contents).session().unwrap();

        assert_eq!(session.git_branch, None);
    }

    #[test]
    fn a_transcript_with_no_session_id_yields_nothing() {
        assert!(parse(r#"{"type":"file-history-delta"}"#)
            .session()
            .is_none());
    }

    fn name_of(contents: &str) -> Option<String> {
        parse(contents).session().unwrap().title
    }

    #[test]
    fn the_last_prompt_names_a_session_that_has_no_title_yet() {
        let contents = concat!(
            r#"{"type":"user","sessionId":"abcdef1234","cwd":"/p","gitBranch":"main","timestamp":"2026-07-23T10:00:00.000Z"}"#,
            "\n",
            r#"{"type":"last-prompt","sessionId":"abcdef1234","lastPrompt":"why does this  request\nreturn a 400?"}"#,
        );

        assert_eq!(
            name_of(contents).as_deref(),
            Some("why does this request return a 400?")
        );
    }

    #[test]
    fn the_title_outranks_the_last_prompt() {
        let contents = concat!(
            r#"{"type":"last-prompt","sessionId":"abcdef1234","lastPrompt":"fix the thing"}"#,
            "\n",
            r#"{"type":"ai-title","sessionId":"abcdef1234","aiTitle":"Fix the login redirect"}"#,
        );

        assert_eq!(name_of(contents).as_deref(), Some("Fix the login redirect"));
    }

    #[test]
    fn the_branch_names_a_session_with_neither() {
        let contents =
            r#"{"type":"user","sessionId":"abcdef1234","cwd":"/p","gitBranch":"feat/lane"}"#;

        assert_eq!(name_of(contents).as_deref(), Some("feat/lane"));
    }

    #[test]
    fn a_transcript_with_nothing_to_name_it_leaves_the_floor_to_the_caller() {
        let contents = r#"{"type":"user","sessionId":"abcdef1234-5678","cwd":"/p"}"#;

        assert_eq!(name_of(contents), None);
    }

    #[test]
    fn a_kilobyte_prompt_is_cut_to_one_readable_line() {
        let prompt = "word ".repeat(400);
        let contents =
            format!(r#"{{"type":"last-prompt","sessionId":"abcdef1234","lastPrompt":"{prompt}"}}"#);

        let name = name_of(&contents).unwrap();

        assert!(name.ends_with('…'));
        assert!(name.chars().count() <= NAME_LIMIT + 1, "got {name:?}");
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    const ASSISTANT: &str = concat!(
        r#"{"type":"user","sessionId":"s1","cwd":"/p","timestamp":"2026-07-23T16:39:50.417Z"}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"s1","timestamp":"2026-07-23T16:39:50.417Z","message":{"model":"claude-opus-4-8","usage":{"input_tokens":2,"output_tokens":343,"cache_read_input_tokens":20458,"cache_creation_input_tokens":27484,"cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":27484}}}}"#,
        "\n",
    );

    #[test]
    fn reads_model_time_and_every_token_class() {
        let entries = parse(ASSISTANT).usage;

        assert_eq!(entries.len(), 1, "only assistant records carry usage");
        let entry = &entries[0];
        assert_eq!(entry.model, "claude-opus-4-8");
        assert_eq!(entry.at, 1_784_824_790);
        assert_eq!(entry.usage.input, 2);
        assert_eq!(entry.usage.output, 343);
        assert_eq!(entry.usage.cache_read, 20_458);
        assert_eq!(entry.usage.cache_write_1h, 27_484);
        assert_eq!(entry.usage.cache_write_5m, 0);
    }

    #[test]
    fn falls_back_to_the_five_minute_rate_when_the_split_is_absent() {
        let line = r#"{"type":"assistant","timestamp":"2026-07-23T16:39:50.417Z","message":{"model":"claude-opus-4-8","usage":{"cache_creation_input_tokens":900}}}"#;

        let entries = parse(line).usage;

        assert_eq!(entries[0].usage.cache_write_5m, 900);
        assert_eq!(entries[0].usage.cache_write_1h, 0);
    }

    #[test]
    fn a_record_without_usage_is_not_an_entry() {
        let line = r#"{"type":"assistant","timestamp":"2026-07-23T16:39:50.417Z","message":{"model":"claude-opus-4-8"}}"#;

        assert!(parse(line).usage.is_empty());
    }

    #[test]
    fn absorbing_the_appended_tail_adds_to_what_was_already_read() {
        let mut transcript = parse(ASSISTANT);

        transcript.absorb(r#"{"type":"assistant","timestamp":"2026-07-23T16:45:00.000Z","message":{"model":"claude-opus-4-8","usage":{"output_tokens":10}}}"#);

        assert_eq!(transcript.usage.len(), 2, "the first read is not repeated");
    }
}
