# Spec — Rename Pervigil to Specola

Status: **draft** (review gate: this PR). Follows the pipeline in
[`../method/README.md`](../method/README.md).

## Problem

The name `Pervigil` fails at three things a product name has to do:

1. **It has no fixed pronunciation.** per-VIH-jil or PER-vi-gil — a name people
   can't say out loud can't be recommended out loud.
2. **`vigil` reads funereal in English.** Candles, mourning, a body watched
   overnight. Wrong register for a tool you keep open during a working day.
3. **It is an adjective.** *Pervigil* means "sleepless, watchful". Products are
   nouns; an adjective as a name leaves nothing for the reader to hold onto.

The cost of fixing it is at its historical minimum and rises with every release:
**no published tags, no releases, 0 forks.** There is no installed base, so a
rename today is a repo-local operation. After the first shipped tag it becomes a
migration problem with users in it.

## Goal

Ship the product as **Specola** — every identifier, path, string, and asset —
without breaking the event pipeline, without leaving orphaned macOS state, and
without back-dating the name into documents written before this decision.

*Specola*: the watchtower — and the name of the Vatican Observatory, the
*Specola Vaticana*. An owl already sits in the tray; the owl and the tower are
the same idea in two media, so the glyph is unchanged.

## Approach

Four layers, in order. Each is independently verifiable, and the one step that
can fail **silently** is isolated in the last.

```
① code + identifiers   →  cargo test green (139)      ← reversible, self-checking
② assets               →  logos swapped, screenshot recaptured
③ docs + this spec     →  forward-facing only
④ external             →  dir rename, GitHub rename, hooks, data move
                          ↑ the only layer that can break things quietly
```

**① Code and identifiers.** `productName`, bundle identifier, window title,
updater endpoint, Cargo package/lib/default-run names, npm name, CI release
name, runtime paths, localStorage keys, and all user-facing strings across nine
locales. Lockfiles are regenerated, not edited. The test suite is the gate:
`tests/fixtures/full_day.jsonl` and the path assertions in `app.rs` both carry
the old name, so an incomplete sweep fails loudly.

**② Assets.** The new logo lands as `assets/logo-{light,dark}.png`;
`public/logo.png` is a height-52 downscale of the **light-ink** variant, because
both the panel and the share card are dark-themed. The README screenshot is
**recaptured**, not renamed — the panel header inside it shows the old wordmark.

**③ Docs.** Forward-facing documents are rewritten. Dated specs, plans, and QA
notes are left exactly as they are, including their filenames. This spec is what
makes that honest rather than careless.

**④ External state.** The repository directory and the GitHub repo are renamed,
`~/.pervigil` is moved, and the Claude Code hooks are re-pasted. Sequencing here
is load-bearing — see *Decisions*.

## Decisions

- **No migration code.** With zero published releases the only migrant is the
  maintainer's own machine, and that is `mv ~/.pervigil ~/.specola` performed
  once with the app closed. (Alternative: read both paths, or a one-shot
  migration on first launch. Rejected — permanent complexity in `app.rs` to
  serve a population of one, on a day we can just move a directory.)

- **New bundle identifier, `com.ronaldoscotti.specola`.** Keeping
  `dev.pervigil.panel` would preserve macOS state but leave the old brand baked
  into the one string users can't see and we can never change later. Renaming now
  costs three one-time manual steps; renaming after release costs a broken update
  channel.

  The chosen form is reverse-DNS on a domain actually controlled —
  `ronaldoscotti.com` — rather than `dev.specola.panel`, which would assert a
  namespace under `specola.dev`, a domain registered to someone else.
  `io.github.ronaldoscotti.specola` was the alternative; rejected as longer and
  dependent on a host that lends the namespace rather than one we own.

- **Launch-at-login must be disabled *before* the rename.** The autostart
  registration is keyed to the old bundle identifier. Rename first and macOS
  keeps a Login Items entry pointing at an application that no longer exists —
  invisible to the new build, which cannot see or clean up the old identifier's
  registration.

