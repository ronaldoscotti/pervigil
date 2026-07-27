# Spec — Tray status and actions

Status: **draft** (review gate: this PR). Follows the pipeline in
[`../method/README.md`](../method/README.md).

## Problem

The tray is the only part of Pervigil that is always on screen, and it is the
least finished. Four things are wrong, and the fourth invalidates the other three.

1. **The icon is the app icon.** `TrayIconBuilder` takes
   `app.default_window_icon()` — full-colour artwork, no template treatment. It
   breaks from every other icon in the macOS menu bar.
2. **There is no menu.** Left click shows and focuses the panel; right click does
   nothing. Every action costs a window.
3. **The badge does not exist on Windows.** `badge()` calls `tray.set_title(count)`.
   Per the Tauri docs, `set_title` is *unsupported on Windows*. The count is not
   unverified there — it is absent by API, while the code reads as cross-platform.
4. **There is no "panel closed" state.** No `ExitRequested` handler, no
   `CloseRequested` handler, no hide affordance anywhere. Closing the last window
   ends the process and takes the tray with it. Pervigil is a background monitor
   that cannot run in the background.

A fifth problem is latent today and becomes live the moment (4) is fixed. Both
the tray badge and the native notifications are side effects of the `snapshot`
command (`app.rs:496-499`), and the only thing that calls it is the webview's
`setInterval(poll, 1000)` (`src/main.ts:1419`). While the window cannot be
hidden, that timer always runs. Once closing the panel hides it, tray state and
notifications inherit the lifetime of a hidden WebView's timer — which the OS is
free to throttle or suspend. Adding hide-on-close without moving that clock would
break notifications, a shipped feature.

## Goal

With the panel closed, the tray answers *is something blocked on me* and lets you
act on it — the same count on all three platforms, and each blocked session
reachable in one click.

## Approach

Four layers, from the foundation up. The first two are prerequisites, not features.

```
RunEvent::ExitRequested { code: None } ──► prevent_exit()   survive a closed window
WindowEvent::CloseRequested            ──► prevent_close + hide
RunEvent::Reopen                       ──► show panel       the Dock re-opens it
        │
        ▼
panel visible  ──► webview poll (1s) ──┐
panel hidden   ──► Rust ticker  (1s) ──┴──► App::snapshot ──► tray_view() ──► TrayView
                                              │                  (pure)         │
                                              └──► notifications                ▼
                                                    icon asset + menu, rebuilt on change
```

**Lifecycle.** `prevent_exit` keeps the core thread alive with no windows — the
Tauri docs name this as its purpose — but it must be **guarded on
`code.is_none()`**. `RunEvent::ExitRequested` carries `Some(code)` when the exit
came from `AppHandle::exit`, and `None` only for user-interaction exits. An
unconditional `prevent_exit()` would block the tray's own `Quit`, which is the
only real exit once the window merely hides. (The updater's relaunch needs no
guard from us: `prevent_exit` already ignores itself when the code is
`RESTART_EXIT_CODE`.)

**Exactly one clock at a time.** A Rust-side ticker calls `App::snapshot` while
the panel is hidden, and stops when it is shown; the webview's existing poll
keeps driving everything while the panel is visible. The ticker pins
`Span::Today` — the panel's span lives in the frontend and is unreachable from
Rust, and it is the panel's clock anyway.

Tauri v2 has **no window-visibility event** — `WindowEvent` carries `Resized`,
`Moved`, `CloseRequested`, `Destroyed`, `Focused`, `ScaleFactorChanged`,
`DragDrop`, `ThemeChanged` and nothing else. So the invariant cannot be observed;
it has to be *owned*. One pair of helpers, `show_panel` / `hide_panel`, performs
the window call and flips the ticker together, and every site routes through
them. There are five once this lands, and the first is pre-existing plumbing that
reads as unrelated:

- the single-instance handler (`lib.rs:20-25`)
- the tray left-click handler (`lib.rs:55-58`)
- the new `CloseRequested` hide
- the new `RunEvent::Reopen` show
- the new `Open Pervigil` menu item

Missing one gives two clocks — the exact `targets` corruption below — or none, a
frozen tray. The helpers exist so that "one clock" is a property of the code
rather than a rule someone has to remember.

This is a hard requirement, not a tidiness preference. `App::snapshot` is not a
read — it replaces `self.targets` wholesale (`app.rs:286`) and advances the
notification `seen` state through `self.notify` (`app.rs:274`). Two tickers
running at 1 Hz with different spans would fight over `targets`: a session the
panel shows under a wider span, but which a narrower tray scan misses, would lose
its focus target, and a click on that still-visible row would degrade silently to
the clipboard floor with no `cd`. One clock removes the interleaving instead of
managing it, and leaves the panel-visible path byte-for-byte as it is today.

