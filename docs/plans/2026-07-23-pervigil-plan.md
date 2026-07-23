# Pervigil Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pinned, cross-platform desktop panel that shows every Claude Code session, surfaces the ones blocked on you, and lets you jump to them — built macOS-first with a portable core.

**Architecture:** A pure Rust core (`fold(events) -> sessions`) with no clock/fs/GUI, fed by an append-only event log that a bundled `record` shim writes from Claude Code hooks. Cost is a second, independent input read from transcripts. A Tauri v2 shell renders the state; per-OS adapters (`WindowFocuser`, `liveness`) sit behind traits with honest capability detection.

**Tech Stack:** Rust, Tauri v2, a web frontend (framework chosen in M0), `notify` (fs-watching), `serde`/`serde_json`, `sysinfo` (liveness). Testing: Rust `#[test]` + JSON fixtures.

---

## Planning approach (read first)

This plan is written **just-in-time**, and that is deliberate — false step-by-step
precision for code whose shape depends on earlier milestones would violate this
repo's honesty rule (see `CLAUDE.md`). So:

- **M0–M2 are fully detailed** as bite-sized TDD tasks — they are the immediate,
  executable next actions and they don't depend on anything unbuilt.
- **M3–M10 are milestone specs**: goal, files, interfaces, and *how each is
  verified*. Each is expanded into bite-sized tasks **when reached**, against the
  real code that exists by then. This is planned decomposition, not vagueness.

**Verification honesty.** Pure-logic milestones use TDD (red → green → commit).
UI, packaging, and actual window-focusing can't be meaningfully unit-tested in the
same way — those milestones declare their real verification method (`agent-browser`
QA, manual capability check on real terminals) instead of pretending to TDD.

**Milestone map**

| M | Milestone | Verification |
|---|-----------|--------------|
| M0 | Scaffold + CI + test harness | build + a trivial passing test in CI |
| M1 | **The pure core** — `store::fold` | TDD, JSON fixtures (crown jewel) |
| M2 | **Design-direction spike** (`frontend-design`) | static mockup renders fixture data; the first screenshot |
| M3 | Ingestion — `record` shim + `watcher` | TDD on parsing; integration test on a temp log |
| M4 | Cost — `pricing` + price table | TDD |
| M5 | Liveness + prune | TDD |
| M6 | UI implementation (wire core → M2 design) | `agent-browser` QA |
| M7 | Focuser — trait + tiers + capability detection | TDD tier-selection; manual focus check |
| M8 | Notifications + config + pin/dismiss + project visibility | TDD logic; manual UX |
| M9 | Hook-install UX (snippet + copy + live detection) | manual + detection test |
| M10 | Packaging + README + demo capture | signed build launches; demo recorded |

---

## File structure (locked here)

```
pervigil/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs              # Tauri bootstrap, tray, window, command handlers
│   │   ├── core/
│   │   │   ├── mod.rs
│   │   │   ├── event.rs         # Event enum + serde (the log's line schema)
│   │   │   ├── store.rs         # fold(Vec<Event>) -> Vec<Session>  ← PURE, the heart
│   │   │   ├── session.rs       # Session, SessionState types
│   │   │   ├── pricing.rs       # tokens × price table -> cost
│   │   │   └── prune.rs         # drop events older than 30d
│   │   ├── io/
│   │   │   ├── watcher.rs       # tail events.jsonl + transcript dir (notify)
│   │   │   └── transcript.rs    # parse ~/.claude/projects/**/*.jsonl -> tokens
│   │   ├── platform/
│   │   │   ├── focuser.rs       # trait WindowFocuser + tier dispatch
│   │   │   ├── focuser_macos.rs # AX/AppleScript, tmux, iTerm2, VS Code, clipboard
│   │   │   └── liveness.rs      # is pid alive? (sysinfo)
│   │   └── config.rs           # load/save settings
│   ├── bin/
│   │   └── record.rs           # `pervigil record` — the hook shim (atomic append)
│   ├── tests/
│   │   └── fixtures/           # recorded event sequences (*.jsonl) + expected states
│   ├── Cargo.toml
│   └── tauri.conf.json
├── ui/                         # web frontend (framework picked in M0)
│   ├── index.html
│   ├── src/
│   └── mock/                   # M2 static mockup + fixture data (screenshotable)
└── assets/pricing.json         # shipped price table
```

**Boundary rule:** `core/` never imports `io/`, `platform/`, or Tauri. It is pure.
That import boundary is what keeps `store` fixture-testable and is enforced by
review.

---

## M0 — Scaffold, CI, test harness

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`, `ui/` (Tauri v2 init), `.github/workflows/ci.yml`

- [ ] **Step 1: Scaffold Tauri v2 app**

Run: `npm create tauri-app@latest pervigil -- --template vanilla-ts` (or chosen
frontend). Move output into repo layout above. Confirm `npm run tauri dev` opens a
window on macOS.
Expected: a window launches.

- [ ] **Step 2: Add a trivial pure-core module + failing test**

`src-tauri/src/core/mod.rs`:
```rust
pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }
```
`src-tauri/src/core/mod.rs` (test):
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn version_is_nonempty() { assert!(!super::version().is_empty()); }
}
```

