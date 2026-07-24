# Pervigil Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pinned, cross-platform desktop panel that shows every Claude Code session, surfaces the ones blocked on you, and lets you jump to them — built macOS-first with a portable core.

**Architecture:** A pure Rust core (`fold(events) -> sessions`) with no clock/fs/GUI, fed by an append-only event log that a bundled `record` shim writes from Claude Code hooks. Cost is a second, independent input read from transcripts. A Tauri v2 shell renders the state; per-OS adapters (`WindowFocuser`, `liveness`) sit behind traits with honest capability detection.

**Tech Stack:** Rust, Tauri v2, vanilla-ts frontend, `serde`/`serde_json`, `chrono` (transcript timestamps), `libc` (liveness, unix). Testing: Rust `#[test]` + JSON fixtures.

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
| M3 | Ingestion — `record` shim + transcripts | TDD on parsing; integration test on a temp log |
| M4 | Cost — `pricing` + price table | TDD |
| M5 | Liveness + prune | TDD + real process lifecycle check |
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
│   │   │   ├── store.rs         # fold(events, now, prefs) + timeline(events, from, to) + merge()  ← PURE, the heart
│   │   │   ├── session.rs       # Session, SessionState types
│   │   │   ├── pricing.rs       # tokens × price table -> cost
│   │   │   └── prune.rs         # drop events older than 30d
│   │   ├── io/
│   │   │   ├── record.rs        # hook payload -> Event, atomic append
│   │   │   ├── transcript.rs    # parse ~/.claude/projects/**/*.jsonl -> sessions, title, branch
│   │   │   └── usage.rs         # opt-in: credentials + OAuth usage endpoint (network, not core/)
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
├── index.html                  # Vite entry — Tauri's default layout, not fought
├── src/                        # web frontend (vanilla-ts)
├── design/                     # M2 mockup — the locked visual direction
├── .github/workflows/ci.yml
└── assets/pricing.json         # shipped price table
```

*(Frontend paths corrected during M0: `create-tauri-app` puts the entry at the repo
root with the frontend in `src/`. The plan originally assumed a `ui/` directory;
fighting the framework default bought nothing, so the design mock moved to
`design/` and the tree above reflects what exists.)*

**Boundary rule:** `core/` never imports `io/`, `platform/`, or Tauri. It is pure.
That import boundary is what keeps `store` fixture-testable and is enforced by
review.

---

## M0 — Scaffold, CI, test harness  ✅ done

**Files:**
- Create: `src-tauri/` (Cargo.toml, src/main.rs, src/lib.rs, src/core/mod.rs, tauri.conf.json, capabilities/), `index.html`, `src/`, `.github/workflows/ci.yml`

- [x] **Step 0: Verify the toolchain**

Run: `node -v && npm -v && cargo -V && rustc -V && xcode-select -p`
Expected: all five print a version/path. Tauri needs a Rust toolchain and, on macOS,
Xcode command-line tools. If `cargo`/`rustc` are missing:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

- [x] **Step 1: Scaffold Tauri v2 app**

Run **into a temp directory**, then move the pieces in — scaffolding in place would
overwrite the M2 design artifact:

```bash
npm create tauri-app@latest pervigil -- --manager npm --template vanilla-ts \
  --identifier dev.pervigil.app --tauri-version 2 --yes
```

Then strip the template demo: the `greet` command, its frontend caller, the Vite/Tauri
logo assets, and the `tauri-plugin-opener` dependency (unused — add it back if a later
milestone needs it, and remember `capabilities/default.json` grants its permission).
Expected: `design/index.html` still present.

- [x] **Step 2: Add a trivial pure-core module + failing test**

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

- [x] **Step 3: Run test**

Run: `cd src-tauri && cargo test`
Expected: PASS.

- [x] **Step 4: CI runs build + test on macOS**

`.github/workflows/ci.yml`: on push/PR, `cargo test` + `cargo clippy -- -D warnings`
+ `cargo fmt --check` on `macos-latest`. (Windows/Linux jobs added in M10 as
allowed-to-fail until tested.)

- [x] **Step 5: Commit**

```bash
git add -A && git commit -m "chore: scaffold Tauri v2 app, CI, and pure-core test harness"
```

---

## M1 — The pure core: `store::fold` (crown jewel, full TDD)  ✅ done

The whole product's correctness lives here. `fold` takes a chronological slice of
events and returns the current sessions. No clock, no fs — "now" is passed in.

**Files:**
- Create: `src-tauri/src/core/event.rs`, `session.rs`, `store.rs`
- Test: inline `#[cfg(test)]` + `src-tauri/tests/fixtures/`

