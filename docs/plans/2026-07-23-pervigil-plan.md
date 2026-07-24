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
- [x] Add the session name row (spec item 13) — **landed in the live UI at M6**, not back-ported
  into `design/index.html`. The mock's job was to lock the direction and it did; editing a frozen
  artifact to match a shipped screen is bookkeeping, not design. `design/README.md` records where
  the two now differ.
- [x] Re-check the lane against the real `Segment` shape once M1 Step 7 lands, so the mock isn't asserting a shape the core can't produce. The mock's eight hand-tuned bands are exactly what
  `timeline()` emits — contiguous, closed at `to`, one state each — and the live lane renders
  `Segment`s straight into that layout with `flex-grow: to − from`.

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

**Investigated 2026-07-24, decision: skip (not deferred — blocked).** On a real machine the
design's assumptions don't hold: there is **no `~/.claude/.credentials.json`** (macOS keeps the
token in the Keychain, behind a permission prompt), and nothing under `~/.claude` caches the 5h/
weekly limit or reset values. The only source is an **undocumented, unstable OAuth endpoint** whose
contract we don't know — building against it means hitting a private Anthropic endpoint with the
user's token on a guessed shape, untestable, exactly the fragility the spec warned of. Not worth it.
`~/.claude/stats-cache.json` *does* hold real historical usage (tokens by model, daily activity),
but that's cumulative spend, not "how much of the rolling limit is left," so it doesn't answer the
question this feature exists for. Left as the $ cost footer. Revisit only if Claude Code exposes the
limits locally or the endpoint becomes known/stable.

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

## M6 — UI implementation (wire live core → the M2 design)  ✅ done

**Goal:** Render real `fold` output in the already-approved M2 look. Tray badge
(count of waiting sessions), pinned panel, always-on-top.

**Verification:** `agent-browser` QA — screenshot the running app against the same
scenarios as the M2 mock; diff intent. This is where QA-as-user happens. Not TDD.

**Everything this milestone renders already exists and is tested** (52 tests): `store::fold`,
`store::timeline`, `store::waiting_share`, `store::merge`, `pricing::cost_in_window`,
`liveness::retain_live`, `transcript::{parse_session, usage_entries}`, `event::parse_log`.
M6 wires them to the locked design in `design/index.html` — it should not need new core logic.

*(That last sentence held for the core. What M6 did need was an `io/` layer able to read
1.3 GB of transcripts without melting — see the scanner note below.)*

**Files:** `src-tauri/src/app.rs` (view model + the Tauri command), `src-tauri/src/io/scan.rs`
(incremental transcript reader), `index.html`, `src/main.ts`, `src/styles.css`.

- [x] **One command, `snapshot(span)`, not three.** `sessions()` + `cost(window)` +
  `timeline(from, to)` would each re-read the event log and re-stat the transcript tree, three
  times per tick, and could tear against one another mid-poll. One command returns one
  internally consistent view of both inputs.
- [x] **Refresh strategy: polling at 1s, no `notify`.** Measured on the real machine —
  **0.5–0.7% CPU, ~95 MB RSS** with 2 176 transcript files on disk. A watcher would have bought
  a dependency and three platform behaviours for a number that is already free. The deferral
  from M3 paid off exactly as intended: the call was made against a real consumer, with a
  measurement.
- [x] **Incremental transcript scanner** (`io/scan.rs`) — the one thing M6 genuinely needed and
  the plan didn't foresee. `~/.claude/projects` is **1.3 GB across 2 176 files** here; re-parsing
  even the *active* files once a second is 50 MB/s of wasted work. Transcripts are append-only,
  so each file is remembered by how many bytes we have consumed and only the tail is parsed —
  and only up to the last complete line, since a transcript is usually mid-write. Cold start on
  a 4h window: **0.82 s**.
- [x] **The span filter bounds transcript discovery, not just the readout.** Files untouched
  since the window start are never opened. The rule the panel states plainly: *it shows the
  window you picked.* Live hook-derived sessions always show regardless.
- [x] Render the design: session list, lane + `% waiting on you`, elapsed timers, cost footer,
  tray icon + badge, filter (`4h · Today · Week`).
- [x] **Session name row** (spec item 13). Line 1 = dot · project · `×N` + branch chip *only when
  a project has multiple live sessions*; line 2 = state label · session name (muted, truncated).
  Elapsed + cost stay in the right column, as in the mock. Tiering finished here too: the
  transcript owns `aiTitle → lastPrompt → branch`, and the **short-id floor moved to the view**,
  because a hook-only session has no transcript to reach it through.
- [x] **Footer**: one window-scoped cost plus the filter control, replacing the mock's
  `Today | This week` pair. Two costs where the filter already scopes one is a duplicated
  number; putting the control next to the figure it scopes makes the filter discoverable and
  costs nothing.
- [x] **Per-session cost is `None` only when *nothing* in the session can be priced** → `—`.
  Blanking the whole figure on any single unpriced entry sounds more honest but isn't usable:
  real transcripts carry stray `<synthetic>` and alias model ids, so the strict rule blanks a
  correct total over rounding dust. `cost_in_window` (the footer) still skips unpriced entries.
