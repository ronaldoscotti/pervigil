# SOTA alignment — the method against the 2026 stack

*A cross-cutting reference (not a pipeline stage). It maps the method this repo
follows to the five-layer model the field converged on by 2026, and marks —
honestly — where the repo **demonstrates** a layer versus where it's a gap or a
forward path. Same rule as everywhere here: a layer is only "demonstrated" if
there's a real artifact you can open.*

## The frame

The 2025 term *vibe coding* (prompt → accept → hope) is now the anti-pattern. The
current term is **agentic engineering**: orchestrating agents through a
**research → plan → execute → review → ship** loop with the human as *oversight,
not typist* ([Claude Code best practices](https://code.claude.com/docs/en/best-practices),
[teamday](https://www.teamday.ai/blog/complete-guide-agentic-coding-2026)). That
loop is the method in [`README.md`](README.md).

## Two workflows, kept distinct

A distinction most people blur, and this repo keeps explicit:

- **(a) Building software *with* AI** — the method in this repo (spec-first,
  review-gated, TDD). This is what the pipeline docs cover.
- **(b) Shipping software that *uses* AI** — a product with an LLM inside. This is
  where **evals** (layer 2) stop being optional.

Specola is mostly a **(b)-free** product: its core is deterministic (a pure
`fold` over an event log). That is a deliberate design choice, and it's why most
of Specola's verification is ordinary fixture testing rather than evals. The one
place (b) could appear is a future optional LLM feature (e.g. summarizing a
session) — and *that* is the only place an eval harness would belong.

## The five layers

### 1 — Spec-driven development (SDD)

**What it is.** The spec is the primary artifact; code is the build output ("the
spec is the prompt"). Every major tool shipped a version — GitHub Spec Kit, AWS
Kiro, Claude Code, OpenSpec, BMAD, Tessl. Reported effect: ~10× fewer
regenerate cycles ([SDD 2026 guide](https://thebcms.com/blog/spec-driven-development)).

**In this repo.** ✅ **Demonstrated.** `superpowers:brainstorming` →
[`../specs/2026-07-23-pervigil-design.md`](../specs/2026-07-23-pervigil-design.md),
committed at a human-review gate before any code. The git history shows spec
before implementation, not after.

### 2 — Eval-driven development

**What it is.** Build the LLM system *around evals first*: a curated golden set
(~100 examples), 3–5 metrics, run **in CI**, LLM-as-judge calibrated against human
labels. For agents, separate **tool-calling correctness** (right tool, right args,
right number of steps) from **outcome correctness** (did the full run accomplish
the goal) ([DeepEval](https://deepeval.com/blog/eval-driven-development),
[Confident AI](https://www.confident-ai.com/blog/llm-agent-evaluation-complete-guide)).

**In this repo.** ⚪ **Not applicable yet, by design.** Specola's core is
deterministic, so it uses fixture tests, not evals — the honest tool for the job.
An eval harness enters *only if* an LLM feature is added, and would live in
`evals/` with its golden set. This doc will not pretend Specola has evals it
doesn't need. *(The high-leverage place to demonstrate this layer is a
product that ships an LLM in the hot path — not this one.)*

### 3 — Guardrails (policy-as-code)

**What it is.** Runtime controls *around* the model — deterministic where the
model is probabilistic. Defense-in-depth: input guards → tool/action gating →
output guards → human-in-the-loop → evals as feedback. **Policy-as-code**: an
external engine that can hard-block an action before it runs
([Galileo](https://galileo.ai/blog/best-ai-agent-guardrails-solutions),
[Maxim](https://www.getmaxim.ai/articles/the-complete-ai-guardrails-implementation-guide-for-2026/)).
Backdrop: prompt injection is OWASP **LLM01**, #1 three years running and unsolved
at the model layer — so the 2026 strategy is *containment*.

**In this repo.** ◐ **Partially demonstrated; a named forward path.** The `record`
shim already carries one hard rule — *never block or fail a Claude Code turn*
(fire-and-forget, hard timeout, always exit 0). Today that's a stated constraint;
the forward path is to make it an **explicit, tested policy module** governing what
the hook may write — a small deterministic gate with its own suite. That is
policy-as-code in miniature, and it lands when stage 5 (TDD) reaches the shim.

### 4 — Observability / tracing

**What it is.** Trace every agent step, evaluate in CI, detect distribution drift
post-launch (Braintrust, Galileo, DeepEval).

**In this repo.** ⚪ **Not applicable.** Specola has no agent in its runtime to
trace. Its "observability" is inverted — it's a tool that gives *you*
observability over *your* Claude Code sessions. Worth noting the irony, not worth
claiming the layer.

### 5 — Orchestration & context engineering

**What it is.** Subagents for isolated/parallel work; the "harness" as distinct
layers — memory, hooks, skills, subagents, plugins, MCP — each changing what the
model can see or do ([Anthropic](https://www.anthropic.com/research/claude-code-expertise)).

**In this repo.** ◐ **Partially demonstrated.** The Brain
([`../../CLAUDE.md`](../../CLAUDE.md)) is context engineering: it constrains every
session in this repo to the method. Skills (superpowers) are used explicitly. What
this repo does *not* yet show is subagent/parallel orchestration — a fair gap,
noted rather than hidden.

## Scorecard

```
Layer                              Demonstrated in this repo?
1  Spec-driven development         [x] yes — spec before code, in git history
2  Eval-driven development         [ ] n/a by design (deterministic core)
3  Guardrails / policy-as-code     [~] partial — one rule today, tested gate is the forward path
4  Observability / tracing         [ ] n/a — no runtime agent to trace
5  Orchestration / context eng.    [~] partial — the Brain + skills; no subagents yet
```

The point of this doc is not to score five out of five — a deterministic desktop
utility *shouldn't* need evals or agent tracing, and claiming them would be the
exact dishonesty the method forbids. The point is to show the method was written
by someone who knows the whole map, applies the layers a given system actually
needs, and says plainly which ones it doesn't.