- [x] **Step 1: Define the event + session types (failing test first)**

Test in `store.rs`:
```rust
#[test]
fn session_start_then_notification_is_waiting() {
    let events = vec![
        Event::SessionStart { id: "s1".into(), cwd: "/p".into(), pid: 10, at: 100 },
        Event::Notification { id: "s1".into(), at: 200 },
    ];
    let sessions = fold(&events, /*now*/ 250, &ViewPrefs::default());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].state, SessionState::WaitingOnYou);
    assert_eq!(sessions[0].since, 200); // elapsed anchored at the Notification
}
```

- [x] **Step 2: Run — verify it fails to compile/pass**

Run: `cargo test session_start_then_notification_is_waiting`
Expected: FAIL (types/fn undefined).

- [x] **Step 3: Minimal types + fold**

`event.rs`: `enum Event { SessionStart{...}, Notification{...}, Stop{...}, UserPromptSubmit{...} }` with `#[serde(tag="type")]`.
`session.rs`: `type SessionId = String; type Timestamp = u64;`
`enum SessionState { Working, WaitingOnYou, Idle }`,
`struct Session { id: SessionId, cwd: String, pid: Option<u32>, state, since: Timestamp, last_active: Timestamp }`.
**`pid` is optional** — transcript-derived sessions (M3) carry no pid, and M5 must not treat a
missing pid as evidence of death.
`store.rs`: `fn fold(events: &[Event], now: u64, prefs: &ViewPrefs) -> Vec<Session>` — group by id, apply last-writer state transition, set `since`/`last_active`, then sort.
`#[derive(Default)] struct ViewPrefs { pinned: HashSet<SessionId>, dismissed: HashMap<SessionId, Timestamp> }` — pin and dismiss live in config (M8), not in the event log, so they enter `fold` as **data**. This keeps `fold` pure while letting it own the whole sort order.

- [x] **Step 4: Run — PASS**

- [x] **Step 5: Commit** — `feat(core): event/session types + fold happy path`

- [x] **Step 6+: Add one failing test per rule, then satisfy it, then commit.** One rule per cycle:
  - `Stop` → `Idle`; `since` = Stop time.
  - `UserPromptSubmit` after waiting → `Working`.
  - Ordering, three tiers (spec item 3): `waiting-on-you` → `prefs.pinned` → the rest by `last_active` desc.
  - Multiple sessions, same cwd → two distinct sessions (keyed by id, not cwd).
  - Corrupt/unknown event line → skipped, never panics (parse layer returns `Result`; `fold` only sees valid events).
  - Dismissed (`prefs.dismissed[id] = t`): the session is hidden until an event for its id arrives *after* `t`.
  - `now` drives elapsed only; `fold` output is otherwise pure of wall-clock.

- [x] **Step 7: `timeline()` — the data behind the activity lane (spec item 4).**

`fold` answers *what is true now*; the lane needs *what was true, when*. Separate pure function:

```rust
pub struct Segment { pub state: SessionState, pub from: u64, pub to: u64 }

#[test]
fn timeline_collapses_to_aggregate_segments() {
    // one session working, then a second starts waiting -> the window reads Waiting
    // while any session is waiting; Working while any is working; else Idle.
    let segs = timeline(&events, /*from*/ 0, /*to*/ 600);
    assert_eq!(segs.first().unwrap().state, SessionState::Working);
    assert_eq!(segs.last().unwrap().state, SessionState::WaitingOnYou);
    assert_eq!(segs.last().unwrap().to, 600); // always closed at `to`
}
```

