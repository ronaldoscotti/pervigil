# Contributing to Specola

The most valuable contribution right now is **running Specola on Windows or Linux
and telling us what happened**. macOS is the tested target; the other two are
architecturally supported but have never run on a maintainer's machine. Open a
[platform test report](https://github.com/ronaldoscotti/specola/issues/new?template=platform_test.yml)
— partial reports are welcome.

For anything that isn't a bug, use
[Discussions](https://github.com/ronaldoscotti/specola/discussions).

## Build it

You need a [Rust toolchain](https://rustup.rs) (stable) and Node 18+.

```bash
git clone https://github.com/ronaldoscotti/specola.git && cd specola
npm install
npm run tauri dev      # run it
npm run tauri build    # or build a bundle
```

The `record` shim is a second binary that ships inside the app bundle. It is staged
automatically before `tauri dev` and `tauri build` — you shouldn't need to run
`npm run sidecar` by hand.

### Linux system dependencies

Debian/Ubuntu:

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev \
  librsvg2-dev patchelf libgtk-3-dev
```

Other distributions: see [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/).

### Wiring up the hooks

Specola reads an event log written by Claude Code hooks. Open **Settings** in the
panel and paste the shown snippet into `~/.claude/settings.json`. Specola never
edits that file for you. The install card disappears once the hooks are detected.

## Before you open a pull request

Everything runs from `src-tauri/`:

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI gates merges on the macOS job. The Linux and Windows jobs run but are allowed to
fail — they are signal, not a gate, until those platforms have real coverage.

## How work happens here

This repo is a deliberate record of an AI-assisted development method, and the
history is meant to be inspectable. Two consequences for contributors:

**Non-trivial changes get a design conversation first.** Open an issue or a
discussion describing the problem before writing the implementation. Big features
follow spec → plan → TDD → review, with the artifacts landing in `docs/`. Bug fixes
and small changes skip most of that — there's less to review, not a relaxed bar.

**Tests come first for anything with logic.** The heart of the project is
`fold(events, now, prefs) -> sessions`: a pure function with no clock, no
filesystem, and no GUI, so behavior is provable with fixtures instead of a UI
harness. It lives in `specola-core`, a crate that depends on neither Tauri nor the
`io`/`platform` modules — so the boundary is a compile error, not a review comment.
Keep it that way; platform effects live behind traits at the edges.

## Conventions

- **Commits**: Conventional Commits, imperative mood, English.
  `fix: keep the tray badge in sync after a dismiss`
  The subject becomes a line in `CHANGELOG.md` — release-please builds the release
  from these, so write it for someone reading the changelog. `feat:` bumps the minor
  version, `fix:` the patch.
- **Comments**: the exception, not the habit, and always in English. Docblocks on
  public functions; a non-obvious decision or known limit; an opaque regex or
  algorithm. Never section banners, narration of the next line, or commented-out
  code. Try renaming or extracting first.
- **Claim only what the code earns.** If a platform blocks a capability, say so and
  fall back — never pretend. If an effect is written but unverified on real
  hardware, label the issue `needs-verification` and say so in the docs. A repo
  that honestly shows work at a gate beats one that lies about being finished.

## License

By contributing you agree that your contributions are licensed under the
[MIT License](LICENSE).