- [ ] **Step 3: Run test**

Run: `cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 4: CI runs build + test on macOS**

`.github/workflows/ci.yml`: on push/PR, `cargo test` + `cargo clippy -- -D warnings`
+ `cargo fmt --check` on `macos-latest`. (Windows/Linux jobs added in M10 as
allowed-to-fail until tested.)

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: scaffold Tauri v2 app, CI, and pure-core test harness"
```

---

## M1 — The pure core: `store::fold` (crown jewel, full TDD)

The whole product's correctness lives here. `fold` takes a chronological slice of
events and returns the current sessions. No clock, no fs — "now" is passed in.

**Files:**
- Create: `src-tauri/src/core/event.rs`, `session.rs`, `store.rs`
- Test: inline `#[cfg(test)]` + `src-tauri/tests/fixtures/`

- [ ] **Step 1: Define the event + session types (failing test first)**

Test in `store.rs`:
```rust
#[test]
fn session_start_then_notification_is_waiting() {
    let events = vec![
        Event::SessionStart { id: "s1".into(), cwd: "/p".into(), pid: 10, at: 100 },
        Event::Notification { id: "s1".into(), at: 200 },
    ];
    let sessions = fold(&events, /*now*/ 250);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
    assert_eq!(sessions[0].since, 200); // elapsed anchored at the Notification
}
```

- [ ] **Step 2: Run — verify it fails to compile/pass**

Run: `cargo test session_start_then_notification_is_waiting`
Expected: FAIL (types/fn undefined).

- [ ] **Step 3: Minimal types + fold**

`event.rs`: `enum Event { SessionStart{...}, Notification{...}, Stop{...}, UserPromptSubmit{...} }` with `#[serde(tag="type")]`.
`session.rs`: `enum SessionState { Working, WaitingOnYou, Idle }`, `struct Session { id, cwd, pid, state, since, last_active }`.
`store.rs`: `fn fold(events: &[Event], now: u64) -> Vec<Session>` — group by id, apply last-writer state transition, set `since`/`last_active`.

- [ ] **Step 4: Run — PASS**

- [ ] **Step 5: Commit** — `feat(core): event/session types + fold happy path`

- [ ] **Step 6+: Add one failing test per rule, then satisfy it, then commit.** One rule per cycle:
  - `Stop` → `Idle`; `since` = Stop time.
  - `UserPromptSubmit` after waiting → `Working`.
  - Ordering: sessions sorted waiting-first, then by `last_active` desc.
  - Multiple sessions, same cwd → two distinct sessions (keyed by id, not cwd).
  - Corrupt/unknown event line → skipped, never panics (parse layer returns `Result`; `fold` only sees valid events).
  - Dismissed flag: a dismissed session is hidden until a *newer* event for its id arrives.
  - `now` drives elapsed only; `fold` output is otherwise pure of wall-clock.

- [ ] **Step N: Fixture regression test**

Add `tests/fixtures/full_day.jsonl` (a realistic multi-project day: killed
terminal, session resumed after hours, two sessions one project) + expected JSON.
One test folds the fixture and asserts the whole `Vec<Session>`. This fixture is
the artifact that proves the core.

---

## M2 — Design-direction spike (where visual design enters)

**Goal:** Lock the look before building plumbing. This is a portfolio artifact; the
screenshot is the deliverable, so design is de-risked *now*, not at QA.

**Sub-skill:** `frontend-design:frontend-design`.

