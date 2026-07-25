# Pervigil — next steps (scratch, do not commit)

State as of tonight: **M0–M9 done**, M10 mostly done. **Nothing merged to `main`,
no git remote, no PRs.** All green (**114 tests**, clippy clean). Owl branding +
Lora/Space Mono type shipped.

**This pass (branch `feat/branding`):** the lane bug (0a) and bundle identifier
(0b) are fixed and committed; plan checkboxes are up to date. Everything left in
this file genuinely needs you — a clean-machine signed launch, the demo capture,
the tmux/iTerm2/banner/tray on-screen confirmations, and the remote+PR+merge
process. I did not touch those.

## 0. Known bugs to fix (found while recording the demo)

### 0a. Activity lane reads 100% "waiting on you" (wrong)  ✅ FIXED (commit 18f3d5d)
Fixed with TDD (4 new fixture tests, 114 green). A `WaitingOnYou` state now decays
to idle after 30 min of silence, so a session that fired a Notification and then
died without a `Stop` no longer paints the whole lane. Chose time-decay over
liveness/window exclusion because `timeline` is pure (no fs, no pid) and decay is
uniform across every offender. Original diagnosis kept below.

- **Symptom:** the `LAST 4 HOURS` lane is fully amber ("100% esperando você") while the
  headline correctly shows **0 waiting**. Screenshot: 2026-07-24 demo.
- **Root cause:** `store::timeline` / `state_after` treats a `Notification` as a
  **permanent** `WaitingOnYou` state — it never expires. A session that fires a
  Notification and then **ends without a `Stop`** (killed terminal, or the session just
  closed) stays "waiting" forever in the timeline. Confirmed offenders in the log that
  fired a Notification with no follow-up event: `6df5328f` (~5.8h ago, i.e. before the
  window → waiting at `from` → paints the *whole* window), `3addcdde`, `8a303211`,
  `10420f85`. The lane aggregates **every** session (incl. dead/old ones the *list*
  hides via liveness + the scan-window), so one stale waiter turns the lane 100% amber
  even though no visible/live session is waiting — hence lane 100% vs headline 0.
- **Fix ideas:** give the lane's waiting state an end — e.g. expire `WaitingOnYou` after
  N minutes of no events; and/or exclude dead sessions (liveness) or sessions whose
  transcript is outside the window from the lane aggregate, so the lane and the list
  agree on what counts. Add a fixture test: a Notification with no follow-up must not
  make the whole lane waiting.

### 0b. Bundle identifier ends in `.app`  ✅ FIXED (commit 2171101)
Renamed `dev.pervigil.app` → `dev.pervigil.panel`. Verified nothing keys off the
identifier — all state lives under `~/.pervigil`. Original note below.

- `tauri.conf.json` `identifier` is `dev.pervigil.app` — Tauri warns this conflicts with
  the `.app` bundle extension on macOS. Change to e.g. `dev.pervigil.panel` (or
  `app.pervigil.desktop`). **Do it before any public release** — the identifier is hard
  to change once people have the app installed (macOS keys prefs/permissions to it).
  Note: this also changes the `~/.pervigil` vs identifier-keyed state, so verify nothing
  keys off the identifier.

## 1. Ship the signed app (done, pending a clean-machine launch)
- [x] `APPLE_ID` fixed in `.env` (was the Team ID).
- [x] Signed + notarized build via `bash scripts/build-signed.sh` — built 18:03 from
      HEAD (so the lane fix + `dev.pervigil.panel` are baked in). Notary status
      **Accepted**; `spctl` = "accepted, source=Notarized Developer ID"; staple valid.
      Output: `src-tauri/target/release/bundle/{macos/pervigil.app,dmg/pervigil_0.1.0_aarch64.dmg}`.
- [ ] Confirm it launches from a *truly* clean machine (another Mac) with no
      Gatekeeper warning. `spctl` here already assesses it as notarized, which is the
      strong signal — this is the belt-and-suspenders step.

## 2. The demo (needs a real desktop — you)
- [ ] Record the ~30s GIF: a session enters **waiting on you** → click the row →
      window snaps to it. Shot list: panel with a waiting session up top → click →
      terminal/editor raises.
- [ ] The flashy focus tiers (tmux pane, iTerm2 tab) only show if those terminals are
      installed — your box is VS Code + clipboard. Install tmux/iTerm2 first if you
      want them in the video.
- [ ] Drop the GIF + a real screenshot into the README (hero already uses the logo).

