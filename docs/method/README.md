# The Method — how this repo is built

Pervigil is built with an explicit, spec-first, review-gated AI-assisted
workflow. This directory is not a description of that workflow — it is the
**residue** of running it. Each stage deposits a real artifact; the git history
records the order.

If you want to verify the claim "I build software with a disciplined AI
workflow," you don't have to take my word for it. Read the commits, read these
docs, and reproduce it.

---

## The pipeline

| # | Stage | Tool | Artifact | Gate |
|---|-------|------|----------|------|
| 0 | Understand the problem | — | [`00-understand.md`](00-understand.md) | — |
| 1 | Gather context | web + `context7` | [`01-context.md`](01-context.md) | — |
| 2 | Brainstorm | `superpowers:brainstorming` | dialogue → spec | — |
| 3 | Spec | `superpowers:brainstorming` | [`../specs/2026-07-23-pervigil-design.md`](../specs/2026-07-23-pervigil-design.md) | **human + AI review** |
| 4 | Plan | `superpowers:writing-plans` | [`../plans/2026-07-23-pervigil-plan.md`](../plans/2026-07-23-pervigil-plan.md) | **human + AI review** |
| 5 | Implement | `superpowers:test-driven-development` | `src/` + tests *(not started)* | tests green |
| 6 | QA | manual + `agent-browser` | screenshots / notes *(not started)* | works as a user |
| 7 | Code review | `code-review` skill + self | review notes *(not started)* | issues resolved |
| 8 | PR | git | pull request *(not started)* | **colleague review** |

## Live status

```
[x] 0  Understand      — docs/method/00-understand.md
[x] 1  Context         — docs/method/01-context.md
[x] 2  Brainstorm      — ran superpowers:brainstorming (one-question-at-a-time)
[x] 3  Spec written    — docs/specs/2026-07-23-pervigil-design.md
[x] 3  Spec APPROVED   — human review gate passed (2026-07-23)
[x] 4  Plan written    — docs/plans/2026-07-23-pervigil-plan.md
[ ] 4  Plan APPROVED   — ← WE ARE HERE (awaiting human approval gate)
[ ] 5  TDD implementation
[ ] 6  QA
[ ] 7  Code review
[ ] 8  PR + colleague review
```

**Nothing below the current line has been faked.** There is no `src/`, no test
suite, and no plan yet, because those stages have not run. When they do, their
artifacts land here and the checklist advances. That status-accuracy is the whole
point: the repo is proof precisely because it never claims a stage it hasn't
reached.

## Reference

- [`02-sota-alignment.md`](02-sota-alignment.md) — the method mapped against the
  2026 five-layer stack (SDD, evals, guardrails, observability, orchestration),
  marking honestly where this repo demonstrates each layer and where it doesn't.

## Why a session monitor, built this way

The recursion is deliberate. Pervigil is a tool for staying on top of many
parallel AI-coding sessions. Building it *with* the disciplined AI workflow it's
meant to support makes the repo a single, coherent argument: here is how I work
with AI agents, and here is the thing that workflow produced.
