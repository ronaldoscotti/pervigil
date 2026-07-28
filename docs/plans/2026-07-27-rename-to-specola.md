# Rename to Specola — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the product as Specola — every identifier, path, string, and asset —
without breaking the event pipeline or leaving orphaned macOS state.

**Architecture:** Four layers, committed separately, in order. Layers ①–③ are
repo-local and reversible; layer ④ mutates state outside the repo (a bundle
identifier's macOS registrations, the user's hook configuration, the GitHub repo
name) and is the only one that can fail silently. Isolating it is the point of the
layering.

**Tech Stack:** Rust / Tauri v2, TypeScript / Vite, GitHub Actions, `sips` for image
resizing, `agent-browser` for the README screenshot.

**On testing:** No new tests are written. This change introduces no behaviour, and
inventing tests for a rename would be exactly the fabricated stage `CLAUDE.md`
forbids. The regression gate is the **existing suite at 139 passing / 2 ignored**,
measured before starting. `tests/fixtures/full_day.jsonl` and the path assertions in
`app.rs` both carry the old name, so an incomplete sweep fails loudly rather than
silently.

**Spec:** [`../specs/2026-07-27-rename-to-specola.md`](../specs/2026-07-27-rename-to-specola.md)

---

## Task 0: Record the baseline and commit the spec

**Files:**
- Create: `docs/specs/2026-07-27-rename-to-specola.md` (already written)
- Create: `docs/plans/2026-07-27-rename-to-specola.md` (this file)

- [ ] **Step 1: Confirm the working tree is clean apart from the new logos**

```bash
cd /Users/scotti/work/personal/pervigil
git status --porcelain
```

Expected: only `?? specola-dark.png`, `?? specola-light.png`, and the two new docs.

- [ ] **Step 2: Record the pre-rename test baseline**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "^test result:"
```

Expected: `132 passed`, `5 passed`, `2 passed` — **139 total, 2 ignored**. If this
number differs, stop and reconcile before touching anything; the whole plan uses it
as the gate.

- [ ] **Step 3: Commit the spec and plan**

```bash
cd /Users/scotti/work/personal/pervigil
git add docs/specs/2026-07-27-rename-to-specola.md docs/plans/2026-07-27-rename-to-specola.md
git commit -m "docs: spec and plan for the Specola rename"
```

---

## Task 1: Layer ① — sweep code and identifiers

**Files:**
- Modify: `src-tauri/src/{app,config,main,tray}.rs`, `src-tauri/src/core/{tray,terminal}.rs`,
  `src-tauri/src/io/{hooks,record,scan,terminals,transcript}.rs`
- Modify: `src-tauri/bin/record.rs`, `src-tauri/tests/{full_day,ingestion}.rs`,
  `src-tauri/tests/fixtures/full_day.jsonl`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`
- Modify: `src/main.ts`, `index.html`, `package.json`
- Modify: `.github/workflows/release.yml`, `scripts/screenshot.sh`
- Regenerate: `src-tauri/Cargo.lock`, `package-lock.json`

- [ ] **Step 1: Sweep the code and config files only**

Docs are deliberately excluded — they are layer ③, and some of them must not be
swept at all.

```bash
cd /Users/scotti/work/personal/pervigil
FILES=$(git ls-files \
  'src-tauri/src/*.rs' 'src-tauri/src/**/*.rs' 'src-tauri/bin/*.rs' \
  'src-tauri/tests/*.rs' 'src-tauri/tests/fixtures/*.jsonl' \
  'src-tauri/tauri.conf.json' 'src-tauri/Cargo.toml' \
  'src/main.ts' 'index.html' 'package.json' \
  '.github/workflows/release.yml' 'scripts/screenshot.sh')
echo "$FILES"
sed -i '' 's/pervigil/specola/g; s/Pervigil/Specola/g' $FILES
```

- [ ] **Step 2: Verify no code file still mentions the old name**

```bash
grep -rn -i "pervigil" $FILES; echo "exit=$?"
```

Expected: no output, `exit=1`.

- [ ] **Step 3: Set the bundle identifier by hand**

