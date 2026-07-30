# Verification — closing a gap that would otherwise reopen every release

Specola had 194 Rust tests and a QA stage in its pipeline, and it still merged a change
nobody had *looked* at. Not because a step was skipped: because every check
that could run stopped at the Rust boundary, and the deliverable is a GUI whose input
is a filesystem. That gap does not close by trying harder next time. It closes by
turning "look at it" into an artifact a machine compares.

This document is the decision, so the next person inherits the reasoning and not just
the files.

## Why it kept happening

Two reasons, and only the first is obvious.

**Nothing exercised the webview.** `cargo test` proves what `App::snapshot` returns.
It says nothing about what the panel draws from it, because the panel is TypeScript on
the other side of an IPC boundary.

**And the real machine could not show the change anyway.** The notification-kind work
changed how `Notification` and `SessionStart` are *read*, and a developer's own event
log holds 30 days of lines written before those fields existed. Opening the app on a
real `$HOME` would have shown the old behaviour and proved nothing. Verification here
was not merely skipped — without a fixture it was impossible.

That second reason is why the fixture tree is the foundation and not a convenience.
`io::home()` reads `$HOME` (`src-tauri/src/io/mod.rs`), so the whole app can be pointed
at a tree we control.

## What we do

**Golden snapshots.** `src-tauri/tests/golden.rs` builds a `$HOME` per scenario, runs
the real `App::snapshot` against a fixed clock, and compares the serialized result to a
committed file in `fixtures/snapshots/`. Four scenarios today: a session blocked on
you, one whose background agent is working, one just opened, and an ordinary day with
four sessions across three projects. They pin the whole shape — states, order, names,
costs, the lane's segments, the waiting share — so an unintended change arrives as a
diff instead of as something to notice by looking.

**The same files are the frontend's fixtures.** `src/render.test.ts` loads the real
`index.html` into jsdom and calls the real `render` with each golden — no IPC mock is
needed, because `render` takes the snapshot as an argument and the app only enters
through its `DOMContentLoaded` listener. The Rust half says what the UI is *given*; the
frontend half says what it *draws*. One artifact, so the two cannot drift into testing
different days.

Asserting against the shipped markup is deliberate, and it paid immediately: the first
draft of these tests used invented class names and failed until they matched the page.
A test that builds its own DOM would have passed and proved nothing.

**Regenerating is deliberate:**

```sh
UPDATE_GOLDENS=1 cargo test --test golden
```

and then **read the diff before committing it**. A red golden that gets regenerated
without being read is worse than no test, because it converts a caught regression into
a committed one. That is the whole risk of this technique and it is worth stating twice.

## The decisions inside it

**Fixture homes are generated, not committed.** The hook log has to carry a live pid:
`retain_live` retires a session whose process is gone, and a committed pid is always
gone. So the tree is written per test from a builder, with `std::process::id()`
substituted in.

**The clock is fixed and in the past.** Every fixture timestamp is an offset from one
constant, so `since`, the lane's bounds and the elapsed columns are deterministic.
Fixture files' real mtimes are always *later* than the fixed clock, which is what keeps
them clearing the scanner's window gate.

**Two fields are redacted.** The `focus` label follows the platform's capabilities, and
`hookSnippet` carries an absolute path to the bundled shim. Neither describes the day,
and two operating systems have to compare the same file.

**`cost: -0.0` in a golden is not a defect.** Rust's `Sum` for floats uses `-0.0` as its
identity, so a window with nothing priced sums to negative zero. `money()` drops the
sign, and `format.test.ts` pins that — "-$0.00" in the footer would read as a refund.
The golden keeps the raw value, because it shows what the frontend is actually handed.

**One caveat about reading goldens from the frontend.** Vite's JSON *import* normalises
`-0` to `0`, while the running app receives its snapshot over IPC and `JSON.parse`
preserves it. So a golden imported as a module is not byte-identical to the same golden
delivered at runtime. Assert on what the page renders, not on the imported value.

## What this does not cover

Three OS surfaces stay unverifiable here and are named in the README rather than
quietly counted as done: the on-screen raise of a tmux pane or iTerm2 tab, the macOS
tray badge, and the notification banner. Goldens cannot see them and neither can CI.
What covers them is a written release checklist and, when there are enough users to
warrant it, a staged rollout — the updater already drafts before publishing, so the
shape is half-built.

## What we deliberately have not built

**WebDriver end-to-end.** Tauri v2's recommended path is WebdriverIO with
`@wdio/tauri-service`, which does support macOS, and it would drive the packaged binary
for real. It costs a dev dependency, a CI job, and a flakiness budget. The trigger for
revisiting: the first bug that reaches a release and would have been caught by clicking
the app — because that is the moment E2E becomes cheaper than the bugs it prevents.

**Screenshot diffing.** Worth it once the visual design stops moving. Today a
deliberate CSS change would turn every baseline red, and a test that is red on purpose
teaches people to ignore it.
