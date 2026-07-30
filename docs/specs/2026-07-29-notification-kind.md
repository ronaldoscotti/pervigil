# Spec — the two kinds of notification, 2026-07-29

**Status:** approved to implement (human, 2026-07-29 — "keep going", with the open
question below left at its assumed answer).

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

### 2. Unknown maps to `Permission`

`permission_prompt` → `Permission`. `idle_prompt` → `Idle`. Anything else, including
a missing field, an old log line, and any type Claude Code adds later → `Permission`.

The two errors are not symmetric. Reading a nudge as a block over-reports, which is
today's behaviour and merely noisy. Reading a block as a nudge hides the one thing
the panel exists to show. So the fallback goes to the side that cannot hide a block,
and a new upstream notification type degrades to noise rather than silence.

Consequence worth naming: an `auth_success` notification will paint a wait it should
not. It decays on the existing 30-minute `WAITING_TTL_SECS`, and it is rare.

### 3. What may clear a wait depends on the kind

- **`Permission`** — only the session's *main* transcript. Approving a prompt fires
  no hook, so a record written after it is the only proof the turn carried on. A
  background agent's records prove nothing about whether you answered: it is a
  different agent, running whether you are there or not.
- **`Idle`** — any activity in the session, an agent's included. The nudge says the
  main loop is idle; an agent writing means the session is not.

This is the whole fix. It needs `Session` and the lane's `activity` to say whether
a record came from a sidechain file, which the scanner knows from the path it opened.

### 4. Sidechain-ness is decided by path, not by content

`Transcript` currently infers it from `isSidechain` on any absorbed line, sticky for
the whole file. That is fragile: one inlined sidechain record in a main transcript —
which older Claude Code wrote, and a resumed old session still holds — would mark the
whole file. The scanner opened `<session>/subagents/agent-*.jsonl` or it did not.

## Test plan

Red first, one test per claim:

1. A `permission_prompt` notification followed by subagent records only → the row
   stays `WaitingOnYou`. (The bug this spec exists to prevent.)
2. The same, followed by main-transcript records → `Working`, as today.
3. An `idle_prompt` notification followed by subagent records → `Working`.
4. A notification with no kind — an old log line — behaves as `Permission`.
5. The lane agrees with the row in all four.
6. A round-trip of every `Event` variant through the log format, kind included.

## Open question for the human gate

An `idle_prompt` notification currently becomes `WaitingOnYou`, which is a
deliberate product decision — `SessionState`'s own doc calls it "a permission prompt
or the away notification". The softer `YourTurn` already exists for "the turn
finished, your move".

Mapping `Idle` → `YourTurn` would be more honest and would fix the reported symptom
at the source, with no reasoning about who cleared what. It also changes the panel's
headline for 166 of 185 notifications a day: fewer amber rows, a smaller tray badge,
no desktop notification when you simply walked away.

This spec assumes **no** — `Idle` stays `WaitingOnYou` and is merely clearable by
any activity — because that preserves the existing decision. Overturning it is a
product call, not a bug fix.