The sweep produced `dev.specola.panel`, which asserts a namespace under a domain
owned by someone else. Replace it with the reverse-DNS of a domain actually
controlled.

```bash
sed -i '' 's/"identifier": "dev.specola.panel"/"identifier": "com.ronaldoscotti.specola"/' \
  src-tauri/tauri.conf.json
grep -n '"identifier"' src-tauri/tauri.conf.json
```

Expected: `"identifier": "com.ronaldoscotti.specola"`

- [ ] **Step 4: Normalize the product name's casing in the ten locales**

The install-card string used lowercase `pervigil` mid-sentence while the tray used
`Pervigil`. The sweep preserved that inconsistency. Fix it, and take the Portuguese
article with it (`o Specola` — Brazilian devs inflect tool names toward the implied
*o app*, as in *o Figma*).

```bash
sed -i '' 's/— specola /— Specola /g; s/ specola / Specola /g; s/ specola\./ Specola./g' src/main.ts
grep -n "specola" src/main.ts
```

Expected: remaining lowercase hits are only URLs (`github.com/ronaldoscotti/specola`)
and `localStorage` keys (`specola.launches`, `specola.shareNudged`). Any lowercase
`specola` inside a translated sentence is a miss — fix it by hand.

- [ ] **Step 5: Regenerate the lockfiles**

Lockfiles are generated artifacts; editing them by hand invites a mismatch.

```bash
cd src-tauri && cargo build 2>&1 | tail -3 && cd ..
npm install --package-lock-only
grep -n '"name"' package.json package-lock.json | head -4
```

Expected: both report `specola`.

- [ ] **Step 6: Run the full suite — this is the gate**

```bash
cd src-tauri && cargo test 2>&1 | grep -E "^test result:"; cd ..
```

Expected: **139 passing, 2 ignored**, unchanged from Task 0 Step 2. A failure here
almost certainly means the fixture and an assertion disagree about the name.

- [ ] **Step 7: Lint and format**

```bash
cd src-tauri && cargo fmt --check && cargo clippy -- -D warnings 2>&1 | tail -5; cd ..
```

Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri src index.html package.json package-lock.json .github scripts
git commit -m "refactor: rename the crate, bundle, and runtime paths to Specola"
```

---

## Task 2: Layer ② — swap the assets

**Files:**
- Replace: `assets/logo-light.png`, `assets/logo-dark.png`
- Regenerate: `public/logo.png`
- Delete: `assets/pervigil-screenshot.png` → recapture as `assets/specola-screenshot.png`
- Modify: `README.md` (asset paths only; prose is layer ③)
- Delete: `specola-light.png`, `specola-dark.png` from the repo root

The app icon (`src-tauri/icons/*`) and all 22 tray icons stay untouched — the owl
carried over and neither carried a wordmark. `gen-tray-icons.py` is **not** run.

- [ ] **Step 1: Move the new logos into place**

Naming follows the existing convention: `-light` is light *ink*, for dark
backgrounds.

```bash
cd /Users/scotti/work/personal/pervigil
mv specola-light.png assets/logo-light.png
mv specola-dark.png  assets/logo-dark.png
sips -g pixelWidth -g pixelHeight assets/logo-light.png assets/logo-dark.png
```

Expected: both 1866×485.

- [ ] **Step 2: Regenerate the in-app logo**

`.brand-logo` is 26px tall in `src/styles.css`, so 52px is the 2× asset. The panel
and the share card are both dark (`#181a22`), so this comes from the light-ink file.

```bash
sips --resampleHeight 52 assets/logo-light.png --out public/logo.png
sips -g pixelWidth -g pixelHeight public/logo.png
```

Expected: 200×52.

- [ ] **Step 3: Point the README at the new asset paths**

```bash
sed -i '' 's|assets/pervigil-screenshot.png|assets/specola-screenshot.png|g; s|alt="Pervigil"|alt="Specola"|g' README.md
grep -n "assets/" README.md
```

- [ ] **Step 4: Recapture the README hero**

The old screenshot shows the old wordmark inside the panel header, so it is
regenerated, not renamed. The mock data inside `screenshot.sh` was already updated
in layer ① (the mock project row now reads `specola`).

```bash
git rm assets/pervigil-screenshot.png
bash scripts/screenshot.sh
sips -g pixelWidth -g pixelHeight assets/specola-screenshot.png
```

Expected: the file exists. **Open it and confirm the panel header shows the new
wordmark** — an automated check cannot see this.

- [ ] **Step 5: Commit**

```bash
git add -A assets public README.md
git commit -m "assets: swap in the Specola logos and recapture the README hero"
```

---

## Task 3: Layer ③ — docs

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `NEXT-STEPS.md`,
  `docs/method/{01-context,02-sota-alignment,README}.md`
- Not touched: `docs/specs/2026-07-23-*`, `docs/plans/2026-07-23-*`,
  `docs/plans/2026-07-27-tray-*`, `docs/specs/2026-07-27-tray-*`, `docs/qa/*`,
  `design/*`, `docs/brand/logo-prompts.md`

- [ ] **Step 1: Sweep the forward-facing docs**

```bash
cd /Users/scotti/work/personal/pervigil
DOCS="README.md CLAUDE.md NEXT-STEPS.md docs/method/01-context.md docs/method/02-sota-alignment.md docs/method/README.md"
sed -i '' 's/pervigil/specola/g; s/Pervigil/Specola/g' $DOCS
```

- [ ] **Step 2: Restore the links to files that keep the old name**

Step 1 rewrote eleven paths pointing at documents deliberately **not** renamed.
Undo exactly those.

```bash
sed -i '' 's|2026-07-23-specola-|2026-07-23-pervigil-|g' $DOCS
grep -rn "2026-07-23-" $DOCS
```

Expected: every hit reads `2026-07-23-pervigil-plan.md` or
`2026-07-23-pervigil-design.md`.

- [ ] **Step 3: Verify every internal link still resolves**

```bash
grep -roh "docs/[a-z]*/[0-9-a-z]*\.md" $DOCS | sort -u | while read -r f; do
  [ -f "$f" ] || echo "BROKEN: $f"
