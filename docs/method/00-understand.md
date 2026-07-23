# Stage 0 — Understand the problem

*Written before any research or code. The point of this stage is to state the
problem plainly and reach a rough approach as if I'd build it by hand — so the
context-gathering that follows has a target.*

## The problem

I run many Claude Code sessions a day, across many projects, in different windows
and IDE tabs. Work falls through the cracks: a session finishes and I don't
notice, or a session is **blocked on my input** and sits idle while I'm heads-down
somewhere else. I lose momentum because I have to go hunting through open windows
to find out where each session stands. There is no single pane of glass.

## Who it's for

Me first — but the honest primary goal is a **portfolio artifact**. It has to read
as staff-level work: a screenshot that lands, a short demo that earns a second
look, and a repo that shows I've genuinely invested in working with AI agents.
The tool is what makes those true.

## The one insight that shapes everything

**"Waiting on you" is urgent; "done" is informational.** These are two tiers, not
one list. Sessions blocked on me must surface loudly and sort to the top. Finished
sessions are a quiet log. If the design optimizes for anything, it optimizes for
*where is my attention owed right now*.

## Rough approach (pre-research, my own first instinct)

- Claude Code already emits lifecycle signals (a session finishes; a session
  waits on input). If those can be captured, the "sensing" layer is close to free.
- The missing piece is **aggregation**: one always-visible surface answering "across
  all sessions, which need me?"
- A pinned, always-on-top panel — feature-light, but polished enough that I
  actually keep it open. Menu-bar-adjacent, glanceable, click a row to jump to the
  session.
- The likely hard part is not sensing — it's **mapping a session to its window** to
  jump there, which differs per terminal/IDE.

## Success criteria

1. At a glance, I can see which sessions are blocked on me, sorted to the top.
2. I can act on that (jump to the session) fast.
3. A screenshot and ~30s demo that read as intentional, staff-level engineering.
4. A repo whose structure and history prove the workflow I claim.

## Known unknowns to resolve in stage 1

- Do lifecycle signals actually exist and expose session id + cwd + pid?
- Does a go-to tool already do exactly this? (Check prior art before building.)
- What does session→window mapping cost, per terminal?
- Is cross-platform feasible, and do I have machines to test it?