**Files:**
- Create: `ui/mock/index.html` (self-contained), `ui/mock/fixture.json` (session data shaped exactly like `fold`'s output from M1)

**Tasks (not TDD — this is design):**
- [ ] Export a realistic `fold` output from the M1 fixture into `ui/mock/fixture.json` (real data shapes, so the mock isn't lying about density).
- [ ] Use `frontend-design` to produce a static, self-contained mockup rendering that data: session list (waiting-on-you pinned top), the timeline band (`4h · Today · Week`), elapsed timers, per-day cost. Light + dark.
- [ ] Capture the screenshot. **Gate: does it read as staff-level?** If not, iterate here — cheaply — before any UI plumbing exists.

**Verification:** a screenshot exists and clears the bar. This milestone's output
is also the first thing worth showing anyone.

---

## M3 — Ingestion: `record` shim + `watcher`

**Goal:** Get real events into the log and into the core, without ever blocking a
Claude Code turn.

**Files:** `src-tauri/bin/record.rs`, `src-tauri/src/io/watcher.rs`, `transcript.rs`

**Interfaces & rules:**
- `record`: reads hook JSON on stdin/args, appends one atomic line to
  `~/.pervigil/events.jsonl`. **Hard timeout, always `exit(0)`, never panics** —
  a failure here must not fail the host turn.
- `watcher`: `notify`-based tail of `events.jsonl`; emits parsed `Event`s to the
  Tauri layer. Corrupt lines counted + skipped.
- `transcript`: parse `~/.claude/projects/**/*.jsonl` → per-session token counts **and session
  title** (`{"type":"ai-title","aiTitle":…}`, fallback `{"type":"last-prompt","lastPrompt":…}` —
  verified present in real transcripts). Tiered: `aiTitle` → `lastPrompt` → branch → short id.

**Verification (TDD where pure):**
- [ ] TDD: line parser round-trips every `Event` variant; a corrupt line yields
  `Err` and is skipped, not fatal.
- [ ] TDD: `record` append is atomic under concurrent writers (write to temp +
  rename, or `O_APPEND` single `write`) — test with N threads appending, assert
  no interleaved/torn lines.
- [ ] Integration: write events to a temp log, run `watcher` fold, assert sessions.

---

## M4 — Cost: `pricing` + shipped price table

**Files:** `src-tauri/src/core/pricing.rs`, `assets/pricing.json`

- [ ] TDD: known model → `tokens × rate` correct to the cent.
- [ ] TDD: **unknown model → cost `None`**, rendered as `—`, never a wrong number
  (the spec's honesty rule).
- [ ] TDD: cost aggregates per session and per time-window (`4h/Today/Week`).

**Opt-in usage limits (spec item 13) — separate module, gated by config.** Default footer is $
cost (above). A settings toggle enables `usage::limits()`, which reads `~/.claude/.credentials.json`
and calls the OAuth usage endpoint for the 5h + weekly bars. Kept behind its own module + trait so
the endpoint call is mockable and the whole feature is off unless enabled.
- [ ] TDD (pure): footer selector — toggle off → render $ cost; toggle on → render limit gauges.
- [ ] TDD: endpoint failure → degrade to $ cost + a quiet notice (never a blank/wrong gauge).
- [ ] Manual: with a real token, gauges match `claude.ai/settings/usage`.

---

## M5 — Liveness + prune

**Files:** `src-tauri/src/platform/liveness.rs`, `src-tauri/src/core/prune.rs`

- [ ] TDD (`prune`, pure): events older than 30d dropped; boundary exact.
- [ ] `liveness`: `sysinfo` — is pid alive? Dead session → hidden from list, cost
  still counted. TDD the *filter* logic with an injected `is_alive` fn (keep the
  syscall behind a trait so the rule is testable without real processes).

---

## M6 — UI implementation (wire live core → the M2 design)

**Goal:** Render real `fold` output in the already-approved M2 look. Tray badge
(count of waiting sessions), pinned panel, always-on-top.

**Verification:** `agent-browser` QA — screenshot the running app against the same
scenarios as the M2 mock; diff intent. This is where QA-as-user happens. Not TDD.

**Tasks (expanded when reached):** Tauri commands exposing `sessions()` +
`cost(window)`; frontend subscribes to watcher events; tray icon + badge; timeline
filter; pin/dismiss wired to config.

---

## M7 — Focuser: trait + tiers + honest capability detection

**Files:** `src-tauri/src/platform/focuser.rs`, `focuser_macos.rs`

- [ ] TDD (pure): given a session's terminal kind, tier **selection** picks the
  right strategy (tmux → iTerm2 → VS Code `code <path>` → clipboard fallback).
- [ ] TDD (pure): capability detection → when focus is unavailable, `focus()`
  returns a `Degraded(reason)` the UI can render (disabled row + tooltip).
- [ ] Manual: verify each tier actually raises the right window on a real Mac
  (tmux pane, iTerm2 tab, VS Code folder window). Record which tiers pass.

**Note:** correct-but-coarse (VS Code folder-level) beats precise-but-wrong.
Never raise a guessed window.

---

## M8 — Notifications + config + pin/dismiss + project visibility

**Files:** `src-tauri/src/config.rs`, notification glue

- [ ] TDD: notification fires on transition **into** `WaitingOnYou` only; never on
  `Idle`/`Working` (dedupe on repeated states).
- [ ] TDD: config load/save round-trip; sane defaults when file absent.
- [ ] Config surface stays short & opinionated: notifications, project visibility,
  terminal/focus prefs (per spec §2). Pin/dismiss persist in config.

---

## M9 — Hook-install UX

**Goal:** Never auto-write `~/.claude/settings.json`. Show the snippet + copy
button + **live "hooks detected ✓/✗"** indicator.

- [ ] TDD: detection logic reads settings and reports installed/not for each hook.
- [ ] Manual: paste snippet → indicator flips to ✓ within one watch cycle.

---

## M10 — Packaging + README + demo capture

- [ ] Signed/notarized macOS build launches from a clean machine.
- [ ] Add Windows/Linux CI jobs as **allowed-to-fail**, README marks them
  "architecturally supported, untested — help wanted" (matches spec §4).
- [ ] Record the ~30s demo: waiting session surfaces → click → window snaps to it.
- [ ] README: real screenshot from M6, GIF from the demo.

---

## Out of scope (from spec §2, do not build)

Approve-from-panel (v2), charts/budgets/quota, Codex/Gemini/Cursor, history >30d,
Windows/Linux *tested* binaries, wide-open configurability.

## Review

Per this repo's method, the plan takes a **human + AI review** at the gate before
TDD begins. The AI plan-review subagent is available but not auto-run (standing
rule: no subagents unless asked). Human approval of this plan is the gate.
