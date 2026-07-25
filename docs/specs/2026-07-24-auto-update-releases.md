# Spec — Auto-updating signed releases from CI

Status: **draft** (review gate: this PR). Follows the pipeline in
[`../method/README.md`](../method/README.md).

## Problem

Two constraints, both traced to "releases live on the maintainer's laptop":

1. **Signing needs a specific machine.** `scripts/build-signed.sh` sources local
   `.env` credentials and runs on one Mac. No other contributor — and no CI — can
   cut a signed, notarized build. This is the M10 signing blocker.
2. **Users have no update path.** A shipped `.app` is frozen. There is no channel
   to tell an installed copy that a newer version exists.

## Goal

Cut a signed, notarized, cross-platform release with one `git tag`, and have an
installed copy notice and install the update itself — with no server to run.

## Approach

Two halves that meet at one file, `latest.json`:

- **CI release workflow** (`.github/workflows/release.yml`) — on a `v*` tag,
  build the matrix, sign + notarize (macOS), generate the updater artifacts, and
  publish a GitHub release. `tauri-action` does the bundling, signing, and
  `latest.json` generation; the maintainer's machine is no longer in the loop.
- **In-app updater** — `tauri-plugin-updater` checks the release's `latest.json`
  on launch. If a newer, signature-valid version exists, the About panel offers
  to install it; `tauri-plugin-process` relaunches into the new binary.

```
git tag v0.2.0 && git push --tags
        │
        ▼
release.yml  ──►  tauri-action  ──►  signed .app/.dmg/.msi/.deb  +  latest.json
        │                                      (GitHub Release)
        ▼
installed copy  ──►  updater.check(latest.json)  ──►  "Install update" → relaunch
```

## Decisions

- **GitHub Releases as the update server.** The endpoint is
  `releases/latest/download/latest.json`. No host to run, no daemon — the same
  ethos as the event-log core. (Alternative: a self-hosted endpoint. Rejected:
  nothing to gain, a server to keep alive.)
- **Signature-verified updates.** A minisign keypair (`tauri signer generate`)
  signs each artifact. The **private key is a GitHub secret**
  (`TAURI_SIGNING_PRIVATE_KEY`); the **public key is committed** in
  `tauri.conf.json`. An update that doesn't verify against the public key is
  refused — a compromised release channel can't push a malicious binary.
- **Trigger is tags + manual dispatch, never fork PRs.** Fork `pull_request`
  runs receive no secrets, so signing/notarization only happens on trusted
  triggers. This is a hard security boundary for a public repo, not a preference.
- **macOS gates; Windows/Linux are `continue-on-error`.** Same honest posture as
  `ci.yml`: macOS is the tested target, the others are architecturally supported
  and allowed to fail until someone owns them.
- **Update is offered, never forced.** The launch check is silent; a found update
  surfaces as an *Install* affordance in About and a toast. No auto-download, no
  surprise relaunch. A watch instrument shouldn't restart itself under you.
- **Check on launch, not on a timer.** The core case is a panel reopened often
  enough; a background poll is complexity the MVP doesn't need. Revisit if a
  long-lived session misses updates in practice.

## What this is and isn't (honesty)

- **Verified in this PR:** the crate builds with both plugins; the pure core and
  its tests stay green; the frontend type-checks and builds; the updater config is
  valid; the launch-check path is exercised in QA (degrades silently with no
  endpoint reachable).
- **Not verified until the first tagged run:** the end-to-end release —
  keychain import, notarization, `latest.json` generation, and the real
  download-install-relaunch — cannot be exercised without pushing a real tag
  against the CI secrets. This is inherent to release pipelines and is flagged in
  the PR rather than asserted as done. The workflow follows the Tauri v2
  distribution docs and a proven reference (cjpais/Handy).
- **TDD note:** this feature is configuration, plugin registration, and CI —
  there is little pure logic to red-green (the plugin owns version comparison and
  signature checks). Per the TDD skill's own carve-out for configuration, the
  checks left behind are the build/type/test gates above, not fabricated unit
  tests around a plugin boundary.

## Files

- `.github/workflows/release.yml` — the release pipeline (new).
- `src-tauri/Cargo.toml` — `tauri-plugin-updater`, `tauri-plugin-process`.
- `src-tauri/src/lib.rs` — register both plugins.
- `src-tauri/tauri.conf.json` — `createUpdaterArtifacts`, `plugins.updater`.
- `src-tauri/capabilities/default.json` — `updater:default`, `process:allow-restart`.
- `package.json` — `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`.
- `src/main.ts`, `index.html`, `src/styles.css` — the About-panel update affordance
  and the launch check, localized in all ten languages.