- [x] `default-run = "pervigil"` in `Cargo.toml` — latent since M3 added the `record` binary;
  `tauri dev` cannot pick between two binaries and had been failing.
- [ ] Pin/dismiss + the settings surface — **moved to M8**, where the config file that persists
  them is built. `fold` and `sort` already take `ViewPrefs`; M6 passes the default. Wiring a
  control to state with nowhere to live would have to be redone a milestone later.

**QA performed.** Real `~/.claude` (15 sessions, 5 projects) + real events written through the
actual `record` shim, driven by hand exactly as a hook would. Verified: waiting-first ordering,
the `×N` + branch chip appearing only where a project runs several sessions, the short-id floor,
`—` for an unpriceable session, span switching (label, axis, footer, lane), the empty state, and
the hooks-not-installed notice. Screenshots in the M6 QA notes.

**Not yet verified:** the native window chrome and the **tray badge**, which need Screen
Recording permission to capture on macOS. The code is in place and the app runs; the visual
confirmation is outstanding and is *not* claimed.

---

## M7 — Focuser: trait + tiers + honest capability detection  ✅ done (on-screen raise unverified)

**Files:** `src-tauri/src/core/terminal.rs` (the captured hint), `src-tauri/src/platform/focuser.rs`
(types, pure `select`, `Caps`, the `Runner`/`WindowFocuser` traits), `src-tauri/src/platform/focuser_macos.rs`
(the executors); capture wired through `io/record.rs` + `bin/record.rs`, `core/event.rs`,
`core/session.rs`, `core/store.rs`; the click wired through `app.rs` + `src/main.ts`.

- [x] TDD (pure): tier **selection** picks the right strategy — tmux ≻ iTerm2 ≻ VS Code ≻
  clipboard. tmux wins because it's the innermost context; a tier whose binary is missing is
  **skipped, never guessed** (7 tests).
- [x] TDD (pure): capability detection maps `PATH` + platform to `Caps`; `Caps` feeds `select`
  so tier choice is testable without shelling out.
- [x] **Capture terminal context at record time.** Transcripts don't carry it (verified against
  1.3 GB of real ones) and it can't be recovered later, so `SessionStart` gained an optional
  `term` — `program`/`tmux_pane`/`iterm_session`, read from the shim's own environment, which
  *is* the session's. Rides the event log; `fold` carries it onto `Session`. Old log lines and
  no-signal captures serialize to nothing.
- [x] TDD: the executor's argv per tier, behind a `Runner` seam (recording fake) — the same
  injected-boundary pattern liveness uses. `SystemRunner` proven **live** by a real
  `pbcopy`→`pbpaste` round-trip.
- [x] TDD: outcome shaping — a raise needs no follow-up; a copy or a failure both hand back the
  resume command so the user is never stranded.
- [x] **UI**: rows are keyboard-operable buttons (`role`, `tabindex`, Enter/Space) with a tooltip
  naming what a click will do; a toast reports what it did. `agent-browser` QA in
  `docs/qa/2026-07-24-m7.md`.
- [x] **`Degraded(reason)` / disabled row → dropped, deliberately.** The resume command needs only
  the session id, which every row has, so the clipboard tier is a universal floor and every row is
  actionable. Inventing a disabled state that can't occur would be dishonest; instead the tooltip
  states *how precise* the action is. Meets the spec's "tooltip explains" intent more honestly.
