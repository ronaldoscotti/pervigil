# Spec — the two kinds of notification, 2026-07-29

**Status:** implemented. Approved 2026-07-29 ("keep going"); the open question below
was then decided the other way, also by the human ("do it"), and a second report
during implementation extended it to `SessionStart`.

## Order of work

Decision 3 splits into two parts, and only the second depends on the open question:

- **A. Origin** — a wait is cleared by the session's *main* transcript, never by an
  agent's file. Required whichever way the open question goes, and enough on its own
  to make PR #25 safe: no missed block, cost still counted, recency still honest. It
  does not fix the reported symptom.
- **B. Kind** — the shim records `notification_type`; an `Idle` wait is clearable by
  any activity. This is what fixes the reported symptom.

## Problem

A session that dispatches a background agent read as **waiting on you** while the
agent was still working. Fixing that (PR #25, first commit) introduced the inverse,
which is worse: a session at a *real* permission prompt, with a background agent
writing beside it, now reads as **working**. The panel's one promise is that it tells
you what is blocked on you, so a missed block is a product failure in a way a false
alarm is not.

Both situations emit the same event. `Notification` is recorded with an id and a
timestamp and nothing else, so `store` cannot tell "Claude needs your permission"
from the "waiting for your input" nudge that fires 60 seconds after the main loop
goes idle. One of the two readings has to be wrong.

Measured over the 185 notifications in this machine's log:

| gap between the notification and the main transcript going quiet | count | what it was |
|---|---|---|
| ≤ 12s | 19 | a permission prompt — the `tool_use` record precedes it |
| exactly 60s | 166 | the idle nudge, on its timer |

Timing separates them, but it is inference about an undocumented constant. There is
direct evidence available instead.

## What we are and are not doing

**In scope:** giving `Event::Notification` a kind, taken from the hook payload;
deciding per kind what may clear the wait it opens; the same rule on both surfaces
(the session row via `merge`/`answered`, the lane via `timeline`/`activity`).

**Out of scope:** whether the idle nudge should be a hard `WaitingOnYou` at all —
see the open question. No change to the hook registration, and no re-install: the
shim already receives this payload.

## Design decisions

### 1. The kind comes from `notification_type`, not from the message text

The `Notification` hook input carries `message`, an optional `title`, and
`notification_type` — documented values include `permission_prompt` and
`idle_prompt`, and the docs also mention authentication and elicitation flows. The
shim reads `notification_type`. Prose is localised and rewritten; a type field is a
contract.

```rust
Event::Notification {
    id: SessionId,
    at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<NotificationKind>,
}

pub enum NotificationKind { Permission, Idle }
```

`Option`, and `#[serde(default)]`, because the log retains 30 days and every line
already written has no kind.

### 2. The writer records what it saw; the reader defaults

`permission_prompt` → `Permission`. `idle_prompt` → `Idle`. Anything else — a missing
field, a type Claude Code adds later — is recorded as **absent**, not guessed at. The
log must not contain a claim the payload never made; a reader a year from now cannot
tell a real `Permission` from a shim's guess, and the log is the only record there is.

Reading an absent kind is where the default lives, and it goes to `Permission`.

The two errors are not symmetric. Reading a nudge as a block over-reports, which is
today's behaviour and merely noisy. Reading a block as a nudge hides the one thing
the panel exists to show. So the default goes to the side that cannot hide a block,
and a new upstream notification type degrades to noise rather than silence.

Consequence worth naming: an `auth_success` notification will paint a wait it should
not. It decays on the existing 30-minute `WAITING_TTL_SECS`, and it is rare.

### 3. What may clear a wait depends on the kind

- **`Permission`** — only the session's *main* transcript. Approving a prompt fires
  no hook, so a record written after it is the only proof the turn carried on. A
  background agent's records prove nothing about whether you answered: it is a
  different agent, running whether you are there or not.
- **`Idle`** — ~~any activity in the session, an agent's included~~. **Superseded by
  decision 5:** an `Idle` notification stopped opening a wait at all, so there is
  nothing here to clear.

It needs the lane's `activity` to say whether a record came from a sidechain file,
which the scanner knows from the path it opened.

### 4. Sidechain-ness is decided by path, not by content

`Transcript` currently infers it from `isSidechain` on any absorbed line, sticky for
the whole file. That is fragile: one inlined sidechain record in a main transcript —
which older Claude Code wrote, and a resumed old session still holds — would mark the
whole file. The scanner opened `<session>/subagents/agent-*.jsonl` or it did not.

### 5. The idle nudge is not a wait at all — decided yes, 2026-07-29

The question put to the human was whether `idle_prompt` should keep becoming
`WaitingOnYou` (merely clearable by any activity) or map to the softer `YourTurn`
that already exists. Answer: `YourTurn`.

What it costs: the panel's headline changes for 166 of 185 notifications a day —
fewer amber rows, a smaller tray badge, and no desktop notification when you simply
walked away from an idle session. That was accepted.

What it bought: the wait is never opened, so nothing has to reason about who may
close it. `Session::wait` and the kind-aware branch in `answered` were both deleted;
what survives is one rule, stated once and mirrored on both surfaces —

> A record in the session's own transcript can end a wait. An agent's records never
> can, but they do prove the session is not sitting quiet.

The lane paints three states and has no `YourTurn`, so the nudge reads `Idle` there
and `YourTurn` on the row. Both say: not blocked.

### 6. The same shape, on `SessionStart`

Reported while the above was being implemented: a project opened and a session
resumed, nothing typed, and the row read `Working` for twenty minutes. Same root as
decision 1 — the event vocabulary was too coarse and the shim was dropping the field
that disambiguates.

`SessionStart` fires for `startup`, `resume`, `clear` and `compact`. Only the last
happens mid-turn. So the shim records `source`, folded to one of two things Specola
cares about (`Opened`, `Compact`), and:

- `Opened` → `Idle`. A live session at its prompt with nothing running.
- `Compact` → `Working`. Compaction interrupts a turn already in flight.
- absent → `Working`, the old reading, because 30 days of lines have no source and
  neither error hides a block.

## Test plan

Red first, one test per claim. What was written:

1. A permission prompt followed by an agent's records only → the row stays
   `WaitingOnYou`. *(The bug this spec exists to prevent — watched failing first.)*
2. The same, followed by main-transcript records → `Working`, as before.
3. An idle nudge → `YourTurn`; a permission prompt → `WaitingOnYou`; no kind →
   `WaitingOnYou`.
4. An agent's records make a `YourTurn` or `Idle` session `Working`.
5. The lane agrees with the row on both notifications.
6. `SessionStart`: `Opened` → `Idle`, `Compact` → `Working`, absent → `Working`.
7. The shim maps `notification_type` and `source`, and records neither when the
   payload names something this version does not know.
8. Every `Event` variant round-trips through the log format, new fields included.
9. End to end through `App::snapshot`, against a temp `HOME`: the reported agent
   session (`tests/background_agent.rs`) and the reported resumed session
   (`tests/opened_session.rs`).

## Limits closed after review

Three things this design first shipped as documented gaps. All are fixed.

### An agent's records never speak for the session's state — decided 2026-07-30

The first version of this let an agent's records move a `YourTurn` or `Idle` session to
`Working`, bounded by a TTL. Review flagged it as a widening of "a session that stopped
must stay stopped" through a channel that did not exist before, and asked for a
conscious decision. The decision was to drop it.

So the rule is one line on both surfaces: **a record in the session's own transcript can
end a wait; an agent's records never speak for the session's state.** They still carry
its recency and its spend — an agent bills to the session that spawned it — but `Stop`
means the turn ended, whatever an agent beside it is doing.

A session running a background agent therefore reads `YourTurn`: your move, nothing
blocked on you, which was the reported complaint. It does not read `Working`, because
the agent is working and the session is not.

What this deleted, all of it existing only to bound the widening: `AGENT_TTL_SECS`, the
expiry refresh on the lane's `Working`, `Wait::working_until`, `Wait::is_an_agents_work`,
the `now` argument threaded into `merge`, and four tests. Net 179 lines removed against
52 added. It also removed a whole bug class — the row recomputes from a clock and the
lane replays ticks, so an agent-inferred `Working` had to be kept in step on two
different mechanisms, and it was not: the lane blinked idle for one gap every ten
minutes of continuous agent work before that was caught.

### An agent's word expires (removed)

*Superseded by the decision above; kept for the record of what the bound was and why it
existed.*

An agent that finishes fires no hook of its own, so one last record left the row green
for the rest of the day — the failure `WAITING_TTL_SECS` exists to prevent, on the
other colour. `AGENT_TTL_SECS` is **10 minutes**, taken from the real distribution
rather than a guess:

| agent transcripts | inter-record gap | per-file worst quiet stretch |
|---|---|---|
| 601 files, 54k gaps | p50 1s, p95 17s, p99 68s | p50 66s, p95 596s |

209 of 601 files have a stretch over two minutes and 63 over five, so anything tighter
would flap while a slow tool call runs. Past the bound the row falls back to what the
hooks said, and the lane decays to idle on the same constant. The common case never
reaches it: when an agent finishes, the main loop wakes and fires its own hook.

### A session known only through its agent keeps its own row

When only an agent's file passed the window floor, the row came from that file — which
carries neither a title nor a cwd by design — and was then dropped as context-less.
The session disappeared from the panel while it was working. `transcripts` now reads a
session's own transcript whenever one of its agents moved, whatever its mtime says, and
the row always comes from there. Keyed by path, so it cannot double-read a file the
walk already found.

### Retained usage is bounded by the window floor

Transcripts are read from byte zero, so every entry a file ever held was kept and
re-cloned into every snapshot, once a second, for as long as the app ran: **73,174
entries against 1,212** with the floor applied.

### The walk became a sweep

The first measurement of this was a proxy that skipped the `stat` per file entry, and it
was wrong by 4.5x. The real cost: `transcripts` spent **14ms of 15ms on `stat`** across
740 candidate files — one per file per poll, at 1Hz, growing with every session ever
created. The bare directory walk was 0.6ms of it.

So the tree is now walked on a sweep (`SWEEP_EVERY`, ten polls) instead of every poll.
Between sweeps the files that are being written are still re-read every poll — a file
stays hot until it has gone `HOT_POLLS` without growing — and the rest are served from
the cache they would have returned anyway.

| | before | after |
|---|---|---|
| scanner, between sweeps | 5.76ms | **186µs** |
| scanner, on a sweep | 5.76ms | 8.48ms (1 poll in 10) |
| full `snapshot`, averaged | 8.26ms | **3.18ms** |

What it costs: a session or an agent whose file nobody was already reading is discovered
up to ten polls late, and only for its cost and title — state comes from the event log,
which is one file and is read every poll regardless.

The ~2ms left in a snapshot is the event log being parsed from scratch each poll, which
predates this work. A filesystem watcher would take the sweep to zero at the price of a
dependency and a thread; the `ponytail:` note at `src/main.ts` records that trade with
these numbers so the next person does not have to measure it again.
