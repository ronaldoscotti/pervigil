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