- [ ] Manual: each tier actually **raising** the right window on a real Mac. VS Code + clipboard
  verified end-to-end on the dev box; tmux + iTerm2 aren't installed here and the on-screen raise
  needs Screen Recording permission this environment lacks — the argv is unit-tested, the raise is
  real code, manually unverified (same posture as M6's tray badge). Known limit noted in code:
  selecting a tmux pane / iTerm2 tab does not itself foreground a background host terminal.

**Note:** correct-but-coarse (VS Code folder-level) beats precise-but-wrong.
Never raise a guessed window.

---

## M8 — Notifications + config + pin/dismiss + project visibility  ✅ done (banner display unverified)

**Files:** `src-tauri/src/config.rs` (persistence), `store::{newly_waiting, states, drop_dismissed}`,
`app.rs` (commands + notice priming + visibility), `index.html` + `src/main.ts` + `src/styles.css`
(settings sheet, pin/dismiss row controls).

- [x] TDD: notification fires on the transition **into** `WaitingOnYou` only — never on
  `Idle`/`Working`, and dedupe means a still-blocked session never re-fires (`newly_waiting`,
  4 tests). The first snapshot **primes silently**; disabling notifications fires nothing but
  still advances the baseline, so re-enabling doesn't replay a backlog (`notices`, 3 tests).
- [x] TDD: config load/save round-trips; a missing / corrupt / partial file degrades to defaults,
  never panicking — settings must not be able to stop the panel opening (7 tests).
- [x] Config surface stays short & opinionated (spec §2): a notifications switch and a
  project-visibility list — nothing else exposed. Fires via `tauri-plugin-notification`, gated on
  the toggle, computed in the pure pipeline and shown by the command wrapper so `snapshot` stays
  Tauri-free.
- [x] Pin/dismiss persist in config and thread into `fold` as `ViewPrefs`; pin keeps a project on
  top, dismiss hides a session until it next acts. Project visibility filters the **list only** —
  the lane and totals still aggregate every session (spec item 4).
- [x] **Self-review fix:** dismiss was applied only inside `fold`, so a **transcript-only** session
  (added by `merge` afterward) ignored it. Extracted `store::drop_dismissed` and applied it to the
  merged list too. Test covers the transcript case.
- [ ] The native banner **appearing on screen** — wired and unit-tested up to the OS call, but
  confirming the banner needs a granted-permission desktop session this environment can't capture
  (same posture as the tray badge and on-screen raise). `docs/qa/2026-07-24-m8.md`.

**Deferred from spec §2:** terminal/focus preferences. The focuser (M7) auto-detects the tier and
degrades honestly; there is no disagreement for a setting to resolve yet, so exposing one would be a
checkbox without a decision behind it. Add it only if a real need to override the auto-tier appears.

---

## M9 — Hook-install UX  ✅ done

**Goal:** Never auto-write `~/.claude/settings.json`. Show the snippet + copy
button + **live "hooks detected ✓/✗"** indicator.

**Files:** `src-tauri/src/io/hooks.rs` (detect + snippet), `app.rs` (wiring + `record_path`),
`index.html` + `src/main.ts` + `src/styles.css` (install card).

- [x] TDD: `detect(settings_json)` reports installed/not **per event**, not rounded up; degrades to
  "nothing installed" on an empty/corrupt/hook-less file; recognises an install by any absolute
  path (case-robust — see the self-review note). The `snippet(record_path)` we emit satisfies our
  own detector (round-trip test). 5 tests.
- [x] The install card renders only while hooks are missing, so its **disappearance is the live
  "detected" signal**; a "Hooks detected ✓" toast confirms the flip. Snippet is copy-buttoned and
  selectable (honest fallback). Verified with `agent-browser` against the detect→install→vanish
  cycle. `docs/qa/2026-07-24-m9.md`.
- [x] **Self-review fixes:** detection had matched the brand string (missed capital-P app paths);
  the copy fallback pointed at text a global `user-select: none` had made unselectable; and the
  dead M6-era `hooks` field was removed in favour of the single settings-based signal.
- [x] Manual on a real install: hooks are installed in this machine's `~/.claude/settings.json`
  and firing (they are the source of the live event log), and the detector sees them, so the
  install card stays hidden — the detect→vanish cycle is proven end-to-end, not just against a
  stub. Remaining nuance: those hooks still point at the dev target; repointing them at the
  bundled shim path is folded into M10's shim-bundling item below.

---

## M10 — Packaging + README + demo capture  ◑ partly done (blocked on credentials + capture)

**Done autonomously (verifiable without a desktop or secrets):**
- [x] **Windows/Linux CI jobs, allowed-to-fail** (`continue-on-error`), macOS stays the gating
  job. Linux installs the Tauri system deps. README marks the two as "architecturally supported,
  untested — help wanted" (spec §4). `.github/workflows/ci.yml`.
- [x] **README** rewritten to the real state: the M6 screenshot as hero, the two-input
  architecture, the honest cross-platform table, from-source install + the hook-paste step, and a
  "verified honestly" section listing the three OS-surface effects still unconfirmed.

**Blocked — needs the user (a real blockage, not deferrable work):**
- [ ] **Signed/notarized macOS build launches from a clean machine.** Needs an Apple Developer ID
  certificate + notarization credentials (`APPLE_CERTIFICATE`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`). Can't be produced or verified without them.
- [x] **Bundle the `record` shim inside the .app** (`bundle.externalBin` — a triple-suffixed
  binary staged by `scripts/stage-sidecar.sh`, kept in `tauri.bundle.conf.json` so `cargo test`
  stays clean). Verified by a real `tauri build`: `pervigil.app/Contents/MacOS/record` ships next
  to the main binary. M9's snippet can now point at the installed path instead of the dev target.
- [ ] **The ~30s demo** (waiting session surfaces → click → window snaps to it) and the README GIF.
  Needs a real desktop with tmux/iTerm2 and Screen Recording permission — the same capture wall
  that left the tray badge, on-screen raise, and notification banner visually unconfirmed.

---

## Out of scope (from spec §2, do not build)

Approve-from-panel (v2), charts/budgets/quota, Codex/Gemini/Cursor, history >30d,
Windows/Linux *tested* binaries, wide-open configurability.

## Review

Per this repo's method, the plan takes a **human + AI review** at the gate before
TDD begins. The AI plan-review subagent is available but not auto-run (standing
rule: no subagents unless asked). Human approval of this plan is the gate.