- **`/Applications/Pervigil.app` is deleted by hand.** A changed identifier makes
  Specola a different application to macOS, not an upgrade. Both would run, both
  would poll, both would notify.

- **Hooks are re-pasted, and this is verified by observing a new event.**
  `hooks.rs::wired()` matches on `record` plus the event name rather than the
  brand — deliberate, so an install by absolute path is recognised regardless of
  where the binary lives. The same property is the trap here: after the directory
  moves, the existing hook command still contains `record` and the event name, so
  the panel reports **hooks installed** while pointing at a path that no longer
  exists. Green checkmark, empty panel, no error on any surface. The layer-④
  exit criterion is therefore not "hooks look installed" but "a new event landed
  in `~/.specola/events.jsonl`".

- **The signing key and updater keypair are untouched.** The minisign public key
  in `tauri.conf.json` and the Apple credentials in `.env` are independent of the
  product name. No key rotation, no re-notarization ceremony beyond a normal
  build.

- **Dated documents keep the old name, filenames included.** `docs/specs/2026-07-23-pervigil-design.md`,
  `docs/plans/2026-07-23-pervigil-plan.md`, and `docs/qa/*` describe a product
  that was called Pervigil when they were written. Rewriting them would produce a
  repo claiming a July 23 spec discussed "Specola" — precisely the tidied history
  `CLAUDE.md` forbids. Their paths are referenced from `CLAUDE.md`, which
  continues to resolve correctly.

- **The sweep is not a global find-and-replace.** Two classes of occurrence break
  under one:

  **Links to files that keep the old name.** Eleven references across `README.md`,
  `NEXT-STEPS.md`, `CLAUDE.md`, `design/README.md`, and `docs/method/*` point at
  `2026-07-23-pervigil-{plan,design}.md`. Those files are deliberately not
  renamed, so rewriting the paths produces eleven dead links.

  **Statements about what the word means.** `README.md:12`,
  `docs/method/01-context.md:82`, `design/README.md:17`, and
  `docs/brand/logo-prompts.md:6` all gloss *pervigil* as Latin for "ever-watchful;
  keeping watch through the whole night." Substituting the name yields a
  confidently false etymology — *specola* is a watchtower or observatory, not an
  adjective meaning watchful. Only the `README.md` gloss is rewritten, by hand;
  the rest sit in preserved documents.

  The sweep therefore runs with path exclusions, and a manual pass covers the
  glosses. `grep -rn "ever-watchful"` returning nothing outside preserved
  documents is part of the layer-③ exit check.

- **`design/`, `docs/brand/logo-prompts.md`, and `docs/method/01-context.md` are
  historical, not forward-facing.** `design/` is the visual direction locked at
  stage M2 and dated to it; `logo-prompts.md` records the prompts that actually
  generated the original mark. Both describe work done under the old name, so both
  stay, on the same reasoning as the dated specs. The owl they produced carries
  over unchanged, which is why no new prompt record is needed.

  `01-context.md` is the sharpest case, and was initially misfiled as
  forward-facing. It contains the **naming-research table from the original
  context-gathering** — the row reading "*Pervigil* · **Clean.** No GitHub/npm
  collision; only hit is a defunct 2012 IT co. **Chosen.**" Sweeping it produced a
  document asserting that *Specola* was researched and chosen in July 2023, against
  a collision that was never Specola's. That is a fabricated record of a decision,
  not a stale string. The file is preserved whole; the other two `docs/method/`
  documents are swept, because every occurrence in them is a present-tense
  statement about what the product is.

- **Product name is capitalized consistently as `Specola`.** The current copy is
  inconsistent — `"Open Pervigil"` in the tray, `"pervigil never edits it"` in the
  install card. The sweep normalizes it.

- **Portuguese takes the masculine article: `o Specola`.** Despite the `-a`
  ending and the Latin feminine, Brazilian developers inflect tool names toward
  the implied *o app* / *o programa* — `o Figma`, `o Prisma`, `o Kibana`. The
  same reasoning applies to the other Romance locales, where the name follows the
  article already used for the product rather than its Latin gender.