The waiting count itself is span-independent — `WaitingOnYou` only ever comes
from `store::fold`, and transcript-derived sessions always yield `Idle` — so
which clock is running never changes what the tray says is blocked. Only the cost
figure would vary, and it does not: see the summary decision below.

**Decision, pure.** `tray_view(&Snapshot) -> TrayView` takes no Tauri types, no
clock, no I/O — the same shape as `store::timeline`. It picks the icon key, the
tooltip, the summary line, and the session items. The Tauri side applies the
result; it does not decide anything.

**Icon.** Count 0 is the bare glyph; count *n* carries a badge; above 9 the badge
reads `9+`. Assets are pre-rendered from one SVG source by
`scripts/gen-tray-icons.py`, so the asset count is a number rather than a cost.
The badged icon is *wider* than the bare one — `9+` crammed into a 16pt square is
a smudge.

**One raster per state, at high resolution — no @1x/@2x pair.** `set_icon` takes
a single `Image` and there is no scale-factor hook for a tray icon: the macOS
backend rescales whatever it is handed to an 18pt height, and the Windows backend
builds one `HICON` from the RGBA. A second density would be an asset no code
could ever select. The generated PNGs are embedded with `include_bytes!` rather
than shipped as bundle resources, so there is no path where the packaged app
finds an empty icon directory that `tauri dev` found full. Swaps go through
`set_icon_with_as_template`, which sets image and template flag atomically —
`set_icon` followed by `set_icon_as_template` flickers on macOS, and the atomic
form degrades to a plain `set_icon` on Windows and Linux, so it costs nothing.

**Menu.**

```
3 waiting · $4.20 today      (disabled)
──────────────
pervigil — fix the axis ticks
comercial-api — migrate to valkey
──────────────
Open Pervigil
Quit
```

Session items call the existing `app::focus` path — the same code the panel's
rows already use. The list is capped at nine, in the panel's own order
(`store::sort`, pins first); the summary line above always states the true count,
so a cap never hides the number. The menu is rebuilt only when its content
changes, keyed by a structural signature over the waiting set and the waiting
count; `src/main.ts:852` already applies that idea to the panel's rows.

**The cost does not enter the signature.** It is an `f64` that moves on almost
every tick during active work, and rebuilding a macOS menu closes it under the
user's cursor. A menu that shuts while you are reading it is a worse failure than
a cost line one rebuild stale — nobody watches a dollar figure at 1 Hz, and it
refreshes with the next real change.

The tooltip is `Pervigil — 3 waiting`, or `Pervigil — nothing waiting` at zero.

## Decisions

