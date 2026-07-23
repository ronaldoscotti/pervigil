# Pervigil

**Cross-platform desktop panel for your Claude Code sessions.** See which agents
are **waiting on you**, at a glance — across every project, in one pinned window.

> *pervigil* (Latin) — ever-watchful; keeping watch through the whole night.

---

## Why

Running many Claude Code sessions across many projects, work falls through the
cracks: a session finishes unnoticed, or sits **blocked on your input** while
you're heads-down elsewhere. Every existing tool in this space is macOS-only and
read-only. Pervigil is:

- **Cross-platform** — macOS, Windows, Linux (one Rust/Tauri core, honest per-OS
  adapters).
- **Organized around the urgent state** — "waiting on you" sorts to the top and is
  the whole point, not one column among many.
- **A timeline of your day** — per-project bands (working / waiting / idle) so the
  dead time is visible.
- **Click-to-focus** — jump straight to the session's window/tab/pane.

*(Full design: [`docs/specs/2026-07-23-pervigil-design.md`](docs/specs/2026-07-23-pervigil-design.md).)*

## Built in the open, by an explicit method

This repo is also a demonstration. Pervigil is built with a disciplined,
spec-first, review-gated AI-assisted workflow — and the repo is the **honest
record** of that workflow, not a description of it. Every stage deposits a real
artifact; the git history records the order; nothing is faked ahead of where the
work actually is.

Read [`docs/method/`](docs/method/) to follow it, or the git log to verify it.

```
[x] Understand the problem      docs/method/00-understand.md
[x] Gather context              docs/method/01-context.md
[x] Brainstorm (superpowers)    → spec
[x] Spec written                docs/specs/2026-07-23-pervigil-design.md
[ ] Spec approved               ← current gate
[ ] Plan → TDD → QA → review → PR
```

The checklist is status-accurate on purpose: there is no `src/` yet because the
implementation stages haven't run. When they do, they land here.

## Status

**Design stage — at the spec-approval gate.** Not yet implemented. This is a
positioning artifact under active construction; the design is committed, the build
follows the method above.

## License

TBD.
