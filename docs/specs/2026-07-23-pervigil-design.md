# Pervigil — Design Spec

**Date:** 2026-07-23
**Status:** Approved — human review gate passed 2026-07-23. Plan: `docs/plans/2026-07-23-pervigil-plan.md`
**Type:** Portfolio artifact (open source) — cross-platform monitor for Claude Code sessions

---

## 1. What it is

> A pinned desktop panel that shows every Claude Code session across your projects,
> which ones are blocked on you, and what your day actually looked like — at a glance.

**Primary goal is positioning, not personal utility.** The deliverable that matters is a
screenshot and a ~30s demo video that read as staff-level engineering. The app exists to make
those true. Every design decision is judged against: *does this make the artifact more credible
and more distinctive?*

### Why this, when a dozen similar tools exist

The category is validated but unowned (the leading tool has ~50 GitHub stars). Every incumbent
is **macOS-only and read-only**. Two wedges, both real searches the incumbents don't answer:

- **Cross-platform** (`claude code cross platform monitor`) — an *architecture* differentiator.
- **"Waiting on you" as the organizing principle** (`claude code waiting for input`) — the urgent
  state sorts to the top and is the whole point, not one column among many.

Cross-platform is a README/credibility differentiator, not a demo differentiator (it doesn't show
up in a screenshot). The video still has to win on craft and the timeline.

---

## 2. Scope

### v1 (tested platform: macOS)

1. **Session discovery** — every active session, auto-detected. No per-project setup.
2. **Three states** — `working` / `waiting on you` / `idle`. Waiting always sorts to the top.
3. **Session list** — all sessions ordered by last-active. Order: waiting-on-you → user-pinned →
   the rest by recency. Dead sessions (process gone) are hidden entirely, but their cost still
   counts toward totals.
4. **Activity lane** — one *combined* band across all sessions (working / waiting / idle), with a
   `% waiting on you` stat. The visual centerpiece. Filter: `4h · Today · Week`; the same filter
   scopes the cost readout. *(Revised after M2: per-row timelines were cut — a ~380px panel can't
   legibly carry 6h of multi-state history per row. Rationale in `design/README.md`.)*
5. **Elapsed timer** on the current state ("Blocked for 22m" is the line that makes you act).
6. **Click to focus** — jump to the session's window/tab/pane. 4-tier best-available (see §5).
7. **Cost (default footer)** — per session and per day/week. Token counts × a shipped price
   table. Text only, no charts. Zero-dependency — reads no credentials, fully portable.
8. **Native notification** on entering `waiting`. Only on `waiting` — `done` stays quiet.
9. **Pin / dismiss** — pin keeps a project at the top; dismiss self-clears when the session next
   acts. No archive (the sort already solves the dead-session problem).
10. **Project visibility** — choose which projects appear.
11. **Settings panel** — short and opinionated (see §4).
12. **Hook install** — show the snippet + copy button + live "hooks detected ✓/✗" indicator.
    Never auto-writes to `~/.claude/settings.json`.
13. **Session name** — each row shows Claude Code's own AI-generated session title (the
    `aiTitle` record in the transcript, same as `claude --resume` shows), tiered fallback:
    `aiTitle` → `lastPrompt` → branch → short session id. Tells you *what* a session is doing,
    not just where it lives. Truncated; never treated as authoritative (titles can lag).
14. **Usage limits (opt-in)** — a settings toggle (default **off**) switches the footer from $
    cost to account-accurate **5-hour + weekly limit gauges** with reset timers, via the OAuth
    endpoint the CLI uses (reads `~/.claude/.credentials.json`). Off by default because it's a
    fragile, undocumented dependency that touches credentials; enabling it is the user's explicit
    consent to that trade. Degrades back to $ cost if the endpoint is unavailable or changes.
    *(Added after M2 prototype feedback — most Claude Code users are on subscriptions where a plan
    limit, not dollars, is the real attention signal. See `docs/method/` for the loop.)*