## 3. Real code, not yet visually confirmed (marked honestly in docs/qa/)
- [ ] tmux / iTerm2 on-screen window raise.
- [ ] macOS notification banner appearing.
- [ ] Tray badge (the waiting count in the menu bar).
      → All wired + unit-tested; just never captured on screen here. Confirm while
      recording the demo and update the QA notes.

## 4. Deliberately skipped (decided)
- Usage-limit gauges (5h/weekly): endpoint unreachable, no credentials file. Left as
  the `$` cost footer. Revisit only if Claude Code exposes limits locally.

## 5. Process / merge (the biggest loose end)
- [ ] Add a git remote (GitHub).
- [ ] Decide PR shape: keep M6–M10 as per-milestone PRs; **flatten** tonight's
      QA/fix/branding branches (`fix/ui-qa-1`, `feat/your-turn-state`,
      `fix/fold-any-event`, `feat/i18n`, `feat/branding`, `fix/cost-window-cursor`)
      into 1–2 "polish + fixes" PRs rather than shipping them individually.
- [ ] Run the method's stage-7 `code-review` skill on the diff before merging.
- [ ] Merge to `main`.

## 6. Housekeeping
- [x] Update stale plan/QA checkboxes (`docs/plans/2026-07-23-pervigil-plan.md`):
      the `record`-shim bundling and the M9 live hook detection are marked done
      (commit 89255db). The VS Code raise checkbox stays unchecked on purpose — its
      line covers *every* tier and tmux/iTerm2 are still unverified here.
- [ ] Delete this file — it's scratch.

## 7. Borrow list — from Handy (`cjpais/Handy`, 27k⭐, MIT, Tauri v2 + Rust)
Studied for inspiration, not lifted wholesale. Ranked by fit. Nothing here is
started — each is a future spec→plan cycle, gated as usual.

### 7a. Auto-update (biggest gap — we have no update story)
Cheap in Tauri: `tauri-plugin-updater` + `tauri.conf.json`
`bundle.createUpdaterArtifacts: true` + a minisign pubkey + an endpoint pointing
at `releases/latest/download/latest.json`. `tauri-action` generates `latest.json`
and signatures from `TAURI_SIGNING_PRIVATE_KEY`. Handy checks **on demand**
(emits a `check-for-updates` event gated on a setting) — no background thread,
which matches our no-daemon ethos.

### 7b. Move signing into CI (could retire the M10 signing blocker)
Today `build-signed.sh` needs *this* machine. Handy signs + notarizes in a
reusable `build.yml`: base64 `APPLE_CERTIFICATE` → keychain import →
`security find-identity` → feed `APPLE_SIGNING_IDENTITY` to `tauri-action`, all
gated on a `sign-binaries` flag. Same `APPLE_*` secrets we already hold, moved to
GitHub Secrets. Output: signed + notarized artifacts **and** the updater manifest,
straight from CI, no local desktop. Pairs naturally with 7a — one feature:
"signed, auto-updating releases from CI."

### 7c. Cross-platform release matrix
Handy builds 7 targets (mac arm/intel, win x64/arm, linux deb/rpm/appimage) via a
draft-release job + the reusable build. Ours are "allowed-to-fail" CI jobs with no
release automation — this is the upgrade path to real Windows/Linux bundles.

### 7d. Tray state icons, theme-aware
`change_tray_icon(state)` swaps the asset per state **and** per OS theme (reads the
Windows registry for the actual taskbar theme, not the app theme). Applies to our
still-unconfirmed tray badge: a distinct **idle vs waiting** icon beats a count
nobody can read at menu-bar size.

### 7e. Two one-plugin wins for an always-watching panel
- `tauri-plugin-autostart` — launch on login.
- `tauri-plugin-single-instance` — a second launch focuses the existing window
  instead of spawning another.

### 7f. Anti-pattern to resist (the V2 lesson, confirmed)
Handy's `src/components/settings/` has **30+** toggle components. That is exactly
the gold-plating our scope discipline forbids. Their scope ≠ ours. Keep settings
short and opinionated.

**Recommendation:** 7a + 7b as one spec→plan cycle; 7d and 7e as smaller
follow-ups. 7c falls out of 7b for free.

## Branch stack (base → tip)
```
main
 └ feat/m6-live-ui → m7-focuser → m8-config → m9-hook-install → m10-packaging
     └ fix/ui-qa-1 → feat/your-turn-state → fix/fold-any-event → feat/i18n
         → fix/cost-window-cursor → feat/branding   ← latest
```
