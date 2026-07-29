# Plan — audit remediation, 2026-07-28

Spec: [`../specs/2026-07-28-audit-remediation.md`](../specs/2026-07-28-audit-remediation.md)

**Status:** approved (human gate, 2026-07-28).

One commit per group, in this order, so the history shows the shape of the work.
Groups with behaviour get their test written and failing first.

---

## A — the defect and its neighbours

Findings 1, 2.

- [ ] Red: `Config::save` into a path whose parent is a file must report the error.
- [ ] Green: `App::update` returns `std::io::Result<()>`; the five settings commands
      return `Result<(), String>`.
- [ ] `settingNotSaved` in all ten locales; `set()` in `main.ts` toasts it.
- [ ] Walk all seventeen `let _ =`. Handle or justify each.

## B — the honesty pass

Findings 3, 4, 5, 6. Documentation only; nothing to redden.

- [ ] `CLAUDE.md`: stage 9 stated as aspirational, with what is true today.
- [ ] `docs/method/README.md`: "The gate that is not met".
- [ ] CI job names carry their blocking status.
- [ ] `CLAUDE.md`: commit-body rule matched to what the history does.
- [ ] `CLAUDE.md`: "Reproducing the workflow" — every skill, marketplace, version.

## C — the frontend safety net

Finding 8.

- [ ] Red: `src/format.test.ts` and `src/i18n.test.ts` against modules that do not
      exist yet.
- [ ] Green: extract `types.ts`, `i18n.ts`, `format.ts`; `detectLang` made pure.
- [ ] `npm test` / `npm run typecheck`; a `frontend (gates merges)` CI job.

## D — the structural change

Findings 14, then 9 — that order, because the workspace split decides where the
`app.rs` pieces land. A refactor: the 133 existing tests are the net, and they stay
green at every step.

- [ ] `src-tauri/core` as the `specola-core` crate; `src-tauri/Cargo.toml` a
      workspace root.
- [ ] `pub use specola_core as core` keeps every call site unchanged.
- [ ] `core::span`, `core::notify`, `commands.rs` extracted.
- [ ] Lock ordering documented on `App` (finding 10).

## E — ingestion errors

Finding 7.

- [ ] Red: three tests, one per failure cause, each asserting a distinct variant.
- [ ] Green: `build_event` returns `Result<Event, IngestError>`; `bin/record.rs`
      still exits 0 on every one of them.
- [ ] Record the decision that `Option` elsewhere is a choice, not an oversight.

## F — property tests on `fold`

Finding 11.

- [ ] `proptest` as a dev-dependency of `specola-core`; a generator for event
      sequences.
- [ ] `fold` is deterministic.
- [ ] `timeline` covers `[from, to]` with no gap and no overlap.
- [ ] fold(prefix) then fold(rest) equals fold(whole).

## G — the untested Tauri edges

Finding 12.

- [ ] Read `src/tray.rs` and `src/lib.rs` for anything decision-shaped that belongs
      in `core::tray`. Move it if there is; record that there is none if there is not.
      The honest outcome may be "no change".

## H — toolchain and supply chain

Finding 13.

- [ ] `rust-toolchain.toml`.
- [ ] `rust-version` in both manifests.
- [ ] `cargo audit` step in CI.

## I — housekeeping

Finding 15.

- [ ] `CHANGELOG.md` for v0.1.0 and v0.1.1.
- [ ] Crate-root lints, only where `clippy -D warnings` does not already cover.
- [ ] `CLAUDE.md` "Current position" updated.

---

## Constraint carried through every group

Verify at the end that none of these changed: the single `unsafe` in
`platform/liveness.rs`, the `Mutex::lock().expect()` calls, the three
build-invariant `expect()`s, the borrowing in `core/store.rs`, the exhaustive `match`
in `state_after`.