### Explicitly out of v1

- Windows / Linux **binaries** — architecture supports them, we don't ship or claim them yet
  (marked "architecturally supported, untested — help wanted").
- **Approve-from-panel** — the v2 wedge (monitor → control surface). Needs its own design.
- Charts, budgets. (Account usage-limit gauges are **in**, but opt-in — never the default; item 14.)
- Codex / Gemini / Cursor session support.
- History beyond 30 days.
- Wide-open configurability.

### Config philosophy

Configurable when reasonable people genuinely disagree; opinionated default otherwise. v1 exposes:
notification behavior, project visibility, terminal/focus preferences, and the usage-limits opt-in
(item 14). Not exposed: timeline
colors, sort order, retention window. Rule of thumb stated for interviews: *"I'd rather ship sane
defaults with an escape hatch for the two things people actually disagree about than expose 40
checkboxes."*

---

## 3. Architecture

```
Claude Code session
   │  SessionStart / Notification / Stop / UserPromptSubmit hooks
   ▼
hook shim ──► `pervigil record` (bundled CLI, one small Rust binary)
                    │ atomic append
                    ▼
        ~/.pervigil/events.jsonl        ← single source of truth
                    │
   ~/.claude/projects/**/*.jsonl        ← second input: tokens → cost
                    │
                    ▼
              Rust core  ── watcher → fold → state
                    │
                    ▼
         Tauri v2 UI (panel + tray badge)
```

**Stack:** Tauri v2 (Rust core + web frontend). Real system tray on all three OSes, small binaries,
full UI freedom for the "looks nice" requirement.

### Key decisions & rationale

**Event-log file as source of truth — not a socket/daemon.** The app is not always running.
The core case (closed laptop, session left blocked) requires state to survive app restarts,
crashes, and reboots. An append-only file + fold-on-launch reconstructs exact state with no
special-casing, no port, no IPC. Identical on all three platforms.

**Hook calls a bundled binary, not `echo >>`.** A shell one-liner means three shims (bash / cmd /
PowerShell) with different quoting and atomicity. One small Rust binary invoked identically
everywhere removes that surface and guarantees atomic appends + log rotation. This is the concrete
point where "cross-platform" becomes an architecture decision rather than a claim.

> **Non-negotiable:** the shim must never block or fail a Claude Code turn. Fire-and-forget, hard
> timeout, always exit 0. A monitor that can wedge the thing it monitors is worse than no monitor.

**Two inputs, deliberately separate.** Hooks → *state* (exact; `Notification` is the only honest
signal for "blocked on you right now"). Transcripts → *cost + session title* (tokens × shipped
price table; `aiTitle`/`lastPrompt` records). They
answer different questions and fail independently: no hooks installed → sessions + cost still work,
state degrades to `idle` rather than breaking.

### Core modules

| Module     | Responsibility                                   | Depends on |
|------------|--------------------------------------------------|------------|
| `record`   | append hook event, atomically                    | —          |
| `watcher`  | tail events.jsonl + transcript dir               | fs notify  |
| `store`    | `fold(events, now, prefs)`, `timeline(events, from, to)`, `merge()` — **pure** | — |
| `pricing`  | tokens × table → cost                            | price JSON |
| `focuser`  | `trait WindowFocuser` + per-OS impls             | OS APIs    |
| `liveness` | is this session's process alive?                 | OS APIs    |
| `config`   | load/save settings                               | —          |
| `prune`    | drop events older than 30 days (on launch)       | —          |

`store` is the heart: a pure function over an event slice — no clock, no filesystem, no GUI. This
is what makes the whole thing fixture-testable.

---

## 4. Cross-platform posture

Portable for free (pure data): session discovery, state, timeline, cost — all from
`~/.claude/projects/**` + hooks, same paths everywhere.

Platform-specific, degrading honestly:

| Feature            | macOS          | Windows      | Linux X11   | Linux Wayland |
|--------------------|----------------|--------------|-------------|---------------|
| Tray icon          | ✅             | ✅           | ⚠️ (needs menu) | ⚠️        |
| Click-to-focus     | ✅ AX/AppleScript | ✅ Win32  | ✅ wmctrl   | ❌ blocked by compositor |
| Always-on-top      | ✅             | ✅           | ✅          | ⚠️            |

**Known walls (documented, not hidden):**

- **Wayland forbids programmatic window activation** by design. No workaround. Focus degrades to
  "copy resume command to clipboard."
- **WSL** splits sensing (Linux FS) from focusing (Windows process). Solvable, deferred.

v1 ships **macOS tested**; Windows/Linux are architecturally supported and open for contribution.
The portfolio value is the clean seam (one `WindowFocuser` trait, per-OS impls, explicit capability
detection, honest degradation) — documented, including exactly where it stops working.

---

## 5. Click-to-focus — tiered, best-available

| Tier | Mechanism                                   | Reliability          |
|------|---------------------------------------------|----------------------|
| 1    | tmux `select-window`/`select-pane`          | Exact pane           |
| 2    | iTerm2 AppleScript                          | Tab-level            |
| 3    | VS Code — `code <project-path>`             | Folder-level window  |
| 4    | Fallback — copy resume command to clipboard | Always works         |

Principle: **correct-but-coarse beats precise-but-sometimes-wrong.** Raising the *wrong* VS Code
window (user runs several) breaks trust live, on camera — worse than folder-level focus. Tier 4 is
the universal floor so the feature never fully fails. VS Code (the author's daily driver) is Tier 3.

---

## 6. Failure modes

| Situation                              | Behavior                                        |
|----------------------------------------|-------------------------------------------------|
| Panel closed for hours                 | Fold log on launch → state is exact             |
| Terminal killed, no `Stop` event fired | `liveness` finds no process → marked dead → hidden |
| Hooks not installed                    | Sessions + cost still shown; state → `idle`     |
| Corrupt line in events.jsonl           | Skipped, counted, never fatal                   |
| Wayland / focus unavailable            | Row renders, click disabled, tooltip explains   |
| Unknown model in price table           | Cost shows `—`, never a wrong number            |

A monitor that displays a confidently wrong dollar figure is worse than one that admits it
doesn't know.

---

## 7. Testing

Recorded event sequences → asserted session states. Because `store` is pure, a full day of
multi-project activity is a fixture file + a few assertions — no UI harness, no mocking, no timing
flakiness. Capture real sessions including the ugly cases (killed terminal, session resumed after
hours, two sessions in one project); those become the regression suite.

The pure-core-with-adapters seam + fixture-driven suite is itself a portfolio artifact — it reads
as *this person has done this before*.

---

## 8. Name

**Pervigil** — Latin, "ever-watchful; keeping watch through the whole night."

- Semantic fit: watch that lasts through the night while you wait — the closed-laptop / session-
  blocked-since-11pm case in one word.
- Distinctive: uncommon compound, reads as a real brand. GitHub/npm clear; only collision is a
  defunct 2012 IT company (PerVigil, Inc., absorbed into Möbius Partners) — background noise.
- Hidden layer (invisible on the surface): the *Pervigilium Paschale*, the Easter Vigil — the
  archetypal "stay awake and wait." Identity woven three meanings deep; reads as neutral tech.
- CLI: `pervigil record`. Domains `pervigil.dev` / `.app` expected free.

---

## 9. Open questions for implementation phase

- Exact price table format + update cadence (ship static JSON; how to refresh on model changes).
- tmux session→pane mapping details.
- VS Code multi-window disambiguation limits (accept folder-level for v1).
- Tray-on-Linux quirks (icon needs a menu set; left-click-menu unsupported) — defer to Linux pass.