`fn timeline(events: &[Event], from: u64, to: u64) -> Vec<Segment>` — aggregate across **all**
sessions (the design settled on one combined lane, not per-row strips; see `design/README.md`).
Precedence at any instant: `WaitingOnYou` > `Working` > `Idle`.
- [x] TDD: segments are contiguous and cover `[from, to]` with no gaps or overlaps.
- [x] TDD: `waiting_share(&segs)` → the "35% waiting on you" stat in the mock.
- [x] TDD: an empty event slice yields one `Idle` segment spanning the window.

The lane and `waiting_share()` aggregate **every** session — including dismissed, dead, and
projects hidden by project visibility (item 10). The lane is a record of your day, not a filtered
view; visibility filters the *list* only. `timeline()` therefore takes no `ViewPrefs`.

- [x] **Step N: Fixture regression test**

Add `tests/fixtures/full_day.jsonl` (a realistic multi-project day: killed
terminal, session resumed after hours, two sessions one project) + expected JSON.
One test folds the fixture and asserts the whole `Vec<Session>`. This fixture is
the artifact that proves the core.

---

## M2 — Design-direction spike (where visual design enters)

**Goal:** Lock the look before building plumbing. This is a portfolio artifact; the
screenshot is the deliverable, so design is de-risked *now*, not at QA.

**Sub-skill:** `frontend-design:frontend-design`.

**Status: done** — ran ahead of M0/M1, deliberately, to de-risk the look before any plumbing.

**Files:** `design/index.html` (self-contained; sample data is inlined rather than loaded from a
`fixture.json`), `design/README.md` (direction, source project, decisions settled).

**Tasks (not TDD — this is design):**
- [x] Produce a static, self-contained mockup with `frontend-design`: session list (waiting-on-you first), the activity lane, elapsed timers, cost footer.
- [x] **Gate: does it read as staff-level?** Iterated four rounds; passed.
- [ ] Add the session name row (spec item 13) — the mock is one revision behind the spec.
- [ ] Re-check the lane against the real `Segment` shape once M1 Step 7 lands, so the mock isn't asserting a shape the core can't produce.

**Verification:** a screenshot exists and clears the bar. Two decisions this milestone
settled — per-row timelines cut, branch chips only when they disambiguate — are recorded in
`design/README.md` so they aren't re-litigated.

---

## M3 — Ingestion: `record` shim + transcripts  ✅ done

**Goal:** Get real events into the log and into the core, without ever blocking a
Claude Code turn.

**Files:** `src-tauri/bin/record.rs`, `src-tauri/src/io/watcher.rs`, `src-tauri/src/io/transcript.rs`,
and `merge()` in `src-tauri/src/core/store.rs` (it's a pure function over two `Vec<Session>` — it
belongs beside `fold`, never in `io/`).

**Interfaces & rules:**
- `record` needs an explicit Cargo target — `bin/record.rs` sits outside `src/`, so add to
  `src-tauri/Cargo.toml` (deferred from M0: the stanza fails to build until the file exists):

  ```toml
  [[bin]]
  name = "record"
  path = "bin/record.rs"
  ```
- `record`: reads hook JSON on stdin/args, appends one atomic line to
  `~/.pervigil/events.jsonl`. **Hard timeout, always `exit(0)`, never panics** —
  a failure here must not fail the host turn.
- ~~`watcher`: `notify`-based tail~~ — **deferred to M6.** A watcher exists to push
  updates to a consumer, and no consumer exists yet; building it now means designing
  against an imagined UI. `parse_log` + `fold` already read the log on demand, and M6
  can choose polling (simpler, no dependency, portable) over `notify` if polling proves
  good enough for a panel that redraws a handful of rows. Adding `notify` before that
  is a dependency and three platform behaviours bought on speculation.