- **Count rendered into the icon, not beside it.** `set_title` is unsupported on
  Windows, so a text badge is macOS/Linux-only by construction. Drawing the count
  into the image is the only form that reaches all three platforms.
  (Alternative: keep `set_title` and let Windows go without. Rejected — it makes
  the product's central signal a macOS privilege and hides that fact in a no-op.)
- **Pre-rendered assets, not runtime rasterisation.** No new dependency, no image
  code on a surface nobody can inspect. Runtime compositing would put pixel logic
  precisely where this project cannot look — see the honesty section. A generator
  script keeps the assets cheap to regenerate when the artwork changes.
  (Alternative: compose digits at runtime with the `image` crate. Rejected — buys
  unlimited counts, which nobody needs, and pays in unverifiable code.)
- **The tray summary is always today, whatever the panel is showing.** The
  panel's span is a user filter that defaults to `4h` and changes under you; the
  tray has no filter UI and no room to explain one. Since either clock may be the
  one running, the figure cannot come from the snapshot's span-scoped `cost`
  without changing meaning when the panel opens.

  Filtering the existing usage by today does **not** work: `Scanner::scan` skips
  any transcript whose mtime predates the span floor (`io/scan.rs:100`), so under
  the default `4h` span this morning's work is never read at all and "today"
  would silently shrink. Instead `Scanner::scan` takes **two floors** — sessions
  keep `from`, usage uses `min(from, start_of_day(now))`. One directory walk, one
  set of incremental reads, two windows applied to two outputs. It costs a wider
  file set in the same pass, not a second pass, and the session list the panel
  renders is untouched.
- **macOS uses a template image; Windows and Linux use light/dark pairs.**
  `icon_as_template` is macOS-only; there the system tints the alpha mask and the
  icon is correct in both themes for free. Elsewhere a white silhouette vanishes
  on a light Windows 11 taskbar, so the variant is chosen from
  `WebviewWindow::theme()` at startup and re-chosen on `WindowEvent::ThemeChanged`,
  defaulting to the dark-background variant when the theme is unknown. This is an
  approximation and is labelled as one: on Windows the taskbar's theme is a
  separate system setting from the app theme Tauri reports, and they can disagree.
- **Two states, not three.** Waiting and not-waiting. A `working` state would
  change constantly while asking nothing of the user, competing for attention
  with the one state that does.
- **Left click keeps opening the panel where we control it.** Tauri defaults
  `show_menu_on_left_click` to `true`, so adding a menu would silently take the
  left click away; it is set to `false` on macOS and Windows. On Linux the
  setting is unsupported and the left click is not ours.
- **The Dock icon stays.** `RunEvent::Reopen` fires when it is clicked; the
  handler shows the panel, so the Dock becomes a second door rather than a dead
  click. (Alternative: `ActivationPolicy::Accessory`. Rejected — it removes the
  menu bar along with the Dock icon, costing `Cmd+Q` to solve a problem four
  lines already solve. `set_dock_visibility(false)` remains available later as a
  one-line preference; it is deliberately not taken now.)
- **`Open Pervigil` is in the menu because of Linux.**
  `set_show_menu_on_left_click` is unsupported there, so left-click behaviour is
  not ours to choose. Without the menu item, a Linux user has no way to open the
  panel.
- **A tray click that falls to the clipboard says so.** `App::focus` degrades to
  copying the resume command when it cannot raise a window. The panel reports
  that with a toast; a hidden panel has no toast, and a silent copy is
  indistinguishable from a dead click. The tray path fires a native notification
  instead, reusing the plumbing in `fire()`. When the user has notifications
  switched off, it **shows the panel** rather than staying quiet — the toggle
  silences ambient alerts, not the feedback for something the user just clicked,
  and falling back to silence would restore the exact failure this bullet exists
  to remove.

## What this is and isn't (honesty)

- **Verified on macOS:** the icon treatment, the menu, the lifecycle, and the
  count, by using the build.
- **Verified everywhere:** `tray_view` is pure and fixture-tested — count 0 maps
  to the bare icon, 12 maps to `9+`, the list caps at nine while the summary
  still reads twelve, and the menu signature changes only when the waiting set or
  the summary does. What is tested is the decision, not the drawing.
- **Not verified on Windows or Linux:** how the icon actually looks in those
  shells, and whether the light/dark selection matches the system theme in
  practice — on Windows it is known to be an approximation, per the decision
  above. `CLAUDE.md` already records the tray badge as visually unverified on
  this machine; this feature widens that area. It is stated here and in the PR,
  and the per-platform table is written from the Tauri documentation rather than
  from observation.
- **Degradation is explicit:** the tooltip is skipped on Linux, where it is
  unsupported. Nothing pretends to work where it does not.
- **One guard is belt-and-braces, and QA should not hunt for it.**
  `ExitRequested { code: None }` is emitted only when the last window is
  destroyed, and the `CloseRequested` handler prevents that destruction — so in
  normal use the guarded `prevent_exit` branch will not fire at all. It is the
  documented pattern and it costs four lines, so it stays; it is recorded here so
  nobody spends an afternoon trying to reach it.
- **`Cmd+Q` is unaffected.** It is not a window close, so the `CloseRequested`
  hide never sees it; it still quits, exactly as today. Worth one QA tick because
  it is the first thing anyone reading "closing now hides" will worry about.
- **A minimised panel is a hidden panel we do not notice.** The ticker keys off
  our own show/hide helpers, and minimising goes through neither — so the webview
  keeps the clock while the OS is free to throttle its timers. Accepted: the
  window is small, always-on-top by default, and minimising it is not a habit
  this panel invites. Recorded rather than fixed, and it is the first thing to
  revisit if the tray is ever reported as stale.

| | macOS | Windows | Linux |
|---|---|---|---|
| Left click | opens panel | opens panel | opens menu (not our choice) |
| Right click | menu | menu | menu |
| Tooltip | yes | yes | skipped — unsupported |
| Icon | template | light/dark pair | light/dark pair |

## Non-goals

A third `working` icon state; a notifications toggle, a settings shortcut, or a
span switch in the menu; hiding the Dock icon; any change to the panel itself.

## Files

- `src-tauri/src/lib.rs` — lifecycle handlers, the ticker, tray menu
  construction, icon application.
- `src-tauri/src/app.rs` — `tray_view` and its `TrayView` type; `badge()` is
  replaced by it, and `snapshot` sheds its side effects.
- `assets/tray.svg` — the source glyph, to be designed in the plan (new).
- `src-tauri/icons/tray/` — generated assets: bare and `1`…`9`, `9+`, one
  high-resolution raster each, in light and dark variants for the non-macOS
  targets (new). Committed, and embedded with `include_bytes!`.
- `scripts/gen-tray-icons.py` — SVG → PNG generator, rasterising with `cairosvg`
  (new). Python tooling is precedented by `scripts/screenshot-frame.py`, but that
  script uses Pillow, which cannot rasterise SVG — `cairosvg` and its native
  libcairo are a **new dependency of the generator only**. Because the PNGs are
  committed, it is needed by whoever changes the artwork and by nobody else: not
  by the build, not by CI, not by a contributor running the app.
- `docs/specs/2026-07-27-tray-status-and-actions.md` — this file.
