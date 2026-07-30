# Specola — Brain

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
9. **PR → review before merge** — self + AI review today; colleague review is the goal, see below.

**Gates are real.** Do not skip a human-review gate to move faster. The proof
this repo offers is that the gates were honored.

**Stage 9 is aspirational here.** Specola has had a single author: every pull
request so far was self-reviewed, AI-reviewed, and merged by the person who
opened it. The mechanism is in place — work lands through PRs, never a push to
`main` — but no colleague has reviewed one. That changes the day an outside
reviewer approves a PR here; until then this paragraph stands.

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
- **Event-log file** (`~/.specola/events.jsonl`) is the single source of truth,
  fed by hooks via a bundled `specola record` binary. No daemon, no socket.
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
- **A body is optional, a findable rationale is not.** About 40% of commits here
  have no body, and that is allowed when the *why* already lives in a spec, plan,
  or milestone doc the subject points at. What is not allowed is a subject that
  promises a rationale — "with rationale", "because …" — over an empty body.
- **No AI/Claude/Anthropic attribution** in commit messages, PR titles, or PR
  bodies. Commits read as written by the developer. (User standing rule.)

## Releasing

Releases are cut by **release-please**. Nothing is bumped or tagged by hand.

1. Merge work into `main` with conventional-commit subjects — the subject *is* the
   changelog line, so write it for a reader of `CHANGELOG.md`.
2. release-please keeps a `chore: release X.Y.Z` PR open, accumulating those subjects
   and bumping `package.json` and `src-tauri/tauri.conf.json`.
3. Merging that PR tags the version and calls `release.yml`, which builds the signed
   mac/Windows/Linux bundles and publishes the release.

Four things that are easy to get wrong:

- `feat:` bumps the minor and `fix:` the patch. Nothing else bumps anything.
- The release is **drafted while the bundles build, then published by a final job**.
  It becomes `latest` the moment it stops being a draft, and
  `releases/latest/download/latest.json` is what every installed client updates
  from — so publishing before the matrix finishes would expose a half-uploaded
  release, and a failed job would leave that manifest incomplete. If a gating job
  fails the draft is simply left behind and `latest` stays where it was.
- A tag pushed with `GITHUB_TOKEN` does not start another workflow. That is why
  release-please calls `release.yml` directly rather than relying on
  `on: push: tags`, which stays only as the manual escape hatch.
- The two `Cargo.toml` versions are deliberately left alone: neither crate is
  published, and release-please cannot update `Cargo.lock` beside them, so bumping
  them would strand the lockfile.

## Reproducing the workflow

The method leans on skills that are **not in this tree** — they live in the
author's Claude Code configuration, and `.claude/` is not committed because it
also carries machine-local settings. Naming them so the setup can be rebuilt:

| Referenced as | Source | Version here |
|---|---|---|
| `superpowers:brainstorming` | `superpowers` plugin, marketplace `obra/superpowers-marketplace` | 3.2.3 |
| `superpowers:writing-plans` | same plugin | 3.2.3 |
| `superpowers:test-driven-development` | same plugin | 3.2.3 |
| `code-review` | `code-review` plugin, marketplace `anthropics/claude-plugins-official` | tracks the marketplace |
| `agent-browser` | the `agent-browser` npm CLI, driven by a thin local skill | latest |
| `context7` | [Context7 MCP server](https://github.com/upstash/context7) | latest |

```
/plugin marketplace add obra/superpowers-marketplace
/plugin install superpowers@superpowers-marketplace
/plugin marketplace add anthropics/claude-plugins-official
/plugin install code-review@claude-plugins-official
```

## Current position

Implementation through **M10 and beyond**: **198 Rust tests + 32 frontend tests
green**. Signed + notarized macOS build; **auto-updating, signed releases from CI**
(tag → mac/Windows/Linux bundles + updater manifest, proven end-to-end); ten UI
languages with RTL; launch-at-login; single-instance; a dismiss "read" mode; and a
share-your-day card.

`core` is its own crate (`specola-core`), so the purity boundary is a dependency
list rather than a convention. The toolchain is pinned; `cargo audit` and a
frontend job run in CI.

The core (M0–M10) followed the full spec→plan→TDD→review pipeline; the post-launch
features used a faster TDD + agent-browser QA + reviewed-PR loop (a written spec for
the release/auto-update, audit-remediation, and notification-kind work — no back-dated
specs). Three OS-surface effects stay visually unverified on this box (tmux/iTerm2
raise, tray badge, notification banner). See `docs/plans/2026-07-23-pervigil-plan.md`,
`docs/specs/2026-07-28-audit-remediation.md`,
`docs/specs/2026-07-29-notification-kind.md`, and `docs/method/README.md`.