done; echo "link check done"
```

Expected: no `BROKEN:` lines.

- [ ] **Step 4: Rewrite the etymology by hand**

The sweep turned "*pervigil* (Latin) — ever-watchful" into a confidently false claim
about a different word. *Specola* is a watchtower, not an adjective.

In `README.md:12`, replace the gloss with:

```markdown
> *specola* (Latin/Italian) — a watchtower; the raised place you keep watch from.
> Also the *Specola Vaticana*, the Vatican Observatory.
```

In `docs/method/01-context.md`, replace the naming paragraph with the same sense:
the name is a place you watch from, and the owl in the tray is the same idea in
another medium.

```bash
grep -rn -i "ever-watchful" $DOCS; echo "exit=$?"
```

Expected: no output, `exit=1`. Any remaining hit is a false etymology.

- [ ] **Step 5: Correct the stale test count in CLAUDE.md**

`CLAUDE.md` claims 120 tests; the measured figure is 139. Left alone, a document
whose stated virtue is being status-accurate would be inaccurate.

```bash
sed -i '' 's/120 tests green/139 tests green/' CLAUDE.md
grep -n "tests green" CLAUDE.md
```

- [ ] **Step 6: Confirm the preserved documents were not touched**

```bash
git status --porcelain docs/specs docs/plans docs/qa design docs/brand
```

Expected: no output beyond the two new files from Task 0.

- [ ] **Step 7: Commit**

```bash
git add -A README.md CLAUDE.md NEXT-STEPS.md docs/method
git commit -m "docs: rename forward-facing docs to Specola and correct the test count"
```

---

## Task 4: Layer ④ — external state

**Nothing in this task is verifiable by the test suite.** The order matters; each
step leaves state the next one depends on.

- [ ] **Step 1: Disable launch-at-login while the old app can still do it**

Open the running Pervigil panel → Settings → turn **launch at login OFF**.

The registration is keyed to `dev.pervigil.panel`. Skip this and macOS keeps a
Login Items entry pointing at a deleted app, which the new build cannot see or
clean up.

- [ ] **Step 2: Quit the app and remove the old bundle**

```bash
osascript -e 'quit app "Pervigil"' 2>/dev/null || true
rm -rf /Applications/Pervigil.app
ls -d /Applications/Pervigil.app 2>&1
```

Expected: `No such file or directory`. A changed identifier makes Specola a
different app to macOS, not an upgrade — leaving both means two trays and duplicate
notifications.

- [ ] **Step 3: Move the event log and settings**

```bash
mv ~/.pervigil ~/.specola
ls -la ~/.specola && wc -l ~/.specola/events.jsonl
```

Expected: `config.json`, `events.jsonl`, `terminals/`, and the event count matching
what was there before (514 at the time of writing).

- [ ] **Step 4: Rename the GitHub repository**

```bash
cd /Users/scotti/work/personal/pervigil
gh repo rename specola --yes
git remote set-url origin https://github.com/ronaldoscotti/specola.git
git remote -v
```

GitHub 301-redirects the old URL, but the updater endpoint was repointed in layer ①
rather than left to depend on that redirect.

- [ ] **Step 5: Rename the working directory**

```bash
cd /Users/scotti/work/personal
mv pervigil specola
cd specola && pwd
```

**This is the step that breaks the hooks.** Everything after it is the repair.

- [ ] **Step 6: Rebuild the record shim at its new path**

```bash
cd src-tauri && cargo build 2>&1 | tail -3 && cd ..
ls -l src-tauri/target/debug/record
```

- [ ] **Step 7: Re-point the hooks**

```bash
sed -i '' 's|/personal/pervigil/|/personal/specola/|g' ~/.claude/settings.json
grep -n "record" ~/.claude/settings.json
```

Expected: four commands, all under `/Users/scotti/work/personal/specola/`.

- [ ] **Step 8: Prove an event actually lands — the real exit criterion**

`hooks.rs::wired()` matches on `record` plus the event name, not the brand. It will
report **installed** against a path that no longer exists. A green checkmark proves
nothing here; only a new line does.

```bash
BEFORE=$(wc -l < ~/.specola/events.jsonl)
```

Now start a new Claude Code session in any project and submit one prompt, then:

```bash
AFTER=$(wc -l < ~/.specola/events.jsonl)
echo "before=$BEFORE after=$AFTER"
tail -2 ~/.specola/events.jsonl
```

Expected: `after > before`. **If the count did not move, the hooks are broken —
stop and fix before continuing**, regardless of what the panel reports.

- [ ] **Step 9: Build signed and install**

```bash
bash scripts/build-signed.sh
```

Then install the produced bundle and launch it. Verify by eye:
- the tray shows the owl,
- the panel header shows the new wordmark,
- the window title reads Specola,
- Settings → launch at login can be re-enabled under the new identifier.

macOS will ask for notification permission again — expected under a new bundle
identifier, not a bug.

- [ ] **Step 10: Commit and push**

```bash
git add -A
git commit -m "chore: point the toolchain at the renamed repository"
git push -u origin HEAD
```

---

## Task 5: Close out

- [ ] **Step 1: Full-repo check for survivors**

```bash
cd /Users/scotti/work/personal/specola
grep -rn -i "pervigil" . --exclude-dir=.git --exclude-dir=node_modules \
  --exclude-dir=target --exclude-dir=dist
```

Expected: hits **only** in `docs/specs/2026-07-23-*`, `docs/plans/2026-07-23-*`,
`docs/plans/2026-07-27-tray-*`, `docs/specs/2026-07-27-tray-*`, `docs/qa/*`,
`design/*`, `docs/brand/logo-prompts.md`, and the rename spec itself. Anything else
is a miss.

- [ ] **Step 2: Point the subdomain (optional, no code impact)**

Add `specola.ronaldoscotti.com` on the Vercel account already serving
`ronaldoscotti.com`. No landing page is built here — `design/` is the locked M2
mock, not a marketing site, and that is separate scope.

- [ ] **Step 3: Open the PR**

```bash
gh pr create --title "Rename Pervigil to Specola" --body-file docs/specs/2026-07-27-rename-to-specola.md
```
