# Spec — audit remediation, 2026-07-28

**Status:** approved (human gate, 2026-07-28).

## Problem

An external read of this repo returned fifteen findings across three tiers: one real
defect, five places where the documentation contradicts the repo's own history, and
nine a senior Rust reviewer would raise.

The second tier is the expensive one. This repo's pitch is that it is a faithful
record of a disciplined workflow, so a document that overstates what happened is the
product failing, not just the prose.

## What we are and are not doing

**In scope:** all fifteen findings, with the audit's "leave alone" list treated as a
constraint — the single `unsafe`, the `Mutex::lock().expect()` calls, the three
build-invariant `expect()`s, the borrowing in `core/store.rs`, and the exhaustive
`match` in `state_after` are correct and must survive unchanged.

**Out of scope:** anything the audit did not raise. This is remediation, not a
refactor tour.

## Design decisions

### 1. Persistence failure is returned, not logged

`App::update` returns `std::io::Result<()>`; every settings command returns
`Result<(), String>`; the panel toasts `settingNotSaved`.

Rejected: a flag on the snapshot for the UI to render. It needs a new field, a new
render path, and a rule for how long the flag stays true. A toast is the signal the
focus path already uses for the same class of event.

### 2. `let _ =` means audited

Each one is handled or left with the reason it cannot be. Where a whole file is one
class of ignorable call — `tray.rs` and its window-server calls — the module doc says
so once instead of repeating it per line.

### 3. `core` becomes its own crate

`src-tauri/core`, package `specola-core`, depending only on `serde`, `serde_json`,
and `chrono`. The boundary rule stops being a review comment and becomes a dependency
list. The app crate re-exports it (`pub use specola_core as core`), so every existing
`crate::core::…` path and both integration tests keep working unchanged.

`chrono` belongs in core because arithmetic over a caller-supplied `now` is pure.
What core must still never have is a *clock* — no `Local::now()` inside it.

### 4. `app.rs` splits four ways

- `core::span` — `Span`, `bounds`, `start_of_day`
- `core::notify` — `Notice`, `notices`, `name`; takes `notifications: bool` rather
  than `&Config`, which is what lets it be pure
- `commands.rs` — the `#[tauri::command]` layer
- `app.rs` — the state and the one snapshot pass

Rejected: splitting the snapshot pass itself. It reads two inputs and folds them
once; halving it means either two passes or a struct that exists only to carry
intermediates between them.

### 5. Lock ordering is documented, not collapsed

The audit offered "collapse into a single `Mutex<AppState>`" or "document the
acquisition order". Collapsing is wrong here: `snapshot` holds `tray` while reading
`strings`, so one mutex would deadlock unless the pass were restructured — a bigger
change than the problem. The order is stated on the struct and matches field order.

### 6. The frontend gets a seam, then tests on it

`main.ts` has no exports and touches the DOM at module scope, so nothing in it can be
imported. Split into `types.ts`, `i18n.ts` (the ten locales and the lookup), and
`format.ts` (the state-to-render path), leaving `main.ts` as DOM and wiring.
`detectLang` becomes pure — it takes the saved value and the browser language instead
of reading `localStorage` and `navigator` itself.

Vitest, no jsdom: the covered seam has no DOM in it.

The locale-parity test is the one that earns its place — it asserts every language
carries exactly the English key set, which is the failure mode ten hand-maintained
dictionaries actually have.

### 7. A small ingestion error type, no new crate

`build_event` returns `Option<Event>`, so "not JSON", "no session id" and "unknown
kind" are indistinguishable. A three-variant enum with a hand-written `Display` is
about fifteen lines. `thiserror` saves ten of them and adds a dependency; the audit's
own warning against sprinkling `anyhow` applies to reaching for `thiserror` at this
size. Scoped to the ingestion boundary only — `Option` elsewhere stays a written
decision rather than an accident.

### 8. Property tests on `fold`

`proptest` as a dev-dependency of `specola-core`, three invariants:

1. `fold` is deterministic for the same inputs.
2. `timeline` covers `[from, to]` with no gap and no overlap, for any event sequence.
3. Folding a prefix then the remainder equals folding the whole.

The third is the one that finds real bugs, and the reason this is worth a dependency.

## Acceptance

- Every finding is fixed, or has a written decision saying why not.
- Nothing on the "leave alone" list changed.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check`, `npm run typecheck`, `npm test` all green.
- No comment added by this pass narrates the line under it.
- The audit file itself is not committed.