## What this is and isn't (honesty)

- **This is not a migration.** No compatibility layer reads `~/.pervigil`. A
  hypothetical existing user would find an empty panel and an install card, which
  is the correct behaviour for what is, to the operating system, a new
  application. That is acceptable **only** because the release count is zero, and
  this spec is the record of that being a deliberate trade rather than an
  oversight.

- **The GitHub rename leaves a redirect, not a guarantee.** GitHub 301-redirects
  the old repository URL, including clone and release paths. The updater endpoint
  is still repointed at the new URL rather than left to depend on that redirect,
  because a redirect is someone else's policy.

- **The app icon and tray icons do not change.** The owl is unchanged, so the 22
  generated tray variants and the 17 platform icons are untouched and
  `gen-tray-icons.py` is not run. The rename touches strings and identifiers; the
  artwork carried no wordmark.

- **No domain is registered, and none is needed.** `specola.dev`, `.app`, `.io`,
  `.com`, and `.ai` are all taken. Of what remains, the ones that read right for a
  developer tool renew badly — `.sh` at $62.98/yr, `.tools` at $38.48 — and the
  cheap first years are loss leaders against renewals up to 39× the intro price.
  The project's site will be **`specola.ronaldoscotti.com`**, on the Vercel
  account already serving `ronaldoscotti.com`. No page is built as part of this
  work — `design/` is the locked M2 mock, not a landing page, and a marketing site
  is separate scope. For a repo whose stated purpose is demonstrating how
  its author works, a subdomain of the author's own domain is not a budget
  compromise; a standalone domain would orphan the proof from the person it
  exists to prove.

- **Three OS-surface effects remain visually unverified on this machine**
  (tmux/iTerm2 raise, tray badge, notification banner), unchanged by this work
  and still honestly unverified.

- **The pre-rename baseline is 139 passing tests, 2 ignored** — measured, not
  quoted. `CLAUDE.md` currently says 120, which is stale; the docs layer corrects
  it. The gate for layer ① is 139, and a drop means the sweep missed a fixture or
  an assertion that carries the old name.

## Non-goals

- Registering a domain or publishing a website.
- Any change to behaviour, features, or the event schema.
- Rewriting historical specs, plans, or QA notes.
- Regenerating the app icon or tray glyph.
- Cutting a release. The first tagged release under the new name is separate work.

## Files

**Identifiers and config**
`src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (regenerated),
`package.json`, `package-lock.json` (regenerated), `.github/workflows/release.yml`

**Code**
`src-tauri/src/app.rs`, `src-tauri/src/config.rs`, `src-tauri/src/main.rs`,
`src-tauri/src/tray.rs`, `src-tauri/src/core/tray.rs`, `src-tauri/src/core/terminal.rs`,
`src-tauri/src/io/{hooks,record,scan,terminals,transcript}.rs`, `src-tauri/bin/record.rs`,
`src-tauri/tests/{full_day,ingestion}.rs`, `src-tauri/tests/fixtures/full_day.jsonl`,
`src/main.ts`, `index.html`, `scripts/screenshot.sh`

**Assets**
`assets/logo-{light,dark}.png` (replaced), `public/logo.png` (regenerated),
`assets/specola-screenshot.png` (recaptured, replaces `assets/pervigil-screenshot.png`)

**Docs**
`README.md`, `CLAUDE.md`, `NEXT-STEPS.md`,
`docs/method/{02-sota-alignment,README}.md`, and this spec

**Unchanged, deliberately**
`docs/specs/2026-07-23-pervigil-design.md`, `docs/plans/2026-07-23-pervigil-plan.md`,
`docs/plans/2026-07-27-tray-status-and-actions.md`,
`docs/specs/2026-07-27-tray-status-and-actions.md`, `docs/qa/*`,
`design/index.html`, `design/README.md`, `docs/brand/logo-prompts.md`,
`docs/method/01-context.md`,
`src-tauri/icons/*`, `assets/tray-owl.png`, `assets/app-icon-*.png`
