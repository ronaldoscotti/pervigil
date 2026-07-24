# Pervigil — Brain

Cross-platform desktop panel that shows every Claude Code session across your
projects, which ones are **blocked on you**, and what your day looked like — at a
glance.

This file is the operating manual for any Claude Code session working in this
repo. It exists because this project is built *by* an explicit AI-assisted
workflow, and the repo is meant to be a faithful record of that workflow — not a
description of it.

---

## The method (non-negotiable order)

Every non-trivial change follows this pipeline. The artifacts each stage produces
live in the repo (`docs/method/`, `docs/specs/`, `docs/plans/`) so the process is
inspectable, not asserted.

1. **Understand** — write the problem down; reach a rough approach as if coding it by hand. → `docs/method/`
2. **Gather context** — user story, prior art, similar code, API/framework docs, an architecture direction. → `docs/method/`
3. **Brainstorm** — `superpowers:brainstorming`; confirm the problem is understood, explore approaches. → dialogue
4. **Spec → review** — write the design spec; review with **human + AI** before moving on. → `docs/specs/`
5. **Plan → review** — `superpowers:writing-plans`; review with **human + AI**. → `docs/plans/`
6. **TDD** — `superpowers:test-driven-development`. Red → green → refactor. Only after the plan is approved.
7. **QA as user + as QA engineer** — exercise the real deliverable; `agent-browser` for any UI (screenshots).
8. **Code review** — run the `code-review` skill, then self-review. Iterate.
9. **PR → colleague review** — human review before merge.

**Gates are real.** Do not skip a human-review gate to move faster. The proof
this repo offers is that the gates were honored.

## Scope discipline (the V2 lesson)

Every feature or change must have a reason to exist: move a metric, prove a
concept, validate an idea. Not "a new framework looks nice," not "everyone talks
about architecture X." Work has to move the needle. The person holding scope here
is the one who once let a team gold-plate for six months — that does not happen
again.

The big-feature pipeline above is for big features. Bug fixes and small changes
use less of it — not because review is relaxed, but because there is less to
review.

## Honesty rules (this is a portfolio artifact)

- **Never fabricate a stage.** No test files for code that doesn't exist, no
  empty `src/` implying TDD ran, no "reviewed" that wasn't. The repo must always
  be status-accurate. A repo that honestly shows work at a gate beats one that
  lies about being finished.
- **The cross-platform seam must be real** where claimed. One `WindowFocuser`
  trait with real per-OS impls and honest capability detection — not `todo!()`s
  behind a cross-platform banner. Claim only what the code earns.
- **Degrade, don't fake.** When a platform blocks a capability (e.g. Wayland
  window activation), the tool says so and falls back. It never pretends.

## Stack & conventions

- **Tauri v2** — Rust core + web frontend. Real tray on macOS/Windows/Linux.
- **`store` is a pure function** `fold(Vec<Event>) -> Vec<Session>` — no clock, no
  fs, no GUI. This is the heart and it is fixture-tested.
- **Event-log file** (`~/.pervigil/events.jsonl`) is the single source of truth,
  fed by hooks via a bundled `pervigil record` binary. No daemon, no socket.
- **Two inputs, separate failure domains:** hooks → state; transcripts → cost.
- The hook shim **must never block or fail a Claude Code turn** — fire-and-forget,
  hard timeout, always exit 0.
- **Comments are the exception, always in English, few and short.** Docblocks on
  public/exported functions; a non-obvious decision or known limit; an opaque
  regex or algorithm. Never section banners, narration of the next line, or
  commented-out code. Try renaming or extracting first — usually that removes the
  need. Rationale for a choice belongs in the commit, the PR, or a doc.

## Git

- Conventional-commit style, developer voice.
- **No AI/Claude/Anthropic attribution** in commit messages, PR titles, or PR
  bodies. Commits read as written by the developer. (User standing rule.)

## Current position

Spec and plan **approved**. Implementation through **M6** (UI wired to the live
core): 70 tests green, panel renders real sessions, cost, and the activity lane.
Next is **M7 — the focuser** (trait + tiers + honest capability detection); no row
is clickable until it lands. See `docs/plans/2026-07-23-pervigil-plan.md` for the
milestone map and `docs/method/README.md` for live status.
