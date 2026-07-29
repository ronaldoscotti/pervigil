# Changelog

Notable changes per release. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- A session replaced by `/resume` stayed in the panel forever. Starting `claude`
  and resuming another conversation left the first session id with no further
  events and a still-live pid, so neither age nor liveness retired it. `fold`
  now keeps only the newest-active session per process.
- The span filter scoped the lane, the cost readout, and the token count, but
  never the session list — every span rendered the same rows, some days old.
- Many sessions showed no cost. `claude-opus-5` was missing from the price
  table, and model ids carrying a release date (`claude-haiku-4-5-20251001`)
  missed their bare alias.
- A settings change the disk refused was reported as saved. The write error is
  returned through the command layer and the panel says so.
- The tray kept a stale menu when `set_menu` failed, because the shown
  signature advanced regardless.

### Added

- `IngestError` names the three ways a hook payload can fail to become an event.
- Property tests over generated event sequences: `fold` determinism, `timeline`
  tiling, prefix monotonicity, and session clock bounds.
- A frontend test suite (Vitest) over the state-to-render path and the ten-locale
  lookup, with a CI job that gates merges.

### Changed

- `core` is its own crate, so the "core never imports io, platform, or Tauri"
  rule is a dependency list rather than a review comment.
- `app.rs` split into `core::span`, `core::notify`, `commands.rs`, and the
  state it kept.
- The toolchain is pinned in `rust-toolchain.toml`; `cargo audit` runs in CI.
- Documentation states what the process actually did — stage 9 is aspirational,
  two CI jobs cannot fail, and the skills the method depends on are named with
  their marketplaces and versions.

## [0.1.1] — 2026-07-25

### Fixed

- The Intel macOS bundle built on a runner GitHub had retired, so the release
  workflow failed at the last step.

### Added

- Contributor guide, issue templates, and download instructions.
- Architecture diagram, rendered with mermaid.

## [0.1.0] — 2026-07-24

First release. A cross-platform tray panel showing every Claude Code session
across your projects and which ones are blocked on you.

- Event-log ingestion through a bundled `record` hook shim; no daemon, no socket.
- The waiting lane, per-session cost from transcripts, and a day summary.
- Click-to-focus with honest degradation: tmux pane, iTerm2 tab, VS Code window,
  and a clipboard floor that always works.
- Tray badge, native notifications, pin and dismiss, hidden projects.
- Ten UI languages with RTL, launch-at-login, single instance.
- Signed and notarized macOS build; signed, auto-updating releases from CI.

[Unreleased]: https://github.com/ronaldoscotti/specola/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/ronaldoscotti/specola/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ronaldoscotti/specola/releases/tag/v0.1.0