- `transcript`: parse `~/.claude/projects/**/*.jsonl` → per-session token counts **and session
  title** (`{"type":"ai-title","aiTitle":…}`, fallback `{"type":"last-prompt","lastPrompt":…}` —
  verified present in real transcripts). Tiered: `aiTitle` → `lastPrompt` → branch → short id.

**Session discovery without hooks (spec §6, item 1).** `fold` consumes only hook events, so with
hooks uninstalled the list would be empty — but the spec promises sessions + cost still render with
state degraded to `idle`. Transcripts already carry session id, cwd, tokens and title, so they are a
second discovery source. `transcript::sessions()` enumerates them; `merge(hook_sessions,
transcript_sessions)` unions by session id, hook state winning where present.
- [x] TDD: transcript-only session → present, `state == Idle`, cost + title intact.
- [x] TDD: same id from both sources → one session, hook state wins, no duplicate.

**Verification (TDD where pure):**
- [x] TDD: line parser round-trips every `Event` variant; a corrupt line yields
  `Err` and is skipped, not fatal.
- [x] TDD: `record` append is atomic under concurrent writers (write to temp +
  rename, or `O_APPEND` single `write`) — test with N threads appending, assert
  no interleaved/torn lines.
- [x] Integration: write events to a temp log, run `watcher` fold, assert sessions.

---

## M4 — Cost: `pricing` + shipped price table  ✅ done (opt-in usage limits deferred to M8)

**Files:** `src-tauri/src/core/pricing.rs`, `assets/pricing.json`

- [x] TDD: known model → `tokens × rate` correct to the cent.
- [x] TDD: **unknown model → cost `None`**, rendered as `—`, never a wrong number
  (the spec's honesty rule).
- [x] TDD: cost aggregates per session and per time-window (`4h/Today/Week`).

**Opt-in usage limits (spec item 14) — deferred to M8**, where the settings toggle that
gates it actually exists. Building the OAuth reader before its config switch means an
unreachable code path. Design unchanged: Default footer is $
cost (above). A settings toggle enables `io::usage::limits()`, which reads
`~/.claude/.credentials.json` and calls the OAuth usage endpoint for the 5h + weekly bars. It lives
in `io/` (network + fs), never `core/`, and sits behind a trait so the endpoint is mockable and the
whole feature is off unless enabled.
- [ ] TDD (pure): footer selector — toggle off → render $ cost; toggle on → render limit gauges.
- [ ] TDD: endpoint failure → degrade to $ cost + a quiet notice (never a blank/wrong gauge).
- [ ] Manual: with a real token, gauges match `claude.ai/settings/usage`.

---

## M5 — Liveness + prune  ✅ done

**Files:** `src-tauri/src/platform/liveness.rs`, `src-tauri/src/core/prune.rs`.
`sysinfo` was **not** used — it enumerates every process to answer one question. `libc::kill(pid, 0)`
is the whole check; `EPERM` means alive-but-not-ours. Non-unix returns `None` (undeterminable).

- [x] TDD (`prune`, pure): events older than 30d dropped; boundary exact.
- [x] `liveness`: `sysinfo` — is pid alive? Dead session → hidden from list, cost
  still counted. TDD the *filter* logic with an injected `is_alive` fn (keep the
  syscall behind a trait so the rule is testable without real processes).
- [x] TDD: **`pid: None` is not evidence of death — keep the session.** Transcript-derived
  sessions (M3) have no pid; hiding them would silently re-break the hooks-not-installed path.

---

## M6 — UI implementation (wire live core → the M2 design)

**Goal:** Render real `fold` output in the already-approved M2 look. Tray badge
(count of waiting sessions), pinned panel, always-on-top.

**Verification:** `agent-browser` QA — screenshot the running app against the same
scenarios as the M2 mock; diff intent. This is where QA-as-user happens. Not TDD.

**Tasks (expanded when reached):** Tauri commands exposing `sessions()`, `cost(window)` and
`timeline(from, to)`; frontend subscribes to watcher events; tray icon + badge; timeline filter;
pin/dismiss wired to config; the settings panel surface (spec item 11).

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
