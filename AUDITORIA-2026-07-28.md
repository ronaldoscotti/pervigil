# External audit, 2026-07-28

An outside read of this repo, done to decide whether it can be shown to hiring managers. Overall
verdict first, because it changes how you should read the rest: **this is roughly 8 out of 10 Rust.
It would pass review on an experienced Rust team with comments, not with a rejection.** Zero
`unwrap()` in production across 4,863 lines, one `unsafe` in the right place, a pure core with a
declared and respected import boundary, 142 tests including two that test genuinely hard things.

Everything below is what to fix. Nothing below contradicts the verdict above.

## How to use this document

**Verify each finding before changing anything.** Line numbers come from a snapshot taken on
2026-07-28 against `main` plus the open `feat/update-affordances` branch. Code moves. If the line
doesn't say what this document claims, investigate before editing.

**Do not "fix" the things listed under [Leave alone](#leave-alone).** Several correct decisions in
this repo look like smells to an agent optimizing for pattern-matching, and undoing them would make
the code worse.

Work top to bottom. P0 is a real defect. P1 is the repo contradicting its own thesis, which matters
more here than in a normal project because honesty of status is the product. P2 is the list a senior
Rust reviewer would open with.

---

## P0 — a real defect

### 1. Config write failure is swallowed on every settings change

`src-tauri/src/app.rs:188`

```rust
let _ = config.save(&self.home.join(CONFIG));
```

This sits inside `update()`, which is the path for every settings mutation. If the write fails, the
user changes a setting, sees the UI accept it, and loses it on the next start with no signal
anywhere.

This is the one finding that directly contradicts `CLAUDE.md`'s own rule, "degrade, don't fake. When
a platform blocks a capability, the tool says so and falls back. It never pretends." A silent
`let _ =` on a persistence path is exactly pretending.

**Fix.** Propagate or surface it. Either return the error from `update()` so the Tauri command layer
can report it, or log it and set a flag the UI can render. Pick one and add a test that a failing
save is observable. Do not leave it silent.

### 2. The other sixteen `let _ =` were never audited individually

Roughly sixteen more `let _ =` exist, mostly Tauri UI side effects like `tray.set_icon` and
`window.set_always_on_top`. Those are defensible. They were never reviewed one by one, so the
category currently carries finding 1 with it.

**Fix.** Walk all of them. For each, either leave it with a one-line comment saying why the failure
is not actionable, or handle it. The goal is that `let _ =` in this codebase means "audited and
intentional" rather than "not thought about yet".

---

## P1 — where the repo contradicts its own claims

The thesis of this project is that the repo is a faithful record of the workflow, not a description
of it. These three items are places where the record and the description disagree. They are cheap to
fix and expensive to be caught on.

### 3. Stage 9 never happened

`CLAUDE.md` lists nine stages and says "Gates are real. Do not skip a human-review gate to move
faster. The proof this repo offers is that the gates were honored." Stage 9 is "PR → colleague
review, human review before merge."

There are 82 commits from one author and 13 merged pull requests approved by that same author. No
colleague reviewed anything. `docs/method/README.md` marks stages 7 and 8 as partial, which is
partial honesty, but `CLAUDE.md` itself still reads as a kept promise.

**Fix, pick one and only one.**

Either get a real external reviewer on the next pull request and keep the claim, or amend `CLAUDE.md`
to state what actually happens today, something like "stage 9 is aspirational on this repo, which has
had a single author; every pull request so far was self-reviewed plus AI-reviewed." Then say when
that changes.

The second option costs nothing and strengthens the repo, because a document that admits its own gap
is worth more than one that quietly overstates.

### 4. Two of three CI platform jobs cannot fail

`.github/workflows/ci.yml` runs `cargo test` on Linux and Windows with `continue-on-error: true`.
The comment in the workflow is honest and the README is honest. A reader glancing at three green
platform checks is not going to read either.

**Fix.** Rename those jobs so the non-blocking status is visible in the checks list, for example
`linux (non-blocking, untested target)`. One line, removes the misread entirely.

### 5. Forty percent of commits have no body, against the repo's own rule

33 of 82 commits carry no body, and several with "with rationale" in the subject have an empty body
because the rationale went into a doc instead.

**Fix.** Either relax the rule in `CLAUDE.md` to say the rationale may live in the milestone doc as
long as the commit names it, or start writing bodies. Right now the rule says one thing and the
history does another, which is the same class of problem as finding 3.

### 6. The workflow is described but not reproducible

There is no `.claude/` in this repo. The skills the method depends on, `superpowers:brainstorming`,
`superpowers:writing-plans`, `superpowers:test-driven-development`, `agent-browser`, `code-review`,
are referenced by name in `CLAUDE.md` and live nowhere in the tree. Nobody can clone this and run the
process it documents.

For a repo whose entire pitch is "faithful record of the workflow", this is the biggest structural
gap in the argument.

**Fix.** At minimum, add a section to `CLAUDE.md` naming where each skill comes from and what version,
so the setup is reconstructible. Better, vendor or submodule the ones that are yours, and link the
ones that are not.

---

## P2 — what a senior Rust reviewer opens with

Ordered by how fast they would find it.

### 7. There is no domain error type

No `thiserror`, no `anyhow`. Domain failures collapse into `Option` or into
`io::Error::other(String)`, which cannot be matched programmatically.

The sharpest example is `src-tauri/src/io/record.rs:26-41`. `build_event` returns `Option<Event>`,
so "malformed JSON", "missing session_id" and "unknown kind" are indistinguishable to the caller and
to the operator reading logs.

For a five thousand line app this is a defensible choice. It is not defensible as an accident.

**Fix, in order of effort.** Cheapest, write it down as a decision in `docs/` so it reads as a choice
and the interview answer is ready. Better, introduce a small `thiserror` enum at the ingestion
boundary only, where the three failure causes are genuinely different and worth logging apart. Do not
sprinkle `anyhow` through the whole codebase, that trades one problem for another.

### 8. The frontend has no safety net

`src/main.ts` is 1,522 lines with zero tests. `package.json` has no test runner and CI runs no lint
or test for the frontend. `tsc` runs during build and that is all.

This is the softest part of the project by a wide margin, and it is 1,522 lines.

**Fix.** Add Vitest and cover the seam that matters, the state to render path and the i18n lookup.
Ten tests on the right seam is enough to stop this being the first thing someone pokes. Add a
frontend job to CI.

### 9. `app.rs` is 895 lines and mixes four concerns

Tauri command layer, snapshot construction, notification dispatch and scheduling all live in one
file. Around 400 of those lines are the test module, so the production half is not as bad as it
looks, and it is still the one modularity weak point in an otherwise well separated tree.

**Fix.** Split along the concerns that are already implicit. Snapshot building and notification
decision are both pure enough to move toward `core/`, which also grows the part of the codebase the
boundary rule protects.

### 10. Eight `Mutex` fields and seventeen `lock().expect()`

`src-tauri/src/app.rs:163-182`. No deadlock found, and the surface for one exists because nothing
documents a lock ordering.

**Fix.** Either collapse into a single `Mutex<AppState>`, which removes lock ordering as a concept,
or use `RwLock` for the read dominant fields and document the acquisition order in a comment on the
struct. The first is simpler and this app does not look contended.

### 11. `fold` is the textbook property test target and has none

`src-tauri/src/core/store.rs` is a pure state machine over an event log. 28 example based tests, zero
generated ones.

`tests/full_day.rs:63-66` already tests the contiguity invariant structurally by walking
`segments.windows(2)`, which is the right instinct against a fixture instead of a generator.

**Fix.** Add `proptest` with three invariants. `fold` is deterministic for the same inputs.
`timeline` covers `[from, to]` with no gap and no overlap for any event sequence. Folding a prefix
then the remainder equals folding the whole. That last one is the one that finds real bugs.

### 12. No tests on `src/tray.rs` or `src/lib.rs`

`src/tray.rs` is 216 lines with zero tests. This is partly mitigated by design, since the pure logic
was extracted into `core/tray.rs`, which has 10. Worth confirming that nothing decision shaped
crept back into the Tauri side.

### 13. Toolchain and supply chain are unpinned

No `rust-toolchain.toml`, no MSRV declared, no `cargo-audit` or `cargo-deny` in CI. The workflow uses
floating `stable`, so a Rust release can break the build with no warning and no way to reproduce
yesterday's build.

**Fix.** Add `rust-toolchain.toml`, declare `rust-version` in `Cargo.toml`, add a `cargo audit` step.
Three small commits.

### 14. The core boundary is enforced by humans, not by the compiler

"core/ never imports io/, platform/, or Tauri. It is pure." The rule is real and it is respected
today. It is also a convention inside a single crate, so nothing stops the next change from breaking
it silently.

**Fix.** Split into a Cargo workspace with `core` as its own crate that does not depend on Tauri.
Then the boundary is a compile error instead of a review comment. This is the highest leverage
architectural change on the list, and it turns a claim in a doc into a guarantee.

### 15. Housekeeping

No `CHANGELOG.md` despite two tagged releases. No `#![deny(...)]` or `#![warn(missing_docs)]` at the
crate root, though `clippy -D warnings` in CI covers part of that. PR #19 on
`feat/update-affordances` is open, so the most recent work is not on `main`.

---

## Leave alone

An agent doing a cleanup pass will want to touch these. Do not.

**The single `unsafe`**, `src-tauri/src/platform/liveness.rs:29`. `libc::kill(pid, 0)` is the correct
tool, it is commented, it is behind the `ProcessCheck` trait, it is gated on `cfg(unix)`, and the
`EPERM` handling is right, which is the detail most implementations get wrong. Replacing this with
`sysinfo` would be a regression, and the rationale is already in commit `2de05ee`.

**The seventeen `Mutex::lock().expect()`.** Mutex poisoning is the canonical accepted `expect`.
Converting these to error propagation adds noise for no safety. Finding 10 is about the number of
mutexes, not about these calls.

**The three build invariant `expect()`** in `pricing.rs:46`, `hooks.rs:73` and `lib.rs:55`. A price
table embedded with `include_str!` that fails to parse is a broken build, not a runtime condition.

**The borrowing in `core/store.rs`.** `HashMap<&SessionId, ...>`, `enum Tick<'a>`, and
`newly_waiting<'a>(...) -> Vec<&'a Session>` are deliberate and correct. An agent "simplifying"
lifetimes here will add clones to the hot path.

**Not being on crates.io.** This is a Tauri application, not a library. Publishing would be wrong.

**The exhaustive `match` in `state_after`** with no `_ =>` arm. Adding a catch all would silently
break the guarantee that a new event variant fails to compile.

---

## Suggested order

One pull request per group, so the history keeps showing the process.

1. Finding 1 and 2. The defect and the audit of its neighbours.
2. Findings 3, 4, 5 and 6. The honesty pass, all documentation, no code.
3. Finding 8. The frontend safety net, because it is the largest untested surface.
4. Finding 14, then 9. Workspace split first, because it changes where the code in 9 should land.
5. Findings 7, 10, 11, 13, 15.

Findings 3 through 6 are the ones to do before showing this repo to anyone, and they cost an
afternoon of writing.
